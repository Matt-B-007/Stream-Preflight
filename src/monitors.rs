// SPDX-License-Identifier: GPL-3.0-or-later
//! Connected monitors with their current and highest refresh rates.
//! Linux: KDE's kscreen-doctor, then xrandr. Windows: display settings API.

use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Monitor {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub current_hz: u32,
    pub max_hz: u32,
    pub primary: bool,
}

impl Monitor {
    /// Running noticeably below what it can do at this resolution.
    pub fn underclocked(&self) -> bool {
        self.max_hz > self.current_hz + 5
    }
}

pub fn detect() -> Vec<Monitor> {
    let mut v = platform();
    v.sort_by_key(|m| !m.primary);
    log::info!(
        "Monitors: {}",
        v.iter()
            .map(|m| format!(
                "{} {}x{} @{}Hz (max {}Hz){}",
                m.name,
                m.width,
                m.height,
                m.current_hz,
                m.max_hz,
                if m.primary { " primary" } else { "" }
            ))
            .collect::<Vec<_>>()
            .join(", ")
    );
    v
}

#[cfg(not(windows))]
fn platform() -> Vec<Monitor> {
    let k = kscreen();
    if !k.is_empty() {
        return k;
    }
    xrandr()
}

#[cfg(not(windows))]
fn kscreen() -> Vec<Monitor> {
    let Ok(o) = std::process::Command::new("kscreen-doctor")
        .arg("-j")
        .output()
    else {
        return Vec::new();
    };
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(&o.stdout) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for op in v["outputs"].as_array().cloned().unwrap_or_default() {
        if op["connected"].as_bool() == Some(false) || op["enabled"].as_bool() == Some(false) {
            continue;
        }
        let cur_id = match &op["currentModeId"] {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        let modes = op["modes"].as_array().cloned().unwrap_or_default();
        let id_of = |m: &serde_json::Value| match &m["id"] {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        let Some(cur) = modes.iter().find(|m| id_of(m) == cur_id) else {
            continue;
        };
        let w = cur["size"]["width"].as_u64().unwrap_or(0) as u32;
        let h = cur["size"]["height"].as_u64().unwrap_or(0) as u32;
        let cur_hz = cur["refreshRate"].as_f64().unwrap_or(0.0).round() as u32;
        let max_hz = modes
            .iter()
            .filter(|m| {
                m["size"]["width"].as_u64() == Some(w as u64)
                    && m["size"]["height"].as_u64() == Some(h as u64)
            })
            .filter_map(|m| m["refreshRate"].as_f64())
            .fold(0.0f64, f64::max)
            .round() as u32;
        let name = op["name"].as_str().unwrap_or("Monitor").to_string();
        let primary = op["priority"].as_u64() == Some(1) || op["primary"].as_bool() == Some(true);
        out.push(Monitor {
            name,
            width: w,
            height: h,
            current_hz: cur_hz,
            max_hz: max_hz.max(cur_hz),
            primary,
        });
    }
    out
}

#[cfg(not(windows))]
fn xrandr() -> Vec<Monitor> {
    let Ok(o) = std::process::Command::new("xrandr").arg("--query").output() else {
        return Vec::new();
    };
    parse_xrandr(&String::from_utf8_lossy(&o.stdout))
}

pub fn parse_xrandr(text: &str) -> Vec<Monitor> {
    let mut out: Vec<Monitor> = Vec::new();
    let mut cur: Option<Monitor> = None;
    for line in text.lines() {
        if !line.starts_with(' ') {
            if let Some(m) = cur.take()
                && m.current_hz > 0
            {
                out.push(m);
            }
            if line.contains(" connected") {
                let name = line
                    .split_whitespace()
                    .next()
                    .unwrap_or("Monitor")
                    .to_string();
                cur = Some(Monitor {
                    name,
                    width: 0,
                    height: 0,
                    current_hz: 0,
                    max_hz: 0,
                    primary: line.contains(" primary "),
                });
            }
            continue;
        }
        let Some(m) = cur.as_mut() else { continue };
        let mut parts = line.split_whitespace();
        let Some(res) = parts.next() else { continue };
        let Some((w, h)) = res.split_once('x') else {
            continue;
        };
        let (w, h): (u32, u32) = (
            w.parse().unwrap_or(0),
            h.trim_end_matches(|c: char| !c.is_ascii_digit())
                .parse()
                .unwrap_or(0),
        );
        let rates: Vec<(f64, bool)> = parts
            .filter_map(|r| {
                let current = r.contains('*');
                r.trim_end_matches(['*', '+'])
                    .parse::<f64>()
                    .ok()
                    .map(|v| (v, current))
            })
            .collect();
        if let Some((hz, _)) = rates.iter().find(|(_, c)| *c) {
            m.width = w;
            m.height = h;
            m.current_hz = hz.round() as u32;
        }
        if m.width == w && m.height == h || m.current_hz == 0 {
            let top = rates.iter().map(|(v, _)| *v).fold(0.0, f64::max).round() as u32;
            if m.width == w && m.height == h {
                m.max_hz = m.max_hz.max(top);
            }
        }
    }
    if let Some(m) = cur.take()
        && m.current_hz > 0
    {
        out.push(m);
    }
    for m in out.iter_mut() {
        m.max_hz = m.max_hz.max(m.current_hz);
    }
    out
}

#[cfg(windows)]
fn platform() -> Vec<Monitor> {
    crate::winsys::monitors()
        .into_iter()
        .map(|(name, w, h, cur, max, primary)| Monitor {
            name,
            width: w,
            height: h,
            current_hz: cur,
            max_hz: max.max(cur),
            primary,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn xrandr_parse() {
        let t = "Screen 0: minimum 16 x 16\nDP-1 connected primary 2560x1440+0+0 (normal) 600mm x 340mm\n   2560x1440    359.97*+ 239.97   143.97\n   1920x1080     60.00\nHDMI-1 connected 2560x1440+2560+0\n   2560x1440     59.95*+ 143.91\nHDMI-2 disconnected\n";
        let m = parse_xrandr(t);
        assert_eq!(m.len(), 2);
        assert_eq!(
            (m[0].current_hz, m[0].max_hz, m[0].primary),
            (360, 360, true)
        );
        assert_eq!((m[1].current_hz, m[1].max_hz), (60, 144));
        assert!(m[1].underclocked());
    }
}
