// SPDX-License-Identifier: GPL-3.0-or-later
//! Every video encoder on this PC: what it is, what it can encode, and
//! whether something is using it right now. Read-only: NVML on NVIDIA,
//! per-process DRM usage files on Linux for AMD and Intel, and Windows'
//! own GPU counters. Only program names are kept, never window titles.

use crate::hardware::{HardwareInfo, Vendor};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct EncSession {
    /// The program using it ("OBS", "Discord"...).
    pub app: String,
    pub codec: String,
    pub res: String,
    pub fps: Option<u32>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct EncoderInfo {
    pub gpu: String,
    /// "NVIDIA NVENC", "AMD AMF", "VA-API (AMD)", "Intel Quick Sync"...
    pub kind: String,
    pub chips: Option<u8>,
    pub codecs: Vec<String>,
    /// How busy the encoder is now, in percent (if known).
    pub busy: Option<f32>,
    /// Programs encoding right now. Empty with `busy` > 0 means "in use,
    /// but this system doesn't say by whom".
    pub sessions: Vec<EncSession>,
    /// Whether "in use" could be checked at all on this system.
    pub can_tell: bool,
}

impl EncoderInfo {
    pub fn in_use(&self) -> bool {
        !self.sessions.is_empty() || self.busy.map(|b| b >= 1.0).unwrap_or(false)
    }

    /// "In use: OBS (H.264 1920×1080 60 fps), OBS (HEVC 2560×1440 60 fps)"
    pub fn use_text(&self) -> String {
        if !self.can_tell {
            return "Can't tell if it's in use on this system".into();
        }
        if !self.sessions.is_empty() {
            let list: Vec<String> = self
                .sessions
                .iter()
                .map(|s| {
                    let detail: Vec<String> = [
                        s.codec.clone(),
                        s.res.clone(),
                        s.fps.map(|f| format!("{f} fps")).unwrap_or_default(),
                    ]
                    .into_iter()
                    .filter(|x| !x.is_empty())
                    .collect();
                    if detail.is_empty() {
                        s.app.clone()
                    } else {
                        format!("{} ({})", s.app, detail.join(" "))
                    }
                })
                .collect();
            return format!(
                "In use: {} encode{}: {}",
                self.sessions.len(),
                if self.sessions.len() == 1 { "" } else { "s" },
                list.join(", ")
            );
        }
        match self.busy {
            Some(b) if b >= 1.0 => format!("In use ({b:.0}% busy)"),
            _ => "Not in use right now".into(),
        }
    }
}

/// Which codecs an encoder supports, from the card's generation.
pub fn codecs_for(vendor: Vendor, gpu: &str) -> Vec<String> {
    let n = gpu.to_lowercase();
    let has = |m: &str| n.contains(m);
    let av1 = match vendor {
        // NVIDIA: RTX 40/50 series and Ada/Blackwell workstation cards.
        Vendor::Nvidia => {
            ["rtx 40", "rtx 50", " 40", " 50"].iter().any(|m| has(m)) && !has("gtx")
                || has("ada")
                || has("blackwell")
        }
        // AMD: RX 7000/9000; Intel: Arc and Core Ultra.
        Vendor::Amd => has("rx 7") || has("rx 9") || has("780m") || has("890m"),
        Vendor::Intel => has("arc") || has("ultra"),
        _ => false,
    };
    let hevc = match vendor {
        Vendor::Nvidia => !has("gtx 9") && !has("gtx 7"),
        _ => true,
    };
    let mut v = vec!["H.264".to_string()];
    if hevc {
        v.push("HEVC".into());
    }
    if av1 {
        v.push("AV1".into());
    }
    v
}

fn kind_for(vendor: Vendor) -> Option<&'static str> {
    Some(match (vendor, cfg!(windows)) {
        (Vendor::Nvidia, _) => "NVIDIA NVENC",
        (Vendor::Amd, true) => "AMD AMF",
        (Vendor::Amd, false) => "VA-API (AMD)",
        (Vendor::Intel, true) => "Intel Quick Sync",
        (Vendor::Intel, false) => "VA-API (Intel)",
        _ => return None,
    })
}

/// A friendly name for a program using the encoder.
fn app_name(pid: u32, sys: &Option<sysinfo::System>) -> String {
    let raw = sys
        .as_ref()
        .and_then(|s| s.process(sysinfo::Pid::from_u32(pid)))
        .map(|p| p.name().to_string_lossy().to_lowercase())
        .unwrap_or_default();
    friendly(&raw)
}

