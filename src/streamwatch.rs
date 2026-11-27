//! The OBS hub: Preflight's one shared connection to OBS. It answers
//! "read OBS now" requests from the rest of the app, and while you're live
//! it quietly watches OBS's stream stats (read-only) for the post-flight
//! summary. (The 2-minute test uses its own connection while it runs.)
//!
//! The log is saved every minute to `streams/current.json`, so a stream
//! is still summarised if Preflight closes before the stream ends.

use crate::hardware::Monitor;
use crate::obs::Client;
use crate::obs::ObsSnapshot;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct StreamSample {
    /// Seconds since the stream started.
    pub t: f32,
    pub bytes: u64,
    /// Frames dropped because of the network (GetStreamStatus).
    pub net_dropped: u64,
    pub net_total: u64,
    /// Frames the encoder skipped and frames OBS couldn't draw (GetStats; running totals).
    pub enc_skipped: u64,
    pub enc_total: u64,
    pub render_skipped: u64,
    pub render_total: u64,
    pub congestion: f32,
    pub reconnecting: bool,
    pub cpu: f32,
    pub gpu: Option<f32>,
    pub gpu_temp: Option<f32>,
    #[serde(default)]
    pub enc: Option<f32>,
    /// Preflight's own CPU (% of the whole CPU) and memory while live.
    #[serde(default)]
    pub own_cpu: Option<f32>,
    #[serde(default)]
    pub own_mb: Option<f32>,
}

pub type ObsReply = (Option<ObsSnapshot>, Option<String>);

pub enum Cmd {
    /// Read OBS's settings now.
    Snapshot(Sender<ObsReply>),
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct StreamLog {
    pub started_at: i64,
    pub ended_at: i64,
    pub target_kbps: Option<u32>,
    pub encoder: String,
    pub output: String,
    pub game: Option<String>,
    pub partial: Option<String>,
    /// A Twitch bandwidth test (not shown to viewers).
    #[serde(default)]
    pub bandwidth_test: bool,
    pub samples: Vec<StreamSample>,
    /// Every active output (recording, plugin outputs, extra streams):
    /// label, (skipped, total) when first seen, and latest.
    #[serde(default)]
    pub outputs: Vec<OutputTotals>,
    /// OBS's own recording during the stream: seconds recorded (longest
    /// seen), how many times it started, and the average recording rate.
    #[serde(default)]
    pub rec_seconds: f32,
    #[serde(default)]
    pub rec_starts: u32,
    #[serde(default)]
    pub rec_kbps: Option<f32>,
}

/// One output's frame counts: label, (skipped, total) when first seen, and latest.
pub type OutputTotals = (String, (u64, u64), (u64, u64));

/// OBS's recording right now (asked every 5 seconds while live).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecStatus {
    pub active: bool,
    pub paused: bool,
    pub seconds: f32,
    /// How fast the recording file grows, in kbps (once it's been measured).
    pub kbps: Option<f32>,
    /// Bytes written so far (for the rate).
    pub bytes: u64,
}

/// Reads GetRecordStatus. None if OBS didn't answer.
pub fn read_record(c: &mut crate::obs::Client) -> Option<RecStatus> {
    let v = c.request("GetRecordStatus", json!({})).ok()?;
    Some(RecStatus {
        active: v["outputActive"].as_bool().unwrap_or(false),
        paused: v["outputPaused"].as_bool().unwrap_or(false),
        seconds: v["outputDuration"].as_u64().unwrap_or(0) as f32 / 1000.0,
        kbps: None,
        bytes: v["outputBytes"].as_u64().unwrap_or(0),
    })
}

/// kbps from two (seconds, bytes) readings of the same recording.
pub fn rec_rate(a: (f32, u64), b: (f32, u64)) -> Option<f32> {
    let dt = b.0 - a.0;
    (dt >= 4.0 && b.1 > a.1).then(|| (b.1 - a.1) as f32 * 8.0 / 1000.0 / dt)
}

#[derive(Clone, Debug, Default)]
pub struct LiveStatus {
    pub elapsed: f32,
    pub kbps: Option<f32>,
    pub dropped_pct: f32,
    /// OBS's recording, if OBS answered.
    pub rec: Option<RecStatus>,
    /// Frames the encoder skipped since the stream started, as a percentage.
    pub enc_skipped_pct: Option<f32>,
    /// Every active output (stream, recording, plugins), read every 10 seconds.
    pub outputs: Vec<crate::obs::OutputInfo>,
}

