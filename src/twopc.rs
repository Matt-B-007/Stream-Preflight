//! Two-PC setups: a gaming PC sends its picture to a streaming PC (capture
//! card or NDI), and OBS runs on the streaming PC. Preflight reads only what
//! OBS reports about its capture sources (never video), and the OBS address
//! is never written into reports or logs.

use serde::Serialize;
use serde_json::Value;

/// OBS input kinds that bring in another computer's picture.
pub fn is_capture_kind(kind: &str) -> bool {
    matches!(kind, "dshow_input" | "v4l2_input" | "av_capture_input" | "av_capture_input_v2" | "decklink-input" | "ndi_source" | "ndi_filter" | "teleport-source")
}

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct CaptureInfo {
    /// "card" (USB/PCIe capture card) or "network" (NDI and similar).
    pub via: String,
    pub kind: String,
    pub res: Option<(u32, u32)>,
    pub fps: Option<f32>,
    /// "MJPEG", "NV12", "YUY2", "P010"...
    pub format: String,
    /// "partial", "full" or "" (the device's default).
    pub range: String,
    /// Audio sync offset in ms, when the source carries audio.
    pub audio_offset_ms: Option<i64>,
    /// Visible in the current scene.
    pub in_scene: bool,
}

fn fourcc(v: u64) -> String {
    let b = (v as u32).to_le_bytes();
    if b.iter().all(|c| c.is_ascii_alphanumeric() || *c == b' ') { String::from_utf8_lossy(&b).trim().to_string() } else { String::new() }
}

impl CaptureInfo {
    pub fn from_settings(kind: &str, st: &Value, audio_offset_ms: Option<i64>, in_scene: bool) -> CaptureInfo {
        let via = if kind.contains("ndi") || kind.contains("teleport") { "network" } else { "card" }.to_string();
        let mut c = CaptureInfo { via, kind: kind.to_string(), audio_offset_ms, in_scene, ..Default::default() };
        match kind {
            "dshow_input" => {
                // Windows: "1920x1080", frame interval in 100 ns units, format name.
                if let Some(r) = st["resolution"].as_str() {
                    let mut p = r.split('x').filter_map(|n| n.trim().parse::<u32>().ok());
                    if let (Some(w), Some(h)) = (p.next(), p.next()) {
                        c.res = Some((w, h));
                    }
                }
                if let Some(fi) = st["frame_interval"].as_i64().filter(|f| *f > 0) {
                    c.fps = Some((10_000_000.0 / fi as f64) as f32);
                }
                c.format = match st["video_format"].as_i64() {
                    Some(0) | None => String::new(),
                    Some(100) | Some(101) => "ARGB".into(),
                    Some(200) => "I420".into(),
                    Some(201) => "NV12".into(),
                    Some(202) => "YV12".into(),
                    Some(300) => "Y800".into(),
                    Some(400) => "YVYU".into(),
                    Some(401) => "YUY2".into(),
                    Some(402) => "UYVY".into(),
                    Some(403) => "HDYC".into(),
                    Some(500) => "MJPEG".into(),
                    Some(600) => "H264".into(),
                    Some(700) => "P010".into(),
                    Some(x) => format!("format {x}"),
                };
                c.range = st["color_range"].as_str().unwrap_or("").to_lowercase();
            }
            "v4l2_input" => {
                // Linux: resolution and frame rate are packed integers.
                if let Some(r) = st["resolution"].as_i64().filter(|r| *r > 0) {
                    // OBS packs (width << 32) | height, like the frame rate. Very old
                    // builds used 16 bits; accept both.
                    c.res = Some(if r >> 32 > 0 { ((r >> 32) as u32, (r & 0xFFFF_FFFF) as u32) } else { ((r >> 16) as u32, (r & 0xFFFF) as u32) });
                }
                if let Some(f) = st["framerate"].as_i64().filter(|f| *f > 0) {
                    let (num, den) = ((f >> 32) as f64, (f & 0xFFFF_FFFF) as f64);
                    if num > 0.0 {
                        c.fps = Some((den / num) as f32);
                    }
                }
                if let Some(pf) = st["pixelformat"].as_u64() {
                    c.format = fourcc(pf);
                }
                c.range = match st["color_range"].as_i64() {
                    Some(1) => "partial".into(),
                    Some(2) => "full".into(),
                    _ => String::new(),
                };
            }
            _ => {}
        }
        c
    }

