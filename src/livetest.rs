// SPDX-License-Identifier: GPL-3.0-or-later
//! The full test. The user presses Record in OBS themselves; we notice the
//! recording start through the WebSocket connection and measure the PC and
//! OBS once a second until they stop recording. We never control OBS.

use crate::hardware::{Monitor, Sample};
use crate::obs::{self, Client};
use serde::Serialize;
use serde_json::json;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const TARGET_SECONDS: f32 = 120.0;
/// Frames grabbed for the viewer preview, after measuring is done.
pub const PREVIEW_SHOTS: usize = 4;
pub const CANCELLED: &str = "Stopped watching.";
const MAX_SECONDS: f32 = 600.0;
const WAIT_FOR_RECORD_SECONDS: f32 = 3600.0;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum Phase {
    Idle,
    Connecting,
    WaitingForRecord,
    Measuring,
    Finished,
    Failed(String),
}

#[derive(Clone, Debug, Default, Serialize, serde::Deserialize)]
#[serde(default)]
pub struct TestResult {
    pub started_at: i64,
    pub seconds: f32,
    pub with_obs: bool,
    pub samples: Vec<Sample>,
    pub cpu_avg: f32,
    pub cpu_max: f32,
    pub gpu_avg: Option<f32>,
    pub gpu_max: Option<f32>,
    pub gpu_temp_max: Option<f32>,
    pub cpu_temp_max: Option<f32>,
    pub gpu_throttle: Option<String>,
    pub gpu_source: String,
    pub obs_cpu_avg: Option<f32>,
    pub render_skipped: u64,
    pub render_total: u64,
    pub output_skipped: u64,
    pub output_total: u64,
    pub mic_db: Option<f32>,
    pub desktop_db: Option<f32>,
    pub game_running: Option<String>,
    /// When the game isn't one Preflight knows: the program using the GPU most.
    pub game_guess: Option<String>,
    pub hogs: Vec<crate::hardware::Hog>,
    pub own_cpu_avg: Option<f32>,
    pub own_ram_mb: Option<f32>,
    #[serde(skip)]
    pub recording_path: Option<String>,
    /// The user's own recording that was already running when the test began
    /// (recording while streaming). Used only to show the viewer preview; it's
    /// never treated as a test recording, so it's never offered for deletion.
    #[serde(skip)]
    pub live_rec_path: Option<String>,
    pub recording_size: Option<u64>,
    /// Original (before-encoding) frames from OBS, with their position in
    /// the recording.
    #[serde(skip)]
    pub preview_shots: Vec<PreviewShot>,
    pub stream_fps: Option<f32>,
    /// The game's own average FPS, when it could be measured.
    pub game_fps: Option<f32>,
    pub game_fps_source: String,
    /// This was a 1-minute quick re-test.
    pub quick: bool,
    /// When the game last saved its settings file, as of the end of the test.
    pub settings_saved: Option<i64>,
    /// How busy the video encoder chip was.
    pub enc_avg: Option<f32>,
    pub enc_max: Option<f32>,
    /// Frames each output dropped during the test (stream: network; recording: disk).
    pub outputs: Vec<OutputStat>,
    /// OBS was streaming during the test (for example a Twitch bandwidth test).
    pub streamed: bool,
    /// The OBS settings this test ran with.
    #[serde(default)]
    pub obs_settings: Option<crate::obs::EncSettings>,
    /// GPU encodes seen during the test (program, codec, size, FPS).
    #[serde(default)]
    pub encodes_seen: Vec<crate::encoders::EncSession>,
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, serde::Deserialize)]
#[serde(default)]
pub struct OutputStat {
    pub label: String,
    pub dropped: u64,
    pub total: u64,
}

#[derive(Clone, Debug, Default)]
pub struct PreviewShot {
    /// Milliseconds into the recording.
    pub rec_ms: u64,
    pub path: std::path::PathBuf,
}

pub struct TestState {
    pub phase: Phase,
    pub elapsed: f32,
    pub last: Option<Sample>,
    pub result: Option<TestResult>,
    /// Preview frames grabbed so far.
    pub shots: usize,
    /// Seconds this test measures for.
    pub target: f32,
    /// What OBS is doing while Preflight waits (for the checklist).
    pub rec_active: bool,
    pub stream_active: bool,
}

