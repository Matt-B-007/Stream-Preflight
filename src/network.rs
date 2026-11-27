// SPDX-License-Identifier: GPL-3.0-or-later
//! Network test: measures upload speed and how steady it is, using
//! Cloudflare's public speed-test endpoints. Sends only random test data.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const UP_URL: &str = "https://speed.cloudflare.com/__up";
const DOWN_URL: &str = "https://speed.cloudflare.com/__down?bytes=0";
pub const MAX_TEST_SECONDS: f32 = 15.0;
pub const MAX_TEST_BYTES: u64 = 120 * 1024 * 1024;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NetResult {
    pub ok: bool,
    pub error: Option<String>,
    pub latency_ms: Option<f32>,
    pub samples_mbps: Vec<f32>,
    pub avg_mbps: f32,
    pub low_mbps: f32,
    pub high_mbps: f32,
    /// low / average: 1.0 = perfectly steady
    pub steadiness: f32,
    pub bytes_used: u64,
    pub seconds: f32,
    pub tested_at: i64,
    /// Fast connection: stopped at the data limit before the time limit.
    pub stopped_early: bool,
}

/// A good result is reused for this long, so each scan or restart doesn't use data again.
pub const REUSE_SECS: i64 = 6 * 60 * 60;

fn saved_path() -> std::path::PathBuf {
    crate::util::dirs().data.join("network.json")
}

/// The last good result, if it's recent enough to reuse.
pub fn load_recent() -> Option<NetResult> {
    let n: NetResult = serde_json::from_str(&std::fs::read_to_string(saved_path()).ok()?).ok()?;
    n.reusable(crate::util::unix_now()).then_some(n)
}

/// Keeps a good result for the next scan. Only speeds and times are stored.
pub fn save(n: &NetResult) {
    if !n.ok {
        return;
    }
    let p = saved_path();
    if let Ok(json) = serde_json::to_string(n) {
        let tmp = p.with_extension("json.tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(&tmp, &p);
        }
    }
}

impl NetResult {
    /// Good, and tested within the last 6 hours (and not in the future).
    pub fn reusable(&self, now: i64) -> bool {
        let age = now - self.tested_at;
        self.ok && (0..REUSE_SECS).contains(&age)
    }

    /// The highest video bitrate that should stay safe on this connection.
    /// Whether the slowest moments come close to what a stream at
    /// `bitrate_kbps` needs. Big swings on a very fast line don't matter.
    pub fn unsteady_for(&self, bitrate_kbps: u32) -> bool {
        let need_mbps = (bitrate_kbps.max(3000) as f32 + 160.0) / 1000.0;
        self.ok
            && self.samples_mbps.len() >= 4
            && self.steadiness < 0.6
            && self.low_mbps < need_mbps * 3.0
    }

