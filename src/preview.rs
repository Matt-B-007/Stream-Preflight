//! Viewer preview: puts OBS's original frames (before encoding) next to the
//! same moments from the test recording (after encoding, at your stream
//! settings), and scores how closely they match.
//!
//! Decoding the recording uses ffmpeg if it's installed. Preflight only
//! reads the recording; it never changes it.

use crate::livetest::PreviewShot;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Clone, Debug)]
pub struct PreviewPair {
    /// Seconds into the recording.
    pub t_sec: f32,
    pub original: PathBuf,
    pub viewer: Option<PathBuf>,
    /// Picture match in dB (PSNR of brightness). Higher is closer.
    pub match_db: Option<f32>,
    /// False when no recorded frame lined up closely enough to compare fairly.
    pub aligned: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Preview {
    pub pairs: Vec<PreviewPair>,
    pub ffmpeg_missing: bool,
    pub error: Option<String>,
    /// The viewer side was re-created by encoding the test recording at the
    /// stream's settings (the recording itself was higher quality).
    pub simulated: Option<String>,
}

/// The stream's settings, for re-creating what viewers get.
#[derive(Clone, Debug)]
pub struct StreamSim {
    pub bitrate_kbps: u32,
    pub width: u32,
    pub height: u32,
    pub fps: f32,
    /// The recording used different (usually higher-quality) settings.
    pub recording_differs: bool,
    /// The recording is high quality (CQP/CRF 22 or better, or 20+ Mbps):
    /// a low match can't come from compression, so it's misalignment.
    pub recording_high_quality: bool,
}

impl Preview {
    /// The pair with the lowest match: what busy moments look like.
    pub fn worst(&self) -> Option<&PreviewPair> {
        self.pairs.iter().filter(|p| p.match_db.is_some()).min_by(|a, b| a.match_db.partial_cmp(&b.match_db).unwrap())
    }
    #[allow(dead_code)]
    pub fn avg_db(&self) -> Option<f32> {
        let v: Vec<f32> = self.pairs.iter().filter_map(|p| p.match_db).collect();
        if v.is_empty() { None } else { Some(v.iter().sum::<f32>() / v.len() as f32) }
    }
}

/// What a picture-match number means to viewers.
pub fn meaning(db: f32) -> (&'static str, &'static str) {
    if db >= 40.0 {
        ("Practically identical", "Viewers see what you see.")
    } else if db >= 36.0 {
        ("Very close", "Fine detail is a little softer. Most viewers won't notice.")
    } else if db >= 32.0 {
        ("Noticeably softer", "Small text, grass and other fine detail smear, especially in motion.")
    } else {
        ("Blocky", "Busy scenes break into blocks. More bitrate or a lower resolution would help.")
    }
}

pub fn ffmpeg_path() -> Option<PathBuf> {
    let exe = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
    if let Some(paths) = std::env::var_os("PATH") {
        for d in std::env::split_paths(&paths) {
            let p = d.join(exe);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    let mut extra: Vec<PathBuf> = Vec::new();
    #[cfg(windows)]
    {
        if let Ok(l) = std::env::var("LOCALAPPDATA") {
            extra.push(PathBuf::from(l).join("Microsoft/WinGet/Links/ffmpeg.exe"));
        }
        if let Ok(pf) = std::env::var("ProgramFiles") {
            extra.push(PathBuf::from(pf).join("ffmpeg/bin/ffmpeg.exe"));
        }
        extra.push(PathBuf::from("C:/ffmpeg/bin/ffmpeg.exe"));
    }
    #[cfg(not(windows))]
    {
        extra.push(PathBuf::from("/usr/bin/ffmpeg"));
        extra.push(PathBuf::from("/usr/local/bin/ffmpeg"));
    }
    extra.into_iter().find(|p| p.is_file())
}

fn hidden(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // no console window
    }
    cmd
}

pub fn build(shots: &[PreviewShot], recording: Option<&str>, fps: f32, sim: Option<&StreamSim>) -> Preview {
    let mut out = Preview::default();
    for s in shots {
        out.pairs.push(PreviewPair { t_sec: s.rec_ms as f32 / 1000.0, original: s.path.clone(), viewer: None, match_db: None, aligned: false });
    }
    if shots.is_empty() {
        out.error = Some("No preview frames were grabbed. Keep recording for a few seconds after the bar fills next time.".into());
        return out;
    }
    let Some(rec) = recording.filter(|r| Path::new(r).is_file()) else {
        out.error = Some("The test recording wasn't found, so only OBS's original frames can be shown.".into());
        return out;
    };
    let Some(ff) = ffmpeg_path() else {
        out.ffmpeg_missing = true;
        return out;
    };
    let fps = if fps > 1.0 { fps } else { 60.0 };
    let simulate = sim.filter(|s| s.recording_differs && s.bitrate_kbps > 0 && s.width > 0);
    if let Some(s) = simulate {
        out.simulated = Some(format!("Both pictures come from your test recording: the left is the recording itself, the right is the same moment re-encoded at your stream settings ({}p{} · {} kbps), which is what viewers get.", s.height, s.fps.round() as u32, s.bitrate_kbps));
    }
    for (i, pair) in out.pairs.iter_mut().enumerate() {
        let res = match_frame(&ff, rec, pair, fps, i, sim.map(|s| s.recording_high_quality).unwrap_or(false));
        let res = match (res, simulate) {
            // Re-encode the recorded moment at the stream's settings, and show
            // the recording's own frame as the original: both sides are then
            // the same moment, even when OBS's screenshot couldn't be lined up.
            (Ok(m), Some(s)) => match simulate_frame(&ff, rec, &m, s, i) {
                Ok((p, db, rec_frame)) => {
                    pair.original = rec_frame;
                    Ok((p, db, true))
                }
                Err(e) => {
                    log::warn!("Viewer preview frame {i}: re-encoding failed ({e}); showing the recording");
                    Ok((m.path, m.db, m.aligned))
                }
            },
            (Ok(m), None) => Ok((m.path, m.db, m.aligned)),
            (Err(e), _) => Err(e),
        };
        match res {
            Ok((path, db, aligned)) => {
                pair.viewer = Some(path);
                pair.aligned = aligned;
                pair.match_db = if aligned { Some(db) } else { None };
            }
            Err(e) => {
                log::warn!("Viewer preview frame {i}: {e}");
                out.error.get_or_insert(e);
            }
        }
    }
    log::info!("Viewer preview built: {:?}", out.pairs.iter().map(|p| p.match_db).collect::<Vec<_>>());
    out
}

/// The recorded frame that best matches OBS's original.
struct Matched {
    path: PathBuf,
    db: f32,
    aligned: bool,
    /// Seconds into the recording.
    at: f32,
    w: u32,
    h: u32,
}

/// Finds the recorded frame that matches OBS's original. OBS's recording
/// position can be off by a fraction of a second, so 1.5 seconds of frames
/// are compared at low resolution first, then the best one is decoded at
/// full size and scored. A match only counts when it clearly stands out from
/// the other frames (fast camera moves make every frame look alike or none).
fn match_frame(ff: &Path, rec: &str, pair: &PreviewPair, fps: f32, i: usize, high_quality_rec: bool) -> Result<Matched, String> {
    let orig = image::open(&pair.original).map_err(|e| format!("couldn't open the original frame: {e}"))?.to_rgb8();
    let (w, h) = orig.dimensions();
    let sw = 320u32;
    let sh = ((h as f32 * sw as f32 / w as f32) as u32 / 2 * 2).max(2);
    let small = image::imageops::resize(&orig, sw, sh, image::imageops::FilterType::Triangle);
    let start = (pair.t_sec - 0.5).max(0.0);
    let n = (fps * 1.5).ceil() as usize;
    let run = |vf: String, frames: usize| -> Result<Vec<u8>, String> {
        let o = hidden(Command::new(ff).args(["-v", "error", "-ss", &format!("{start:.3}"), "-i", rec, "-vf", &vf, "-frames:v", &frames.to_string(), "-fps_mode", "passthrough", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"]))
            .stdin(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| format!("couldn't run ffmpeg: {e}"))?;
        if o.stdout.is_empty() {
            let err = String::from_utf8_lossy(&o.stderr);
            return Err(format!("ffmpeg couldn't read the recording ({})", err.lines().next().unwrap_or("no frames")));
        }
        Ok(o.stdout)
    };
    // 1. Score every frame at low resolution.
    let lo = run(format!("scale={sw}:{sh}:flags=area"), n)?;
    let size_s = (sw * sh * 3) as usize;
    let scores: Vec<f32> = lo.chunks_exact(size_s).map(|f| psnr(small.as_raw(), f, sw as usize, sh as usize)).collect();
    let (idx, small_db) = scores.iter().copied().enumerate().max_by(|a, b| a.1.partial_cmp(&b.1).unwrap()).ok_or("no frames decoded")?;
    // The best frame away from its immediate neighbours.
    let runner_up = scores.iter().enumerate().filter(|(j, _)| (*j as i64 - idx as i64).abs() > 2).map(|(_, d)| *d).fold(f32::MIN, f32::max);
    let peak = small_db - runner_up.max(0.0);
    // 2. Decode that one frame at full size.
    let full = run(format!("select=eq(n\\,{idx}),scale={w}:{h}:flags=bicubic"), 1)?;
    let size = (w * h * 3) as usize;
    if full.len() < size {
        return Err("couldn't decode the matching frame".into());
    }
    let frame = &full[..size];
    let db = psnr(orig.as_raw(), frame, w as usize, h as usize);
    // Different moments, not compression: the match is weak, doesn't stand
    // out, or is low even though the recording is high quality.
    let clear = small_db >= 22.0 && (peak >= 0.8 || small_db >= 32.0 || scores.len() < 6);
    let aligned = clear && !(high_quality_rec && db < 30.0);
    log::info!("Preview frame {i}: best match {idx} frames after {start:.2}s, {small_db:.1} dB small (peak +{peak:.1}), {db:.1} dB full, aligned={aligned}");
    let img = image::RgbImage::from_raw(w, h, frame.to_vec()).ok_or("bad frame size")?;
    let path = crate::util::dirs().previews.join(format!("viewer-{i}.png"));
    img.save(&path).map_err(|e| format!("couldn't save the frame: {e}"))?;
    Ok(Matched { path, db, aligned, at: start + idx as f32 / fps, w, h })
}

/// Re-creates what viewers see: encodes two seconds of the (high-quality)
/// recording around the matched moment at the stream's size, frame rate and
/// bitrate, then compares that frame with the recording's own.
fn simulate_frame(ff: &Path, rec: &str, m: &Matched, s: &StreamSim, i: usize) -> Result<(PathBuf, f32, PathBuf), String> {
    let dir = crate::util::dirs().previews.clone();
    // Keep the recording's frame: it becomes the "original" side.
    let rec_frame = dir.join(format!("recorded-{i}.png"));
    std::fs::copy(&m.path, &rec_frame).map_err(|e| format!("couldn't keep the recorded frame: {e}"))?;
    let clip = dir.join(format!("sim-{i}.mkv"));
    let start = (m.at - 1.0).max(0.0);
    let b = format!("{}k", s.bitrate_kbps);
    let vf = format!("scale={}:{}:flags=lanczos,fps={}", s.width, s.height, s.fps.round().max(1.0) as u32);
    let o = hidden(Command::new(ff).args([
        "-v", "error", "-y", "-ss", &format!("{start:.3}"), "-i", rec, "-t", "2", "-an", "-vf", &vf, "-c:v", "libx264", "-preset", "medium", "-b:v", &b, "-maxrate", &b, "-bufsize", &b, "-x264-params", "nal-hrd=cbr", "-pix_fmt", "yuv420p",
    ]).arg(&clip))
    .stdin(Stdio::null())
    .stderr(Stdio::piped())
    .output()
    .map_err(|e| format!("couldn't run ffmpeg: {e}"))?;
    if !o.status.success() || !clip.is_file() {
        return Err(String::from_utf8_lossy(&o.stderr).lines().next().unwrap_or("encode failed").to_string());
    }
    let offset = m.at - start;
    let dec = hidden(Command::new(ff).args(["-v", "error", "-ss", &format!("{offset:.3}")]).arg("-i").arg(&clip).args(["-frames:v", "1", "-vf", &format!("scale={}:{}:flags=bicubic", m.w, m.h), "-f", "rawvideo", "-pix_fmt", "rgb24", "-"]))
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("couldn't run ffmpeg: {e}"))?;
    let _ = std::fs::remove_file(&clip);
    let size = (m.w * m.h * 3) as usize;
    if dec.stdout.len() < size {
        return Err("couldn't decode the re-encoded frame".into());
    }
    let viewer = &dec.stdout[..size];
    let recorded = image::open(&rec_frame).map_err(|e| e.to_string())?.to_rgb8();
    let db = psnr(recorded.as_raw(), viewer, m.w as usize, m.h as usize);
    let img = image::RgbImage::from_raw(m.w, m.h, viewer.to_vec()).ok_or("bad frame size")?;
    let path = dir.join(format!("viewer-{i}.png"));
    img.save(&path).map_err(|e| e.to_string())?;
    log::info!("Preview frame {i}: re-encoded at stream settings, {db:.1} dB");
    Ok((path, db, rec_frame))
}

/// Brightness PSNR, sampling every other pixel. Capped at 60 dB.
pub fn psnr(a: &[u8], b: &[u8], w: usize, h: usize) -> f32 {
    let luma = |p: &[u8], i: usize| 0.299 * p[i] as f64 + 0.587 * p[i + 1] as f64 + 0.114 * p[i + 2] as f64;
    let mut sum = 0.0f64;
    let mut n = 0u64;
    for y in (0..h).step_by(2) {
        for x in (0..w).step_by(2) {
            let i = (y * w + x) * 3;
            let d = luma(a, i) - luma(b, i);
            sum += d * d;
            n += 1;
        }
    }
    if n == 0 {
        return 0.0;
    }
    let mse = sum / n as f64;
    if mse < 1e-6 {
        return 60.0;
    }
    ((10.0 * (255.0f64 * 255.0 / mse).log10()) as f32).min(60.0)
}

/// Opens the recording in the system's video player.
pub fn open_in_player(path: &str) {
    #[cfg(windows)]
    let _ = hidden(Command::new("cmd").args(["/C", "start", "", path])).spawn();
    #[cfg(not(windows))]
    let _ = Command::new("xdg-open").arg(path).spawn();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn psnr_identical_and_different() {
        let a = vec![100u8; 4 * 4 * 3];
        assert_eq!(psnr(&a, &a, 4, 4), 60.0);
        let b = vec![110u8; 4 * 4 * 3];
        let d = psnr(&a, &b, 4, 4);
        assert!(d > 27.0 && d < 29.0, "{d}");
    }

    /// End to end with real ffmpeg: a high-quality "recording" and an OBS
    /// screenshot of one moment; the viewer side is re-encoded at 6000 kbps.
    #[test]
    fn simulated_viewer_preview() {
        let Some(ff) = ffmpeg_path() else { return };
        let dir = std::env::temp_dir().join(format!("pf-prev-{}", fastrand::u32(..)));
        std::fs::create_dir_all(&dir).unwrap();
        let rec = dir.join("rec.mkv");
        let ok = Command::new(&ff).args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=size=1280x720:rate=60", "-t", "4", "-c:v", "libx264", "-crf", "14", "-preset", "ultrafast"]).arg(&rec).status().map(|s| s.success()).unwrap_or(false);
        if !ok {
            return;
        }
        let shot = dir.join("orig.png");
        Command::new(&ff).args(["-v", "error", "-y", "-ss", "2.0", "-i"]).arg(&rec).args(["-frames:v", "1"]).arg(&shot).status().unwrap();
        let shots = vec![PreviewShot { path: shot, rec_ms: 2000, ..Default::default() }];
        let sim = StreamSim { bitrate_kbps: 1500, width: 960, height: 540, fps: 60.0, recording_differs: true, recording_high_quality: true };
        let p = build(&shots, Some(rec.to_str().unwrap()), 60.0, Some(&sim));
        assert!(p.simulated.is_some(), "{:?}", p.error);
        let pair = &p.pairs[0];
        assert!(pair.aligned, "same moment found");
        let db = pair.match_db.unwrap();
        assert!(db > 20.0 && db < 50.0, "re-encoded frame scored {db}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