pub struct TestHandle {
    pub state: Arc<Mutex<TestState>>,
    pub stop: Arc<AtomicBool>,
}

impl TestHandle {
    pub fn phase(&self) -> Phase {
        self.state
            .lock()
            .map(|s| s.phase.clone())
            .unwrap_or(Phase::Idle)
    }
}

pub struct TestConfig {
    pub host: String,
    pub port: u16,
    pub password: Option<String>,
    /// Run without OBS (system measurements only, starts immediately).
    pub system_only: bool,
    pub game_processes: Vec<(String, Vec<String>)>,
    pub audio_roles: HashMap<String, String>,
    /// How long to measure: 120 s normally, 60 s for a quick re-test.
    pub target_secs: f32,
    /// Stream + record test: wait until OBS is streaming AND recording.
    pub need_stream: bool,
}

pub fn start(cfg: TestConfig, ctx: eframe::egui::Context) -> TestHandle {
    let state = Arc::new(Mutex::new(TestState {
        phase: Phase::Connecting,
        elapsed: 0.0,
        last: None,
        result: None,
        shots: 0,
        target: cfg.target_secs,
        rec_active: false,
        stream_active: false,
    }));
    let stop = Arc::new(AtomicBool::new(false));
    let (st, sp) = (state.clone(), stop.clone());
    std::thread::Builder::new()
        .name("live-test".into())
        .spawn(move || {
            let res = run(&cfg, &st, &sp, &ctx);
            if let Ok(mut s) = st.lock() {
                match res {
                    Ok(r) => {
                        s.result = Some(r);
                        s.phase = Phase::Finished;
                    }
                    Err(e) => {
                        // Stopping is the user's choice, not a problem.
                        if e.starts_with("Stopped") {
                            log::info!("Live test ended: {e}");
                        } else {
                            log::warn!("Live test ended: {e}");
                        }
                        s.phase = Phase::Failed(e);
                    }
                }
            }
            ctx.request_repaint();
        })
        .expect("spawn test thread");
    TestHandle { state, stop }
}

fn set_phase(st: &Arc<Mutex<TestState>>, p: Phase) {
    if let Ok(mut s) = st.lock() {
        s.phase = p;
    }
}