    pub fn describe(&self) -> String {
        let res = self.res.map(|(w, h)| format!("{w}x{h}")).unwrap_or_else(|| "device default".into());
        let fps = self.fps.map(|f| format!(" at {f:.0} fps")).unwrap_or_default();
        let fmt = if self.format.is_empty() { String::new() } else { format!(", {}", self.format) };
        format!("{} · {res}{fps}{fmt}", if self.via == "network" { "Network source (NDI)" } else { "Capture card" })
    }
}

/// localhost, 127.x, ::1 or this computer's name: OBS runs here.
pub fn is_local_host(host: &str) -> bool {
    let h = host.trim().trim_start_matches('[').trim_end_matches(']').to_lowercase();
    if h.is_empty() || h == "localhost" || h == "::1" || h.starts_with("127.") {
        return true;
    }
    let me = sysinfo::System::host_name().unwrap_or_default().to_lowercase();
    !me.is_empty() && h == me
}

/// Which computer this copy of Preflight is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    One,
    Gaming,
    Streaming,
}

impl Role {
    pub fn from_str(s: &str) -> Role {
        match s {
            "gaming" => Role::Gaming,
            "streaming" => Role::Streaming,
            _ => Role::One,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Role::One => "one",
            Role::Gaming => "gaming",
            Role::Streaming => "streaming",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Role::One => "One PC for everything",
            Role::Gaming => "Gaming PC (OBS runs on another PC)",
            Role::Streaming => "Streaming PC (the game runs on another PC)",
        }
    }
}

/// Looks like a two-PC setup: OBS shows a capture card or NDI source in the
/// scene and no game capture, or OBS is on another computer.
pub fn looks_two_pc(captures: &[CaptureInfo], uses_game_capture: bool, obs_remote: bool) -> bool {
    obs_remote || (captures.iter().any(|c| c.in_scene) && !uses_game_capture)
}

/// Which PC a fix belongs to, for the "On your gaming PC" label.
pub fn pc_of(place: crate::analysis::Place, role: Role) -> Option<&'static str> {
    use crate::analysis::Place;
    match (role, place) {
        (Role::One, _) => None,
        (_, Place::Game) => Some("On your gaming PC"),
        (_, Place::Obs) => Some("On your streaming PC"),
        (Role::Gaming, Place::Pc) => Some("On your gaming PC"),
        (Role::Streaming, Place::Pc) => Some("On your streaming PC"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn windows_capture_card_settings() {
        let c = CaptureInfo::from_settings("dshow_input", &json!({"resolution": "1920x1080", "frame_interval": 166667, "video_format": 500, "color_range": "Full"}), Some(0), true);
        assert_eq!(c.res, Some((1920, 1080)));
        let old = CaptureInfo::from_settings("v4l2_input", &json!({"resolution": (1280i64 << 16) | 720}), None, true);
        assert_eq!(old.res, Some((1280, 720)));
        assert!((c.fps.unwrap() - 60.0).abs() < 0.1);
        assert_eq!(c.format, "MJPEG");
        assert_eq!(c.range, "full");
        assert_eq!(c.via, "card");
    }

    #[test]
    fn linux_capture_card_settings() {
        // The real value from a user's report: 1920 << 32 | 1080.
        let res = (1920i64 << 32) | 1080;
        let fr = (1i64 << 32) | 60;
        let yuyv = u32::from_le_bytes(*b"YUYV") as u64;
        let c = CaptureInfo::from_settings("v4l2_input", &json!({"resolution": res, "framerate": fr, "pixelformat": yuyv, "color_range": 1}), None, true);
        assert_eq!(c.res, Some((1920, 1080)));
        let old = CaptureInfo::from_settings("v4l2_input", &json!({"resolution": (1280i64 << 16) | 720}), None, true);
        assert_eq!(old.res, Some((1280, 720)));
        assert!((c.fps.unwrap() - 60.0).abs() < 0.1);
        assert_eq!(c.format, "YUYV");
        assert_eq!(c.range, "partial");
    }

    #[test]
    fn local_and_remote_hosts() {
        assert!(is_local_host("localhost"));
        assert!(is_local_host("127.0.0.1"));
        assert!(!is_local_host("192.168.1.20"));
    }
}