#[derive(Default)]
pub struct WatchState {
    /// Connected to OBS right now.
    pub connected: bool,
    pub live: Option<LiveStatus>,
    /// A stream that just ended; the app takes it to show a notice.
    pub finished: Option<StreamLog>,
}

pub struct WatchHandle {
    pub state: Arc<Mutex<WatchState>>,
    pub stop: Arc<AtomicBool>,
    pub cmd: Sender<Cmd>,
    /// Set when the watcher has saved its work and finished.
    pub stopped: Arc<AtomicBool>,
}

impl WatchHandle {
    /// Stops watching and waits (up to `max`) until an unfinished stream is saved,
    /// so another Preflight process can pick it up without losing anything.
    pub fn stop_and_wait(&self, max: Duration) {
        self.stop.store(true, Ordering::Relaxed);
        let t = Instant::now();
        while !self.stopped.load(Ordering::Relaxed) && t.elapsed() < max {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

/// Asks the hub for an OBS snapshot, waiting up to 10 seconds.
pub fn request_snapshot(cmd: &Sender<Cmd>) -> ObsReply {
    let (tx, rx) = channel();
    if cmd.send(Cmd::Snapshot(tx)).is_err() {
        return (
            crate::obs::snapshot_saved(),
            Some("Preflight's OBS connection isn't running.".into()),
        );
    }
    rx.recv_timeout(Duration::from_secs(10))
        .unwrap_or_else(|_| {
            (
                crate::obs::snapshot_saved(),
                Some("OBS took too long to answer.".into()),
            )
        })
}

impl Drop for WatchHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

pub struct WatchConfig {
    pub host: String,
    pub port: u16,
    pub password: Option<String>,
    pub game_processes: Vec<(String, Vec<String>)>,
}

fn current_path() -> std::path::PathBuf {
    crate::util::dirs().streams.join("current.json")
}

pub fn start(cfg: WatchConfig, ctx: eframe::egui::Context) -> WatchHandle {
    let state = Arc::new(Mutex::new(WatchState::default()));
    let stop = Arc::new(AtomicBool::new(false));
    let stopped = Arc::new(AtomicBool::new(false));
    let (st, sp, done) = (state.clone(), stop.clone(), stopped.clone());
    let (cmd, rx) = channel();
    std::thread::Builder::new()
        .name("obs-hub".into())
        .spawn(move || {
            run(cfg, st, sp, rx, ctx);
            done.store(true, Ordering::Relaxed);
        })
        .expect("spawn OBS hub");
    WatchHandle {
        state,
        stop,
        cmd,
        stopped,
    }
}

fn run(
    cfg: WatchConfig,
    st: Arc<Mutex<WatchState>>,
    stop: Arc<AtomicBool>,
    cmds: Receiver<Cmd>,
    ctx: eframe::egui::Context,
) {
    // A stream from last time that wasn't finished.
    let mut pending: Option<StreamLog> = std::fs::read_to_string(current_path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok());
    let started = Instant::now();
    let mut client: Option<Client> = None;
    let mut last_err: Option<String> = None;
    let mut next_try = Instant::now();
    let mut next_status = Instant::now();
    let mut log: Option<StreamLog> = None;
    let mut mon: Option<Monitor> = None;
    let mut sys = None;
    let mut last_save = Instant::now();
    let mut tick = 0u32;
    // The recording's (seconds, bytes) when its rate was last measured, and the rate.
    let mut rec_mark: Option<(f32, u64)> = None;
    let mut rec_kbps: Option<f32> = None;
    let mut rec_was_on = false;
    let mut live_outputs: Vec<crate::obs::OutputInfo> = Vec::new();
    let me = sysinfo::get_current_pid().ok();
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1) as f32;

    let connect = |client: &mut Option<Client>, last_err: &mut Option<String>| match Client::connect(
        &cfg.host,
        cfg.port,
        cfg.password.as_deref(),
    ) {
        Ok(c) => {
            *client = Some(c);
            *last_err = None;
        }
        Err(e) => *last_err = Some(e.to_string()),
    };

    loop {
        if stop.load(Ordering::Relaxed) {
            if let Some(l) = &log {
                save_current(l);
            }
            return;
        }
        if let Ok(mut g) = st.lock() {
            g.connected = client.is_some();
        }
        if client.is_none() && Instant::now() >= next_try {
            connect(&mut client, &mut last_err);
            if client.is_none() {
                next_try = Instant::now() + Duration::from_secs(10);
                // OBS closed: finish a stream we were watching, or an old one
                // after a minute of not finding OBS.
                if let Some(l) = log.take() {
                    finish(
                        l,
                        Some("Preflight lost contact with OBS before the stream ended.".into()),
                        &st,
                        &ctx,
                    );
                    mon = None;
                }
                if started.elapsed().as_secs() > 60
                    && let Some(l) = pending.take()
                {
                    finish(
                        l,
                        Some(
                            "Preflight was closed before the stream ended, so the end is missing."
                                .into(),
                        ),
                        &st,
                        &ctx,
                    );
                }
            }
        }
        let now = Instant::now();
        let wake = if client.is_some() {
            next_status
        } else {
            next_try.min(now + Duration::from_secs(10))
        };
        match cmds.recv_timeout(
            wake.saturating_duration_since(now)
                .min(Duration::from_millis(500)),
        ) {
            Ok(Cmd::Snapshot(reply)) => {
                if client.is_none() {
                    connect(&mut client, &mut last_err);
                }
                let r = match client.as_mut() {
                    Some(c) => (Some(crate::obs::snapshot_live(c)), None),
                    None => (crate::obs::snapshot_saved(), last_err.clone()),
                };
                let _ = reply.send(r);
                continue;
            }
            Err(RecvTimeoutError::Disconnected) => {
                if let Some(l) = &log {
                    save_current(l);
                }
                return;
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
        if Instant::now() < next_status {
            continue;
        }
        let Some(c) = client.as_mut() else { continue };
        let status = match c.request("GetStreamStatus", json!({})) {
            Ok(v) => v,
            Err(e) => {
                last_err = Some(e.to_string());
                client = None;
                next_try = Instant::now();
                continue;
            }
        };
        let active = status["outputActive"].as_bool().unwrap_or(false);
        if active && log.is_none() {
            let mut l = match pending.take() {
                Some(p) => {
                    log::info!("Resuming the post-flight log of a stream that's still live");
                    p
                }
                None => {
                    let snap = crate::obs::snapshot_live(c);
                    log::info!("Stream started; post-flight is watching");
                    StreamLog {
                        started_at: crate::util::unix_now()
                            - status["outputDuration"].as_u64().unwrap_or(0) as i64 / 1000,
                        target_kbps: snap.bitrate_kbps,
                        encoder: snap.encoder_kind.clone(),
                        output: format!(
                            "{}{}",
                            snap.output_res
                                .map(|r| format!("{}p", r.1))
                                .unwrap_or_default(),
                            snap.fps
                                .map(|f| (f.round() as u32).to_string())
                                .unwrap_or_default()
                        ),
                        ..Default::default()
                    }
                }
            };
            l.partial = None;
            log = Some(l);
            mon = Some(Monitor::new());
            sys = crate::hardware::new_system();
            tick = 0;
            rec_was_on = false;
            rec_mark = None;
            rec_kbps = None;
            live_outputs.clear();
        }
        if !active {
            if let Some(l) = log.take() {
                finish(l, None, &st, &ctx);
                mon = None;
                sys = None;
            } else if let Some(l) = pending.take() {
                finish(
                    l,
                    Some(
                        "Preflight was closed before the stream ended, so the end is missing."
                            .into(),
                    ),
                    &st,
                    &ctx,
                );
            }
            if let Ok(mut s) = st.lock() {
                s.live = None;
            }
            next_status = Instant::now() + Duration::from_secs(3);
            continue;
        }
        // Live: sample every 5 seconds.
        let l = log.as_mut().unwrap();
        let t = status["outputDuration"].as_u64().unwrap_or(0) as f32 / 1000.0;
        let stats = c.request("GetStats", json!({})).unwrap_or_default();
        let hw = mon.as_mut().map(|m| m.sample(t)).unwrap_or_default();
        let (mut own_cpu, mut own_mb) = (None, None);
        if let (Some(pid), Some(sy)) = (me, sys.as_mut()) {
            sy.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), false);
            if let Some(p) = sy.process(pid) {
                if tick > 0 {
                    own_cpu = Some(p.cpu_usage() / cores);
                }
                own_mb = Some(p.memory() as f32 / 1_048_576.0);
            }
        }
        let u = |v: &serde_json::Value, k: &str| v[k].as_u64().unwrap_or(0);
        let smp = StreamSample {
            t,
            bytes: u(&status, "outputBytes"),
            net_dropped: u(&status, "outputSkippedFrames"),
            net_total: u(&status, "outputTotalFrames"),
            enc_skipped: u(&stats, "outputSkippedFrames"),
            enc_total: u(&stats, "outputTotalFrames"),
            render_skipped: u(&stats, "renderSkippedFrames"),
            render_total: u(&stats, "renderTotalFrames"),
            congestion: status["outputCongestion"].as_f64().unwrap_or(0.0) as f32,
            reconnecting: status["outputReconnecting"].as_bool().unwrap_or(false),
            cpu: hw.cpu,
            gpu: hw.gpu,
            gpu_temp: hw.gpu_temp,
            enc: hw.enc,
            own_cpu,
            own_mb,
        };
        let kbps = l
            .samples
            .last()
            .filter(|p| t - p.t > 0.5)
            .map(|p| smp.bytes.saturating_sub(p.bytes) as f32 * 8.0 / 1000.0 / (t - p.t));
        let dropped_pct = if smp.net_total > 0 {
            smp.net_dropped as f32 * 100.0 / smp.net_total as f32
        } else {
            0.0
        };
        l.samples.push(smp);
        // Recording: asked every sample, so starting or stopping it mid-stream shows within 5 s.
        let mut rec = read_record(c);
        if let Some(r) = rec.as_mut() {
            if r.active {
                if !rec_was_on {
                    l.rec_starts += 1;
                    rec_mark = None;
                }
                l.rec_seconds = l.rec_seconds.max(r.seconds);
                if !r.paused {
                    match rec_mark {
                        Some(m) if r.seconds - m.0 >= 30.0 => {
                            if let Some(k) = rec_rate(m, (r.seconds, r.bytes)) {
                                rec_kbps = Some(k);
                                l.rec_kbps = Some(k);
                            }
                            rec_mark = Some((r.seconds, r.bytes));
                        }
                        None => rec_mark = Some((r.seconds, r.bytes)),
                        _ => {}
                    }
                }
                r.kbps = rec_kbps;
            }
            rec_was_on = r.active;
        }
        // Every 2 samples (10 s): each output for the Live page; every 10, the log's totals.
        if tick.is_multiple_of(2) {
            live_outputs = crate::obs::read_outputs(c)
                .into_iter()
                .filter(|o| o.active)
                .collect();
        }
        if tick.is_multiple_of(10) {
            for o in live_outputs.iter().filter(|o| o.label != "Stream").cloned() {
                if let (Some(d), Some(tot)) = (o.skipped, o.total) {
                    let label = o.label.clone();
                    match l.outputs.iter_mut().find(|x| x.0 == label) {
                        Some(x) => x.2 = (d, tot),
                        None => l.outputs.push((label, (d, tot), (d, tot))),
                    }
                }
            }
        }
        if tick.is_multiple_of(12) && l.game.is_none() {
            let procs = crate::hardware::running_process_names(&mut sys);
            for (name, pats) in &cfg.game_processes {
                if procs
                    .iter()
                    .any(|(n, _)| pats.iter().any(|p| crate::games::process_matches(n, p)))
                {
                    l.game = Some(name.clone());
                }
            }
        }
        if last_save.elapsed().as_secs() >= 60 {
            save_current(l);
            last_save = Instant::now();
        }
        if let Ok(mut g) = st.lock() {
            let enc_skipped_pct = l.samples.first().zip(l.samples.last()).and_then(|(a, b)| {
                let tot = b.enc_total.saturating_sub(a.enc_total);
                (tot > 0).then(|| {
                    b.enc_skipped.saturating_sub(a.enc_skipped) as f32 * 100.0 / tot as f32
                })
            });
            g.live = Some(LiveStatus {
                elapsed: t,
                kbps,
                dropped_pct,
                rec,
                enc_skipped_pct,
                outputs: live_outputs.clone(),
            });
        }
        ctx.request_repaint();
        tick += 1;
        next_status = Instant::now() + Duration::from_secs(5);
    }
}

fn save_current(l: &StreamLog) {
    if let Ok(t) = serde_json::to_string(l) {
        let _ = std::fs::write(current_path(), t);
    }
}

fn finish(
    mut l: StreamLog,
    partial: Option<String>,
    st: &Arc<Mutex<WatchState>>,
    ctx: &eframe::egui::Context,
) {
    let _ = std::fs::remove_file(current_path());
    if l.samples.len() < 3 {
        log::info!("Stream ended after less than 15 seconds of watching; no post-flight summary");
        return;
    }
    l.ended_at = crate::util::unix_now();
    l.partial = partial;
    let path = crate::util::dirs()
        .streams
        .join(format!("stream-{}.json", l.started_at));
    if let Ok(t) = serde_json::to_string(&l) {
        let _ = std::fs::write(&path, t);
    }
    log::info!(
        "Stream ended after {:.0} s; post-flight summary saved",
        l.samples.last().map(|s| s.t).unwrap_or(0.0)
    );
    if let Ok(mut s) = st.lock() {
        s.live = None;
        s.finished = Some(l);
    }
    ctx.request_repaint();
}

/// Saves a changed log over its file.
pub fn resave(l: &StreamLog) {
    let path = crate::util::dirs()
        .streams
        .join(format!("stream-{}.json", l.started_at));
    if let Ok(t) = serde_json::to_string(l) {
        let _ = std::fs::write(path, t);
    }
}

/// Past streams, newest first.
pub fn past_streams() -> Vec<StreamLog> {
    let mut v: Vec<StreamLog> = std::fs::read_dir(&crate::util::dirs().streams)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.file_name().to_string_lossy().starts_with("stream-"))
                .filter_map(|e| std::fs::read_to_string(e.path()).ok())
                .filter_map(|t| serde_json::from_str(&t).ok())
                .collect()
        })
        .unwrap_or_default();
    v.sort_by_key(|l: &StreamLog| -l.started_at);
    v
}

