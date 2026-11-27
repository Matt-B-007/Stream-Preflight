//! Local history: readiness scores, internet results, saved setups, and the
//! original value of every game setting we've seen. Stays on this PC.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Write;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub time: i64,
    pub kind: String,
    pub score: u8,
    pub upload_avg: Option<f32>,
    pub upload_low: Option<f32>,
    pub gpu_avg: Option<f32>,
    pub cpu_avg: Option<f32>,
    pub game: Option<String>,
    /// Share of stream frames OBS skipped during a test, in percent.
    #[serde(default)]
    pub skipped_pct: Option<f32>,
    /// Share of the test the GPU spent at 95%+, in percent.
    #[serde(default)]
    pub maxed_pct: Option<f32>,
    /// The two scores shown in the app (newer entries).
    #[serde(default)]
    pub smooth: Option<u8>,
    #[serde(default)]
    pub setup: Option<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Original {
    pub first_value: String,
    pub first_seen: i64,
    pub last_value: String,
    pub last_seen: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedSetup {
    pub name: String,
    pub time: i64,
    pub score: u8,
    pub style: String,
    pub output: String,
    pub encoder: String,
    pub notes: String,
}

#[derive(Default, Serialize, Deserialize)]
struct Store {
    format_version: u32,
    originals: BTreeMap<String, BTreeMap<String, Original>>,
    setups: Vec<SavedSetup>,
}

pub struct History {
    pub entries: Vec<Entry>,
    pub originals: BTreeMap<String, BTreeMap<String, Original>>,
    pub setups: Vec<SavedSetup>,
}

fn entries_path() -> std::path::PathBuf {
    crate::util::dirs().data.join("history.jsonl")
}
fn store_path() -> std::path::PathBuf {
    crate::util::dirs().data.join("history-store.json")
}

impl History {
    pub fn load() -> Self {
        let entries = std::fs::read_to_string(entries_path())
            .map(|s| {
                s.lines()
                    .filter_map(|l| serde_json::from_str::<Entry>(l).ok())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let store: Store = std::fs::read_to_string(store_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let mut h = Self {
            entries,
            originals: store.originals,
            setups: store.setups,
        };
        if h.entries.len() > 500 {
            h.entries.drain(..h.entries.len() - 500);
        }
        h
    }

    pub fn add(&mut self, e: Entry) {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(entries_path())
            && let Ok(s) = serde_json::to_string(&e)
        {
            let _ = writeln!(f, "{s}");
        }
        self.entries.push(e);
    }

    pub fn note_settings(&mut self, games: &[crate::games::GameResult]) {
        let now = crate::util::unix_now();
        for g in games {
            for f in &g.findings {
                let m = self.originals.entry(g.name.clone()).or_default();
                m.entry(f.label.clone())
                    .and_modify(|o| {
                        o.last_value = f.value.clone();
                        o.last_seen = now;
                    })
                    .or_insert(Original {
                        first_value: f.value.clone(),
                        first_seen: now,
                        last_value: f.value.clone(),
                        last_seen: now,
                    });
            }
        }
        self.save_store();
    }

    pub fn save_setup(&mut self, s: SavedSetup) {
        self.setups.push(s);
        self.save_store();
    }

    fn save_store(&self) {
        let store = Store {
            format_version: 1,
            originals: self.originals.clone(),
            setups: self.setups.clone(),
        };
        if let Ok(s) = serde_json::to_string_pretty(&store) {
            let _ = std::fs::write(store_path(), s);
        }
    }

    pub fn clear(&mut self) {
        let _ = std::fs::remove_file(entries_path());
        let _ = std::fs::remove_file(store_path());
        self.entries.clear();
        self.originals.clear();
        self.setups.clear();
        log::info!("History cleared by the user");
    }

    /// Average upload by hour of day, from quick scans.
    pub fn upload_by_hour(&self) -> Vec<(u32, f32, usize)> {
        let mut acc: BTreeMap<u32, (f32, usize)> = BTreeMap::new();
        for e in &self.entries {
            if let Some(u) = e.upload_low {
                let h = chrono::DateTime::from_timestamp(e.time, 0)
                    .map(|d| {
                        d.with_timezone(&chrono::Local)
                            .format("%H")
                            .to_string()
                            .parse::<u32>()
                            .unwrap_or(0)
                    })
                    .unwrap_or(0);
                let v = acc.entry(h).or_insert((0.0, 0));
                v.0 += u;
                v.1 += 1;
            }
        }
        acc.into_iter()
            .map(|(h, (s, n))| (h, s / n as f32, n))
            .collect()
    }
}

pub fn fmt_time(t: i64) -> String {
    chrono::DateTime::from_timestamp(t, 0)
        .map(|d| {
            d.with_timezone(&chrono::Local)
                .format("%b %d, %H:%M")
                .to_string()
        })
        .unwrap_or_default()
}