fn run(
    cfg: &TestConfig,
    st: &Arc<Mutex<TestState>>,
    stop: &Arc<AtomicBool>,
    ctx: &eframe::egui::Context,
) -> Result<TestResult, String> {
    crate::logger::activity("live test");
    let mut client: Option<Client> = None;
    if !cfg.system_only {
        let mut c = Client::connect(&cfg.host, cfg.port, cfg.password.as_deref())
            .map_err(|e| e.to_string())?;
        set_phase(st, Phase::WaitingForRecord);
        ctx.request_repaint();
        let wait = Instant::now();
        let mut seen: Option<(bool, bool)> = None;
        loop {
            if stop.load(Ordering::Relaxed) {
                return Err(CANCELLED.into());
            }
            let rec = c
                .request("GetRecordStatus", json!({}))
                .ok()
                .and_then(|v| v["outputActive"].as_bool())
                .unwrap_or(false);
            // The stream + record test starts only when both run, in any order.
            let streaming = cfg.need_stream
                && c.request("GetStreamStatus", json!({}))
                    .ok()
                    .and_then(|v| v["outputActive"].as_bool())
                    .unwrap_or(false);
            log::debug!("Waiting: recording {rec}, streaming {streaming}");
            if let Ok(mut s) = st.lock() {
                s.elapsed = wait.elapsed().as_secs_f32();
                s.rec_active = rec;
                s.stream_active = streaming;
            }
            if rec && (!cfg.need_stream || streaming) {
                break;
            }
            // Redraw only when something changed (the spinner ticks by itself once a second).
            if seen != Some((rec, streaming)) {
                seen = Some((rec, streaming));
                ctx.request_repaint();
            }
            if wait.elapsed().as_secs_f32() > WAIT_FOR_RECORD_SECONDS {
                return Err("Stopped watching for a recording after an hour.".into());
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        let _ = c.subscribe_volume(true);
        c.events.clear();
        client = Some(c);
    }
    log::info!("Live test measuring (with OBS: {})", client.is_some());
    set_phase(st, Phase::Measuring);
    ctx.request_repaint();

    let mut mon = Monitor::new();
    let mut sys = crate::hardware::new_system();
    let mut r = TestResult {
        started_at: crate::util::unix_now(),
        with_obs: client.is_some(),
        gpu_source: mon.gpu_source.clone(),
        quick: cfg.target_secs < 100.0,
        ..Default::default()
    };
    let fps_job = crate::gamefps::start();
    // For the viewer preview: the scene being shown and the stream size.
    let mut shot_scene = String::new();
    let mut shot_size: Option<(u64, u64)> = None;
    if let Some(c) = client.as_mut() {
        if let Ok(v) = c.request("GetCurrentProgramScene", json!({})) {
            shot_scene = v["currentProgramSceneName"]
                .as_str()
                .or_else(|| v["sceneName"].as_str())
                .unwrap_or("")
                .to_string();
        }
        if let Ok(v) = c.request("GetVideoSettings", json!({})) {
            if let (Some(w), Some(h)) = (v["outputWidth"].as_u64(), v["outputHeight"].as_u64()) {
                shot_size = Some((w, h));
            }
            if let (Some(n), Some(d)) = (v["fpsNumerator"].as_f64(), v["fpsDenominator"].as_f64())
                && d > 0.0
            {
                r.stream_fps = Some((n / d) as f32);
            }
        }
        clear_previews();
    }
    let mut last_shot_t = 0.0f32;
    // name -> (label, first (dropped, total), last (dropped, total))
    let mut out_acc: HashMap<String, crate::streamwatch::OutputTotals> = HashMap::new();
    let mut first_stats: Option<serde_json::Value> = None;
    let mut last_stats: Option<serde_json::Value> = None;
    let mut obs_cpu = Vec::new();
    let mut mic: Vec<f32> = Vec::new();
    let mut desk: Vec<f32> = Vec::new();
    let mut hog_acc: HashMap<String, (f32, u32)> = HashMap::new();
    let me = sysinfo::get_current_pid().ok();
    let mut own_cpu: Vec<f32> = Vec::new();
    let mut own_ram: f32 = 0.0;
    let start = Instant::now();
    let mut tick = 0u32;
    loop {
        let t = start.elapsed().as_secs_f32();
        if stop.load(Ordering::Relaxed) {
            r.notes
                .push("Test was stopped early from Preflight.".into());
            break;
        }
        if t > MAX_SECONDS {
            r.notes.push("Test stopped after 10 minutes.".into());
            break;
        }
        if cfg.system_only && t >= cfg.target_secs {
            break;
        }
        let s = mon.sample(t);
        r.samples.push(s.clone());
        if let Ok(mut g) = st.lock() {
            g.elapsed = t;
            g.last = Some(s.clone());
        }
        ctx.request_repaint();

        // Every 5 seconds: which game is running, background programs, and
        // frames dropped by each OBS output.
        if tick.is_multiple_of(5) {
            if let Some(c) = client.as_mut() {
                for o in obs::read_outputs(c).into_iter().filter(|o| o.active) {
                    if let (Some(d), Some(t)) = (o.skipped, o.total) {
                        if o.label == "Stream" {
                            r.streamed = true;
                        }
                        let e = out_acc.entry(o.name.clone()).or_insert((
                            o.label.clone(),
                            (d, t),
                            (d, t),
                        ));
                        e.2 = (d, t);
                    }
                }
            }
            let procs = crate::hardware::running_process_names(&mut sys);
            if r.game_running.is_none() {
                for (name, pats) in &cfg.game_processes {
                    if procs
                        .iter()
                        .any(|(n, _)| pats.iter().any(|p| crate::games::process_matches(n, p)))
                    {
                        r.game_running = Some(name.clone());
                    }
                }
                if r.game_running.is_none()
                    && tick.is_multiple_of(15)
                    && let Some(g) = mon.top_gpu_process(&mut sys)
                {
                    // It might still be a known game under another name.
                    let known = cfg.game_processes.iter().find(|(_, pats)| {
                        pats.iter().any(|p| crate::games::process_matches(&g, p))
                    });
                    match known {
                        Some((name, _)) => r.game_running = Some(name.clone()),
                        None => {
                            if r.game_guess.as_deref() != Some(g.as_str()) {
                                log::info!("Game not recognised; the main GPU program is {g}");
                            }
                            r.game_guess = Some(g);
                        }
                    }
                }
            }
            if let (Some(pid), Some(sy)) = (me, sys.as_ref())
                && let Some(p) = sy.process(pid)
            {
                if tick > 0 {
                    own_cpu.push(
                        p.cpu_usage()
                            / std::thread::available_parallelism()
                                .map(|n| n.get())
                                .unwrap_or(1) as f32,
                    );
                }
                own_ram = own_ram.max(p.memory() as f32 / 1_048_576.0);
            }
            for h in crate::hardware::find_hogs(&procs) {
                let e = hog_acc.entry(h.label).or_insert((0.0, 0));
                e.0 += h.cpu;
                e.1 += 1;
            }
        }

        let mut recording_stopped = false;
        if let Some(c) = client.as_mut() {
            match c.request("GetStats", json!({})) {
                Ok(v) => {
                    if first_stats.is_none() {
                        first_stats = Some(v.clone());
                    }
                    if let Some(x) = v["cpuUsage"].as_f64() {
                        obs_cpu.push(x as f32);
                    }
                    last_stats = Some(v);
                }
                Err(e) => {
                    r.notes.push(format!("Lost connection to OBS: {e}"));
                    break;
                }
            }
            // Measuring is done: grab a few original frames for the viewer
            // preview while the recording keeps running. Doing it after the
            // 2 minutes keeps it from affecting the measurements.
            if t >= cfg.target_secs
                && r.preview_shots.len() < PREVIEW_SHOTS
                && t - last_shot_t >= 2.0
                && !shot_scene.is_empty()
            {
                last_shot_t = t;
                if let Some(shot) = grab_shot(c, &shot_scene, shot_size, r.preview_shots.len()) {
                    r.preview_shots.push(shot);
                    if let Ok(mut g) = st.lock() {
                        g.shots = r.preview_shots.len();
                    }
                }
            }
            c.pump_events(Duration::from_millis(900));
            while let Some(ev) = c.events.pop_front() {
                match ev["eventType"].as_str() {
                    Some("RecordStateChanged") => {
                        let active = ev["eventData"]["outputActive"].as_bool().unwrap_or(true);
                        if let Some(p) = ev["eventData"]["outputPath"].as_str()
                            && !p.is_empty()
                        {
                            r.recording_path = Some(p.to_string());
                        }
                        if !active
                            && ev["eventData"]["outputState"].as_str()
                                == Some("OBS_WEBSOCKET_OUTPUT_STOPPED")
                        {
                            recording_stopped = true;
                        }
                    }
                    Some("InputVolumeMeters") => {
                        if let Some(inputs) = ev["eventData"]["inputs"].as_array() {
                            for i in inputs {
                                let name = i["inputName"].as_str().unwrap_or("");
                                let role =
                                    cfg.audio_roles.get(name).map(|s| s.as_str()).unwrap_or("");
                                let peak = i["inputLevelsMul"]
                                    .as_array()
                                    .map(|chs| {
                                        chs.iter()
                                            .filter_map(|ch| ch.get(1).and_then(|x| x.as_f64()))
                                            .fold(0.0f64, f64::max)
                                    })
                                    .unwrap_or(0.0);
                                let db = obs::mul_to_db(peak) as f32;
                                if db > -50.0 {
                                    match role {
                                        "mic" => mic.push(db),
                                        "desktop" | "app" => desk.push(db),
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            if !recording_stopped
                && tick.is_multiple_of(3)
                && let Ok(v) = c.request("GetRecordStatus", json!({}))
                && v["outputActive"].as_bool() == Some(false)
            {
                recording_stopped = true;
            }
        } else {
            std::thread::sleep(Duration::from_millis(1000));
        }
        if recording_stopped {
            log::info!("Recording stopped; test finished at {t:.0}s");
            // Give OBS a moment to send the final file path.
            if let Some(c) = client.as_mut() {
                c.pump_events(Duration::from_millis(800));
                while let Some(ev) = c.events.pop_front() {
                    if ev["eventType"] == "RecordStateChanged"
                        && let Some(p) = ev["eventData"]["outputPath"].as_str()
                        && !p.is_empty()
                    {
                        r.recording_path = Some(p.to_string());
                    }
                }
            }
            break;
        }
        tick += 1;
    }
    if let Some(c) = client.as_mut() {
        let _ = c.subscribe_volume(false);
        // Recording while streaming: the recording started before the test, so OBS
        // never told us its file. Find it for the viewer preview only.
        if r.recording_path.is_none() {
            r.live_rec_path = newest_recording(c);
            if let Some(p) = &r.live_rec_path {
                log::info!(
                    "Using the recording that was already running for the preview ({})",
                    crate::util::scrub(p)
                );
            }
        }
    }

    r.seconds = start.elapsed().as_secs_f32();
    // The game's own FPS (only from sources that don't touch the game).
    let exes: Vec<String> = match &r.game_running {
        Some(name) => cfg
            .game_processes
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, p)| p.clone())
            .unwrap_or_default(),
        None => r.game_guess.iter().cloned().collect(),
    };
    if let Some(m) = crate::gamefps::finish(fps_job, &exes) {
        r.game_fps = Some(m.fps);
        r.game_fps_source = m.source;
    }
    let n = r.samples.len().max(1) as f32;
    r.cpu_avg = r.samples.iter().map(|s| s.cpu).sum::<f32>() / n;
    r.cpu_max = r.samples.iter().map(|s| s.cpu).fold(0.0, f32::max);
    let encs: Vec<f32> = r.samples.iter().filter_map(|s| s.enc).collect();
    if !encs.is_empty() {
        r.enc_avg = Some(encs.iter().sum::<f32>() / encs.len() as f32);
        r.enc_max = Some(encs.iter().cloned().fold(0.0, f32::max));
    }
    r.outputs = out_acc
        .into_values()
        .map(|(label, a, b)| OutputStat {
            label,
            dropped: b.0.saturating_sub(a.0),
            total: b.1.saturating_sub(a.1),
        })
        .filter(|o| o.total > 0)
        .collect();
    r.outputs.sort_by(|a, b| a.label.cmp(&b.label));
    let gpus: Vec<f32> = r.samples.iter().filter_map(|s| s.gpu).collect();
    if !gpus.is_empty() {
        r.gpu_avg = Some(gpus.iter().sum::<f32>() / gpus.len() as f32);
        r.gpu_max = Some(gpus.iter().cloned().fold(0.0, f32::max));
    }
    r.gpu_temp_max = r
        .samples
        .iter()
        .filter_map(|s| s.gpu_temp)
        .fold(None, |a, b| Some(a.map_or(b, |x: f32| x.max(b))));
    r.cpu_temp_max = r
        .samples
        .iter()
        .filter_map(|s| s.cpu_temp)
        .fold(None, |a, b| Some(a.map_or(b, |x: f32| x.max(b))));
    let throttled = r
        .samples
        .iter()
        .filter(|s| s.gpu_throttle.as_deref() == Some("thermal"))
        .count();
    if throttled as f32 > n * 0.1 {
        r.gpu_throttle = Some("thermal".into());
    }
    if !obs_cpu.is_empty() {
        r.obs_cpu_avg = Some(obs_cpu.iter().sum::<f32>() / obs_cpu.len() as f32);
    }
    if let (Some(a), Some(b)) = (&first_stats, &last_stats) {
        let d = |k: &str| {
            b[k].as_u64()
                .unwrap_or(0)
                .saturating_sub(a[k].as_u64().unwrap_or(0))
        };
        r.render_skipped = d("renderSkippedFrames");
        r.render_total = d("renderTotalFrames");
        r.output_skipped = d("outputSkippedFrames");
        r.output_total = d("outputTotalFrames");
    }
    let avg = |v: &Vec<f32>| {
        if v.len() > 10 {
            Some(v.iter().sum::<f32>() / v.len() as f32)
        } else {
            None
        }
    };
    r.own_cpu_avg = if own_cpu.is_empty() {
        None
    } else {
        Some(own_cpu.iter().sum::<f32>() / own_cpu.len() as f32)
    };
    r.own_ram_mb = if own_ram > 0.0 { Some(own_ram) } else { None };
    r.mic_db = avg(&mic);
    r.desktop_db = avg(&desk);
    r.hogs = hog_acc
        .into_iter()
        .map(|(label, (cpu, k))| crate::hardware::Hog {
            label,
            cpu: cpu / k.max(1) as f32,
        })
        .collect();
    if let Some(p) = &r.recording_path {
        r.recording_size = std::fs::metadata(p).ok().map(|m| m.len());
    }
    if r.seconds < 60.0 {
        r.notes
            .push("The test was shorter than a minute, so results are less reliable.".into());
    }
    log::info!(
        "Test done: {:.0}s, CPU avg {:.0}% max {:.0}%, GPU avg {:?} max {:?}, GPU temp max {:?}, render skipped {}/{}, output skipped {}/{}, mic {:?} dB, desktop {:?} dB, game {:?}",
        r.seconds,
        r.cpu_avg,
        r.cpu_max,
        r.gpu_avg,
        r.gpu_max,
        r.gpu_temp_max,
        r.render_skipped,
        r.render_total,
        r.output_skipped,
        r.output_total,
        r.mic_db,
        r.desktop_db,
        r.game_running
    );
    Ok(r)
}

fn clear_previews() {
    if let Ok(rd) = std::fs::read_dir(&crate::util::dirs().previews) {
        for e in rd.flatten() {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// Asks OBS for a picture of the program scene as it is drawn, before
/// encoding (GetSourceScreenshot only reads; it changes nothing in OBS).
fn grab_shot(
    c: &mut Client,
    scene: &str,
    size: Option<(u64, u64)>,
    i: usize,
) -> Option<PreviewShot> {
    use base64::Engine;
    let pos = |c: &mut Client| {
        c.request("GetRecordStatus", json!({}))
            .ok()
            .and_then(|v| v["outputDuration"].as_u64())
    };
    let before = pos(c)?;
    let mut req = json!({"sourceName": scene, "imageFormat": "png"});
    if let Some((w, h)) = size {
        req["imageWidth"] = json!(w);
        req["imageHeight"] = json!(h);
    }
    let v = match c.request("GetSourceScreenshot", req) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("Preview frame failed: {e}");
            return None;
        }
    };
    let after = pos(c).unwrap_or(before);
    let data = v["imageData"].as_str()?;
    let b64 = data.split_once("base64,").map(|x| x.1).unwrap_or(data);
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .ok()?;
    let path = crate::util::dirs()
        .previews
        .join(format!("original-{i}.png"));
    std::fs::write(&path, bytes).ok()?;
    log::info!(
        "Preview frame {i} grabbed at {} ms into the recording",
        (before + after) / 2
    );
    Some(PreviewShot {
        rec_ms: (before + after) / 2,
        path,
    })
}

/// The newest video in OBS's recording folder, if OBS is recording right now
/// and the file was written to in the last minute (the running recording).
fn newest_recording(c: &mut Client) -> Option<String> {
    let st = c.request("GetRecordStatus", json!({})).ok()?;
    if st["outputActive"].as_bool() != Some(true) {
        return None;
    }
    let dir = c.request("GetRecordDirectory", json!({})).ok()?["recordDirectory"]
        .as_str()?
        .to_string();
    newest_video_in(std::path::Path::new(&dir), 60)
}

pub fn newest_video_in(dir: &std::path::Path, max_age_secs: u64) -> Option<String> {
    let now = std::time::SystemTime::now();
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| {
            let n = e.file_name().to_string_lossy().to_lowercase();
            [".mkv", ".mp4", ".mov", ".flv", ".ts"]
                .iter()
                .any(|x| n.ends_with(x))
        })
        .filter_map(|e| {
            let m = e.metadata().ok()?.modified().ok()?;
            (now.duration_since(m).map(|d| d.as_secs()).unwrap_or(0) <= max_age_secs)
                .then(|| (m, e.path()))
        })
        .max_by_key(|(m, _)| *m)
        .map(|(_, p)| p.to_string_lossy().to_string())
}

#[cfg(test)]
mod live_rec_tests {
    #[test]
    fn finds_the_file_being_written() {
        let d = std::env::temp_dir().join(format!("sp-rec-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&d);
        std::fs::write(d.join("notes.txt"), "x").unwrap();
        std::fs::write(d.join("2026-11-26 07-58-00.mov"), "x").unwrap();
        let got = super::newest_video_in(&d, 60).unwrap();
        assert!(got.ends_with(".mov"));
        assert!(super::newest_video_in(&d.join("missing"), 60).is_none());
        let _ = std::fs::remove_dir_all(&d);
    }
}