// ---------------------------------------------------------------------------
// The summary

#[derive(Clone, Debug)]
pub struct Moment {
    pub t: f32,
    /// Seconds the problem lasted.
    pub secs: f32,
    pub text: String,
    severity: f32,
    kind: u8,
    value: f32,
}

#[derive(Clone, Debug)]
pub struct Advice {
    pub title: String,
    pub why: String,
    pub menu: String,
}

#[derive(Clone, Debug, Default)]
pub struct PostFlight {
    pub duration: f32,
    pub grade: String,
    pub grade_level: u8, // 0 smooth .. 3 rough
    pub net_frac: f32,
    pub enc_frac: f32,
    pub render_frac: f32,
    pub avg_kbps: f32,
    pub low_kbps: Option<(f32, f32)>,
    pub reconnects: u32,
    pub cpu_avg: f32,
    pub gpu_avg: Option<f32>,
    pub gpu_temp_max: Option<f32>,
    pub enc_avg: Option<f32>,
    pub own_cpu_avg: Option<f32>,
    pub own_mb_max: Option<f32>,
    /// Preflight's memory at the start and end of the stream (to spot growth).
    pub own_mb_start: Option<f32>,
    pub own_mb_end: Option<f32>,
    /// "Recorded 5h 11m · about 28 Mbps" when OBS recorded during the stream.
    pub recording: Option<String>,
    pub kbps_series: Vec<f32>,
    pub moments: Vec<Moment>,
    pub advice: Vec<Advice>,
    /// "Recording: 0.00% skipped" for every other output during the stream.
    pub outputs: Vec<String>,
}