pub fn friendly(raw: &str) -> String {
    let r = raw.trim_end_matches(".exe");
    match r {
        x if x.starts_with("obs") => "OBS".into(),
        x if x.contains("discord") => "Discord".into(),
        x if x.contains("nvcontainer") || x.contains("nvidia") || x.contains("nvoverlay") => {
            "NVIDIA app / overlay".into()
        }
        x if x.contains("steam") => "Steam".into(),
        x if x.contains("chrome") || x.contains("firefox") || x.contains("msedge") => {
            "Web browser".into()
        }
        x if x.contains("streamlabs") => "Streamlabs".into(),
        x if x.contains("meld") => "Meld Studio".into(),
        x if x.contains("parsec") || x.contains("sunshine") || x.contains("moonlight") => {
            "Game streaming app".into()
        }
        x if x.contains("gpu-screen-recorder") || x.contains("gsr") => "GPU Screen Recorder".into(),
        x if x.contains("kwin")
            || x.contains("gnome-shell")
            || x.contains("xdg-desktop-portal") =>
        {
            "Desktop screen sharing".into()
        }
        "" => "Unknown program".into(),
        x => x.to_string(),
    }
}

/// Linux: which programs are using the AMD/Intel video encode engine, from
/// /proc/<pid>/fdinfo (the kernel's own usage counters). Two quick samples.
#[cfg(not(windows))]
fn drm_encode_users(sys: &Option<sysinfo::System>) -> (Option<f32>, Vec<EncSession>) {
    use std::collections::HashMap;
    fn read_all() -> HashMap<u32, u64> {
        let mut m = HashMap::new();
        let Ok(rd) = std::fs::read_dir("/proc") else {
            return m;
        };
        for e in rd.flatten() {
            let Some(pid) = e.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else {
                continue;
            };
            let Ok(fds) = std::fs::read_dir(e.path().join("fdinfo")) else {
                continue;
            };
            let mut seen_clients: Vec<String> = Vec::new();
            let mut total = 0u64;
            for f in fds.flatten() {
                let Ok(t) = std::fs::read_to_string(f.path()) else {
                    continue;
                };
                if !t.contains("drm-driver") {
                    continue;
                }
                // One DRM client can be open on several file handles.
                let client = t
                    .lines()
                    .find(|l| l.starts_with("drm-client-id"))
                    .unwrap_or("")
                    .to_string();
                if seen_clients.contains(&client) {
                    continue;
                }
                seen_clients.push(client);
                for l in t.lines() {
                    // amdgpu: drm-engine-enc / enc_1 (VCN); i915/xe: drm-engine-video.
                    if let Some(v) = l
                        .strip_prefix("drm-engine-enc")
                        .or_else(|| l.strip_prefix("drm-engine-video:"))
                    {
                        let num: String = v
                            .chars()
                            .skip_while(|c| !c.is_ascii_digit())
                            .take_while(|c| c.is_ascii_digit())
                            .collect();
                        total += num.parse::<u64>().unwrap_or(0);
                    }
                }
            }
            if total > 0 {
                m.insert(pid, total);
            }
        }
        m
    }
    let a = read_all();
    if a.is_empty() {
        return (Some(0.0), Vec::new());
    }
    std::thread::sleep(std::time::Duration::from_millis(400));
    let b = read_all();
    let mut users = Vec::new();
    let mut busy_ns = 0u64;
    for (pid, nb) in &b {
        let d = nb.saturating_sub(*a.get(pid).unwrap_or(nb));
        if d > 0 {
            busy_ns += d;
            users.push(EncSession {
                app: app_name(*pid, sys),
                ..Default::default()
            });
        }
    }
    users.dedup();
    (Some((busy_ns as f32 / 4.0e6).min(100.0)), users)
}

/// Windows: the "GPU Engine" counters name the program (pid) of each video
/// encode engine user.
#[cfg(windows)]
fn windows_encode_users(sys: &Option<sysinfo::System>) -> (Option<f32>, Vec<EncSession>) {
    let script = "(Get-Counter '\\GPU Engine(*engtype_VideoEncode)\\Utilization Percentage' -ErrorAction SilentlyContinue).CounterSamples | Where-Object {$_.CookedValue -gt 0.5} | ForEach-Object { $_.InstanceName + '=' + [math]::Round($_.CookedValue,1) }";
    let mut cmd = std::process::Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", script]);
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let Ok(o) = cmd.output() else {
        return (None, Vec::new());
    };
    let text = String::from_utf8_lossy(&o.stdout);
    let mut users = Vec::new();
    let mut busy = 0.0f32;
    for l in text.lines() {
        let Some((inst, val)) = l.split_once('=') else {
            continue;
        };
        busy = busy.max(val.trim().parse().unwrap_or(0.0));
        if let Some(pid) = inst
            .split('_')
            .skip_while(|p| *p != "pid")
            .nth(1)
            .and_then(|p| p.parse::<u32>().ok())
        {
            let s = EncSession {
                app: app_name(pid, sys),
                ..Default::default()
            };
            if !users.contains(&s) {
                users.push(s);
            }
        }
    }
    (Some(busy), users)
}