    pub fn safe_video_kbps(&self) -> u32 {
        if !self.ok || self.low_mbps <= 0.0 {
            return 0;
        }
        let kbps = self.low_mbps * 1000.0 * 0.75 - 160.0;
        kbps.max(0.0) as u32
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .user_agent(format!("StreamPreflight/{}", crate::util::VERSION))
        .build()
        .into()
}

pub fn run(progress: Arc<std::sync::Mutex<f32>>, cancel: Arc<AtomicBool>) -> NetResult {
    crate::logger::activity("network test");
    let a = agent();
    let mut r = NetResult {
        tested_at: crate::util::unix_now(),
        ..Default::default()
    };

    // Latency: median of 5 tiny requests.
    let mut lat = Vec::new();
    for _ in 0..5 {
        let t = Instant::now();
        if a.get(DOWN_URL).call().is_ok() {
            lat.push(t.elapsed().as_secs_f32() * 1000.0);
        }
    }
    if lat.is_empty() {
        r.error = Some(
            "Couldn't reach the speed-test server. Check your internet connection or firewall."
                .into(),
        );
        log::warn!("Network test: speed-test server unreachable");
        return r;
    }
    lat.sort_by(|x, y| x.partial_cmp(y).unwrap());
    r.latency_ms = Some(lat[lat.len() / 2]);

    // Upload. First a short warm-up (not counted) to find a good upload
    // size, then fixed-size uploads of about a second each, so the
    // samples measure the connection rather than the test itself.
    let mut payload: Vec<u8> = (0..4 * 1024 * 1024).map(|_| fastrand::u8(..)).collect();
    let mut chunk: usize = 1024 * 1024;
    let mut warm = Vec::new();
    for _ in 0..2 {
        if cancel.load(Ordering::Relaxed) {
            r.error = Some("Network test was cancelled.".into());
            return r;
        }
        let t = Instant::now();
        if a.post(UP_URL)
            .header("Content-Type", "application/octet-stream")
            .send(&payload[..chunk])
            .is_ok()
        {
            warm.push((chunk as f32 * 8.0) / t.elapsed().as_secs_f32().max(0.001) / 1_000_000.0);
            r.bytes_used += chunk as u64;
        }
    }
    let est = warm.iter().cloned().fold(0.0f32, f32::max);
    // About a second of data per upload, but never more than 8 MB: very
    // large uploads can time out or be refused by the test server.
    chunk = ((est * 1_000_000.0 / 8.0) as usize).clamp(512 * 1024, 8 * 1024 * 1024);
    let mut last_err: Option<String> = None;
    if payload.len() < chunk {
        payload.resize(chunk, 7);
    }
    log::debug!(
        "Network warm-up: {:?} Mbps, upload size {} KB",
        warm,
        chunk / 1024
    );
    let start = Instant::now();
    while start.elapsed().as_secs_f32() < MAX_TEST_SECONDS
        && r.bytes_used + chunk as u64 <= MAX_TEST_BYTES
    {
        if cancel.load(Ordering::Relaxed) {
            r.error = Some("Network test was cancelled.".into());
            return r;
        }
        let t = Instant::now();
        let res = a
            .post(UP_URL)
            .header("Content-Type", "application/octet-stream")
            .send(&payload[..chunk]);
        let secs = t.elapsed().as_secs_f32();
        match res {
            Ok(_) => {
                r.samples_mbps
                    .push((chunk as f32 * 8.0) / secs.max(0.001) / 1_000_000.0);
                r.bytes_used += chunk as u64;
            }
            Err(e) => {
                log::warn!(
                    "Upload of {} KB failed after {secs:.1} s: {e}",
                    chunk / 1024
                );
                last_err = Some(e.to_string());
                // Try smaller uploads instead of counting a zero.
                if chunk > 512 * 1024 {
                    chunk /= 2;
                } else {
                    r.samples_mbps.push(0.0);
                }
            }
        }
        if let Ok(mut p) = progress.lock() {
            *p = (start.elapsed().as_secs_f32() / MAX_TEST_SECONDS).min(1.0);
        }
    }
    if r.bytes_used + chunk as u64 > MAX_TEST_BYTES {
        r.stopped_early = true;
    }
    r.seconds = start.elapsed().as_secs_f32();
    // The first sample includes connection setup; drop it if we have more.
    let mut s: Vec<f32> = r
        .samples_mbps
        .iter()
        .cloned()
        .filter(|x| *x > 0.0)
        .collect();
    if s.is_empty() && !warm.is_empty() {
        // The main uploads failed but the warm-up worked: use that as a
        // rough figure rather than reporting nothing.
        log::warn!(
            "Upload test: main uploads failed ({}); using the warm-up figures",
            last_err.as_deref().unwrap_or("unknown error")
        );
        s = warm.clone();
        r.error = None;
    }
    if s.is_empty() {
        r.error = Some(format!(
            "Upload test failed. Your connection may be blocking uploads to the test server.{}",
            last_err
                .map(|e| format!(" (Error: {e})"))
                .unwrap_or_default()
        ));
        return r;
    }
    let avg = s.iter().sum::<f32>() / s.len() as f32;
    let mut sorted = s.clone();
    sorted.sort_by(|x, y| x.partial_cmp(y).unwrap());
    let low = sorted[(sorted.len() as f32 * 0.1) as usize];
    let high = *sorted.last().unwrap();
    r.avg_mbps = avg;
    r.low_mbps = low;
    r.high_mbps = high;
    r.steadiness = if avg > 0.0 { (low / avg).min(1.0) } else { 0.0 };
    r.ok = true;
    log::info!(
        "Network: latency {:.0} ms, upload avg {:.1} / low {:.1} / high {:.1} Mbps, steadiness {:.2}, used {}",
        r.latency_ms.unwrap_or(0.0),
        avg,
        low,
        high,
        r.steadiness,
        crate::util::fmt_bytes(r.bytes_used)
    );
    r
}

// ---------------------------------------------------------------------------
// Platform limits. Kept as data so they can be updated like game profiles.

#[derive(Clone, Debug, Serialize)]
pub struct PlatformLimits {
    pub max_kbps_1080p60: u32,
    pub note: &'static str,
}

pub fn limits(p: crate::config::Platform) -> PlatformLimits {
    use crate::config::Platform::*;
    match p {
        Twitch => PlatformLimits {
            max_kbps_1080p60: 6000,
            note: "Twitch's standard limit for H.264 is about 6000 kbps. Higher bitrates only apply with Enhanced Broadcasting.",
        },
        YouTube => PlatformLimits {
            max_kbps_1080p60: 12000,
            note: "YouTube accepts higher bitrates; around 9000–12000 kbps suits 1080p60.",
        },
        Kick => PlatformLimits {
            max_kbps_1080p60: 8000,
            note: "Kick accepts up to about 8000 kbps.",
        },
    }
}

#[cfg(test)]
mod reuse_tests {
    use super::*;

    #[test]
    fn reuses_good_results_for_six_hours() {
        let now = 1_000_000;
        let n = |ok, ago| NetResult {
            ok,
            tested_at: now - ago,
            ..Default::default()
        };
        assert!(n(true, 60).reusable(now));
        assert!(n(true, REUSE_SECS - 1).reusable(now));
        assert!(!n(true, REUSE_SECS).reusable(now));
        assert!(!n(false, 60).reusable(now));
        assert!(
            !n(true, -60).reusable(now),
            "a clock change must not keep it forever"
        );
        let back: NetResult =
            serde_json::from_str(&serde_json::to_string(&n(true, 5)).unwrap()).unwrap();
        assert_eq!(back.tested_at, now - 5);
    }
}