pub fn fmt_t(t: f32) -> String {
    let s = t.max(0.0) as u32;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

pub fn fmt_duration(t: f32) -> String {
    let m = (t / 60.0).round() as u32;
    if m >= 60 {
        format!("{}h {:02}m", m / 60, m % 60)
    } else {
        format!("{m} min")
    }
}

/// "5.02% of frames (about 1 in every 20)"
fn frac_text(f: f32) -> String {
    if f <= 0.0 {
        return "no frames".into();
    }
    format!(
        "{:.2}% of frames (about 1 in every {})",
        f * 100.0,
        (1.0 / f).round() as u32
    )
}

/// Adds a problem window, merging it with the previous one of the same kind
/// when they touch.
fn push_moment(v: &mut Vec<Moment>, kind: u8, t: f32, secs: f32, value: f32, severity: f32) {
    if let Some(m) = v.iter_mut().rev().find(|m| m.kind == kind)
        && (m.t + m.secs - t).abs() < 1.0
    {
        let total = m.secs + secs;
        m.value = if kind == 1 {
            m.value.min(value)
        } else {
            (m.value * m.secs + value * secs) / total
        };
        m.secs = total;
        m.severity = m.severity.max(severity) + 0.1;
        return;
    }
    v.push(Moment {
        t,
        secs,
        text: String::new(),
        severity,
        kind,
        value,
    });
}

fn moment_text(m: &Moment, target: f32) -> String {
    let dur = if m.secs >= 90.0 {
        format!("{:.0} minutes", m.secs / 60.0)
    } else {
        format!("{:.0} seconds", m.secs)
    };
    match m.kind {
        0 => format!("Your internet dropped about {:.0}% of frames for {dur}. Viewers saw stutters or buffering.", m.value * 100.0),
        1 => format!("Your bitrate dipped as low as {:.0} kbps (you set {:.0}) for {dur}. The picture got blurrier.", m.value, target),
        2 => format!("The encoder fell behind and skipped about {:.0}% of frames for {dur}.", m.value * 100.0),
        3 => format!("OBS couldn't draw about {:.0}% of frames for {dur} (the GPU was too busy).", m.value * 100.0),
        4 => "OBS lost the connection to the server and reconnected. Viewers' streams froze or restarted.".into(),
        _ => format!("Your GPU reached {:.0}°C. Hot cards slow down to protect themselves.", m.value),
    }
}

pub fn summarize(l: &StreamLog) -> PostFlight {
    let mut p = PostFlight::default();
    let (Some(first), Some(last)) = (l.samples.first(), l.samples.last()) else {
        return p;
    };
    p.duration = last.t;
    p.net_frac = if last.net_total > 0 {
        last.net_dropped as f32 / last.net_total as f32
    } else {
        0.0
    };
    let d = |a: u64, b: u64| b.saturating_sub(a);
    let et = d(first.enc_total, last.enc_total);
    let rt = d(first.render_total, last.render_total);
    p.enc_frac = if et > 0 {
        d(first.enc_skipped, last.enc_skipped) as f32 / et as f32
    } else {
        0.0
    };
    p.outputs = l
        .outputs
        .iter()
        .map(|(label, a, b)| {
            format!(
                "{label}: {:.2}% skipped",
                d(a.0, b.0) as f32 * 100.0 / d(a.1, b.1).max(1) as f32
            )
        })
        .collect();
    p.render_frac = if rt > 0 {
        d(first.render_skipped, last.render_skipped) as f32 / rt as f32
    } else {
        0.0
    };
    p.avg_kbps = if last.t > 0.0 {
        last.bytes as f32 * 8.0 / 1000.0 / last.t
    } else {
        0.0
    };
    let n = l.samples.len() as f32;
    p.cpu_avg = l.samples.iter().map(|s| s.cpu).sum::<f32>() / n;
    let gpus: Vec<f32> = l.samples.iter().filter_map(|s| s.gpu).collect();
    p.gpu_avg = if gpus.is_empty() {
        None
    } else {
        Some(gpus.iter().sum::<f32>() / gpus.len() as f32)
    };
    p.gpu_temp_max = l
        .samples
        .iter()
        .filter_map(|s| s.gpu_temp)
        .fold(None, |a, b| Some(a.map_or(b, |x: f32| x.max(b))));
    let avg = |v: Vec<f32>| {
        if v.is_empty() {
            None
        } else {
            Some(v.iter().sum::<f32>() / v.len() as f32)
        }
    };
    p.enc_avg = avg(l.samples.iter().filter_map(|s| s.enc).collect());
    p.own_cpu_avg = avg(l.samples.iter().filter_map(|s| s.own_cpu).collect());
    p.own_mb_max = l
        .samples
        .iter()
        .filter_map(|s| s.own_mb)
        .fold(None, |a, b| Some(a.map_or(b, |x: f32| x.max(b))));
    p.own_mb_start = l.samples.iter().find_map(|s| s.own_mb);
    p.own_mb_end = l.samples.iter().rev().find_map(|s| s.own_mb);
    p.recording = (l.rec_starts > 0).then(|| {
        format!(
            "Recorded {}{}{}",
            fmt_duration(l.rec_seconds),
            l.rec_kbps
                .map(|k| format!(" · about {:.0} Mbps", k / 1000.0))
                .unwrap_or_default(),
            if l.rec_starts > 1 {
                format!(" · started {} times", l.rec_starts)
            } else {
                String::new()
            }
        )
    });
    let mut was = false;
    for s in &l.samples {
        if s.reconnecting && !was {
            p.reconnects += 1;
        }
        was = s.reconnecting;
    }
    for w in l.samples.windows(2) {
        let dt = w[1].t - w[0].t;
        if dt > 0.5 {
            p.kbps_series
                .push(w[1].bytes.saturating_sub(w[0].bytes) as f32 * 8.0 / 1000.0 / dt);
        }
    }

    // 30-second windows for moments and the lowest bitrate.
    let mut i = 0;
    let target = l.target_kbps.map(|b| b as f32).unwrap_or(p.avg_kbps);
    while i + 1 < l.samples.len() {
        let a = &l.samples[i];
        let mut j = i + 1;
        while j + 1 < l.samples.len() && l.samples[j].t - a.t < 30.0 {
            j += 1;
        }
        let b = &l.samples[j];
        let dt = b.t - a.t;
        if dt >= 10.0 {
            let kbps = b.bytes.saturating_sub(a.bytes) as f32 * 8.0 / 1000.0 / dt;
            if p.low_kbps.map(|x| kbps < x.0).unwrap_or(true) {
                p.low_kbps = Some((kbps, a.t));
            }
            let frac = |s: u64, sb: u64, tt: u64, tb: u64| {
                let tot = tb.saturating_sub(tt);
                if tot > 0 {
                    sb.saturating_sub(s) as f32 / tot as f32
                } else {
                    0.0
                }
            };
            let nd = frac(a.net_dropped, b.net_dropped, a.net_total, b.net_total);
            let ed = frac(a.enc_skipped, b.enc_skipped, a.enc_total, b.enc_total);
            let rd = frac(
                a.render_skipped,
                b.render_skipped,
                a.render_total,
                b.render_total,
            );
            if nd > 0.02 {
                push_moment(&mut p.moments, 0, a.t, dt, nd, nd * 3.0);
            } else if target > 0.0 && kbps < target * 0.6 {
                push_moment(&mut p.moments, 1, a.t, dt, kbps, 1.0 - kbps / target);
            }
            if ed > 0.02 {
                push_moment(&mut p.moments, 2, a.t, dt, ed, ed * 3.0);
            }
            if rd > 0.02 {
                push_moment(&mut p.moments, 3, a.t, dt, rd, rd * 3.0);
            }
        }
        i = j;
    }
    for (k, w) in l.samples.windows(2).enumerate() {
        if w[1].reconnecting && !w[0].reconnecting {
            let _ = k;
            p.moments.push(Moment {
                t: w[1].t,
                secs: 0.0,
                text: String::new(),
                severity: 5.0,
                kind: 4,
                value: 0.0,
            });
        }
    }
    if let Some(s) = l
        .samples
        .iter()
        .find(|s| s.gpu_temp.map(|t| t >= 87.0).unwrap_or(false))
    {
        p.moments.push(Moment {
            t: s.t,
            secs: 0.0,
            text: String::new(),
            severity: 0.5,
            kind: 5,
            value: s.gpu_temp.unwrap_or(0.0),
        });
    }
    p.moments
        .sort_by(|a, b| b.severity.partial_cmp(&a.severity).unwrap());
    p.moments.truncate(6);
    p.moments.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap());
    for m in p.moments.iter_mut() {
        m.text = moment_text(m, target);
    }

    let worst = p.net_frac.max(p.enc_frac).max(p.render_frac);
    p.grade_level = if worst < 0.001 {
        0
    } else if worst < 0.01 {
        1
    } else if worst < 0.05 {
        2
    } else {
        3
    };
    if p.reconnects > 0 {
        p.grade_level = p.grade_level.max(2);
    }
    p.grade = [
        "Smooth stream",
        "Mostly smooth",
        "Some rough patches",
        "Rough stream",
    ][p.grade_level as usize]
        .into();

    // What to change next time
    if p.net_frac > 0.005 || p.reconnects > 0 {
        let base = l.target_kbps.map(|b| b as f32).unwrap_or(p.avg_kbps);
        let worst = p.low_kbps.map(|x| x.0).filter(|w| *w < base * 0.9);
        let goal = worst
            .map(|w| (base * 0.8).min(w * 0.9))
            .unwrap_or(base * 0.8);
        let lower = ((goal / 500.0).floor() * 500.0).max(1500.0) as u32;
        let managed = worst
            .map(|w| format!(" At its worst, your connection only carried about {w:.0} kbps."))
            .unwrap_or_default();
        p.advice.push(Advice {
            title: format!("Lower your bitrate to about {lower} kbps"),
            why: format!("Your internet dropped {}.{managed} A lower bitrate leaves headroom for dips. Also pause cloud backups and downloads while live, and use a cable instead of Wi-Fi if you can.", frac_text(p.net_frac)),
            menu: "OBS › Settings › Output › Video Bitrate".into(),
        });
    }
    if p.enc_frac > 0.005 {
        p.advice.push(Advice {
            title: "Give the encoder more room".into(),
            why: format!("The encoder skipped {}. Use your GPU encoder (NVENC, AMF or Quick Sync), pick a faster preset, or cap your game's FPS.", frac_text(p.enc_frac)),
            menu: "OBS › Settings › Output › Encoder / Preset".into(),
        });
    }
    if p.render_frac > 0.005 {
        let gpu = p
            .gpu_avg
            .map(|g| format!(" Your GPU averaged {g:.0}%."))
            .unwrap_or_default();
        p.advice.push(Advice {
            title: "Free up your GPU for OBS".into(),
            why: format!("OBS couldn't draw {} because the game used the whole GPU.{gpu} Cap your game's FPS, or turn on DLSS or FSR.", frac_text(p.render_frac)),
            menu: "In your game's graphics settings".into(),
        });
    }
    if p.gpu_temp_max.map(|t| t >= 87.0).unwrap_or(false) {
        p.advice.push(Advice {
            title: "Bring your GPU temperature down".into(),
            why: format!("It peaked at {:.0}°C. Cap your FPS, check the fans, and give the case more airflow.", p.gpu_temp_max.unwrap_or(0.0)),
            menu: "Your PC and game settings".into(),
        });
    }
    p
}