/// A snapshot of every encoder. Takes about half a second (it samples twice).
pub fn snapshot(hw: &HardwareInfo) -> Vec<EncoderInfo> {
    let mut sys = sysinfo::System::new().ok();
    if let Some(s) = sys.as_mut() {
        s.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    }
    let nvml = nvml_wrapper::Nvml::init().ok();
    let mut out: Vec<EncoderInfo> = Vec::new();
    let mut nv_index = 0u32;
    #[cfg(not(windows))]
    let mut drm_done = false;
    #[cfg(windows)]
    let mut win_done = false;
    for g in &hw.gpus {
        let Some(kind) = kind_for(g.vendor) else {
            continue;
        };
        let mut e = EncoderInfo {
            gpu: g.name.clone(),
            kind: kind.into(),
            chips: if g.vendor == Vendor::Nvidia {
                crate::hardware::nvenc_chips(&g.name)
            } else {
                Some(1)
            },
            codecs: codecs_for(g.vendor, &g.name),
            ..Default::default()
        };
        if g.vendor == Vendor::Nvidia {
            if let Some(d) = nvml.as_ref().and_then(|n| n.device_by_index(nv_index).ok()) {
                e.can_tell = true;
                e.busy = d.encoder_utilization().ok().map(|u| u.utilization as f32);
                match d.encoder_sessions() {
                    Ok(list) => {
                        for s in list {
                            let codec = match s.codec_type {
                                nvml_wrapper::enum_wrappers::device::EncoderType::H264 => "H.264",
                                nvml_wrapper::enum_wrappers::device::EncoderType::HEVC => "HEVC",
                            };
                            e.sessions.push(EncSession {
                                app: app_name(s.pid, &sys),
                                codec: codec.into(),
                                res: format!("{}×{}", s.hres, s.vres),
                                fps: (s.average_fps > 0).then_some(s.average_fps),
                            });
                        }
                    }
                    // NVML can't describe some sessions (AV1): count them instead.
                    Err(_) => {
                        if let Ok(st) = d.encoder_stats() {
                            for _ in 0..st.session_count {
                                e.sessions.push(EncSession {
                                    app: "A program".into(),
                                    codec: "AV1 or other".into(),
                                    fps: (st.average_fps > 0).then_some(st.average_fps),
                                    ..Default::default()
                                });
                            }
                        }
                    }
                }
            }
            nv_index += 1;
        } else {
            #[cfg(not(windows))]
            if !drm_done {
                drm_done = true;
                let (busy, users) = drm_encode_users(&sys);
                e.can_tell = busy.is_some();
                e.busy = busy;
                e.sessions = users;
            }
            #[cfg(windows)]
            if !win_done {
                win_done = true;
                let (busy, users) = windows_encode_users(&sys);
                e.can_tell = busy.is_some();
                e.busy = busy;
                e.sessions = users;
            }
        }
        out.push(e);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codecs_by_generation() {
        assert_eq!(
            codecs_for(Vendor::Nvidia, "NVIDIA GeForce RTX 5090"),
            vec!["H.264", "HEVC", "AV1"]
        );
        assert_eq!(
            codecs_for(Vendor::Nvidia, "NVIDIA GeForce RTX 3080"),
            vec!["H.264", "HEVC"]
        );
        assert_eq!(
            codecs_for(Vendor::Amd, "AMD Radeon RX 7900 XTX"),
            vec!["H.264", "HEVC", "AV1"]
        );
        assert_eq!(
            codecs_for(Vendor::Amd, "AMD Granite Ridge [Radeon 610M]"),
            vec!["H.264", "HEVC"]
        );
        assert_eq!(friendly("obs"), "OBS");
        assert_eq!(friendly("Discord.exe"), "Discord");
        let e = EncoderInfo {
            can_tell: true,
            sessions: vec![EncSession {
                app: "OBS".into(),
                codec: "H.264".into(),
                res: "1920×1080".into(),
                fps: Some(60),
            }],
            ..Default::default()
        };
        assert_eq!(
            e.use_text(),
            "In use: 1 encode: OBS (H.264 1920×1080 60 fps)"
        );
    }
}