impl PostFlight {
    pub fn net_sentence(&self) -> (String, String) {
        if self.net_frac <= 0.0 {
            (
                "Your internet didn't drop any frames.".into(),
                "The connection held up the whole time.".into(),
            )
        } else {
            (
                format!("Your internet dropped {}.", frac_text(self.net_frac)),
                if self.net_frac < 0.001 {
                    "Too few for viewers to notice.".into()
                } else if self.net_frac < 0.01 {
                    "Viewers may have seen a few small hitches.".into()
                } else {
                    "Viewers saw stutters or buffering.".into()
                },
            )
        }
    }
    pub fn enc_sentence(&self) -> (String, String) {
        if self.enc_frac <= 0.0 {
            (
                "The encoder kept up with every frame.".into(),
                "No stutters from encoding.".into(),
            )
        } else {
            (
                format!(
                    "The encoder skipped {:.2}% of frames.",
                    self.enc_frac * 100.0
                ),
                if self.enc_frac < 0.001 {
                    "Too few to notice.".into()
                } else {
                    "Viewers saw stutters when it fell behind.".into()
                },
            )
        }
    }
    pub fn render_sentence(&self) -> (String, String) {
        if self.render_frac <= 0.0 {
            (
                "OBS drew every frame.".into(),
                "The GPU had room for OBS.".into(),
            )
        } else {
            (
                format!(
                    "OBS couldn't draw {:.2}% of frames.",
                    self.render_frac * 100.0
                ),
                if self.render_frac < 0.001 {
                    "Too few to notice.".into()
                } else {
                    "The game was using the whole GPU.".into()
                },
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log_with(drop_from: usize) -> StreamLog {
        let mut l = StreamLog {
            target_kbps: Some(6000),
            ..Default::default()
        };
        for i in 0..120u64 {
            let t = i as f32 * 5.0;
            let nd = if i as usize > drop_from {
                (i - drop_from as u64) * 30
            } else {
                0
            };
            l.samples.push(StreamSample {
                t,
                bytes: i * 5 * 750_000,
                net_dropped: nd,
                net_total: i * 300,
                enc_total: i * 300,
                render_total: i * 300,
                cpu: 30.0,
                ..Default::default()
            });
        }
        l
    }

    #[test]
    fn smooth_stream() {
        let p = summarize(&log_with(10_000));
        assert_eq!(p.grade_level, 0);
        assert!(p.advice.is_empty());
        assert!((p.avg_kbps - 6000.0).abs() < 100.0, "{}", p.avg_kbps);
    }

    #[test]
    fn network_drops_found() {
        let p = summarize(&log_with(100));
        assert!(p.grade_level >= 1);
        assert!(p.advice.iter().any(|a| a.title.contains("bitrate")));
        assert!(!p.moments.is_empty());
        assert!(p.moments[0].t >= 480.0);
    }
}

#[cfg(test)]
mod rec_tests {
    use super::*;

    #[test]
    fn recording_rate_and_auto_record() {
        assert_eq!(
            rec_rate((10.0, 1_000_000), (40.0, 106_000_000)),
            Some(28000.0)
        );
        assert_eq!(rec_rate((10.0, 5), (12.0, 9)), None, "too short to measure");
        assert!(crate::obs::parse_auto_record(
            "[General]\nX=1\n[BasicWindow]\nRecordWhenStreaming=true\n"
        ));
        assert!(!crate::obs::parse_auto_record(
            "[BasicWindow]\nRecordWhenStreaming=false\n"
        ));
        assert!(
            !crate::obs::parse_auto_record("[Other]\nRecordWhenStreaming=true\n"),
            "only the BasicWindow section counts"
        );
        let l = StreamLog {
            rec_starts: 1,
            rec_seconds: 3600.0 * 5.0 + 660.0,
            rec_kbps: Some(28000.0),
            samples: vec![
                StreamSample {
                    t: 0.0,
                    own_mb: Some(198.0),
                    ..Default::default()
                },
                StreamSample {
                    t: 5.0,
                    own_mb: Some(249.0),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let p = summarize(&l);
        assert_eq!((p.own_mb_start, p.own_mb_end), (Some(198.0), Some(249.0)));
        assert_eq!(
            p.recording.as_deref(),
            Some("Recorded 5h 11m · about 28 Mbps")
        );
    }
}
