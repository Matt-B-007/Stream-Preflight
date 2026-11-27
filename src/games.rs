//! Game profiles: data files that say where each game keeps its settings,
//! how to read them, and which settings cost the most performance.
//! Settings files are only ever READ, never written.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const PROFILE_FILES: &[&str] = &[
    include_str!("profiles/fortnite.json"),
    include_str!("profiles/cs2.json"),
    include_str!("profiles/cyberpunk2077.json"),
    include_str!("profiles/halo_infinite.json"),
    include_str!("profiles/halo_mcc.json"),
    include_str!("profiles/cities_skylines.json"),
    include_str!("profiles/cities_skylines_2.json"),
    include_str!("profiles/valorant.json"),
    include_str!("profiles/apex.json"),
    include_str!("profiles/warzone.json"),
    include_str!("profiles/league.json"),
    include_str!("profiles/minecraft.json"),
    include_str!("profiles/marvel_rivals.json"),
    include_str!("profiles/elden_ring.json"),
    include_str!("profiles/gta5.json"),
];

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SettingsFile {
    pub windows: Option<String>,
    pub linux: Option<String>,
    /// Path inside a Proton prefix, relative to drive_c/users/steamuser/
    pub proton: Option<String>,
    pub format: String,
    /// An older place the game no longer uses: "not found" there is normal.
    #[serde(default)]
    pub legacy: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FindingRule {
    pub id: String,
    pub keys: Vec<String>,
    pub when: String,
    pub effect: String,
    pub label: String,
    pub menu: String,
    #[serde(default)]
    pub change_to: Option<String>,
    #[serde(default)]
    pub gain: Option<String>,
    #[serde(default)]
    pub cost: Option<String>,
    /// Plain name of the setting, e.g. "FPS limit". Defaults to the label
    /// without "is on/off".
    #[serde(default)]
    pub setting: Option<String>,
    /// Extra keys whose value is shown next to the setting (e.g. the FPS limit number).
    #[serde(default)]
    pub value_keys: Vec<String>,
    /// Only counts when this "main switch" key is on (e.g. ray tracing).
    #[serde(default)]
    pub requires: Option<String>,
}

/// The game's quality preset ("Quick preset: Ultra").
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PresetRule {
    pub key: String,
    pub setting: String,
    pub menu: String,
    /// Raw values from lowest to highest.
    pub levels: Vec<String>,
    /// Raw values meaning "the player changed individual settings".
    #[serde(default)]
    pub custom: Vec<String>,
}

/// One heavy individual setting with known levels, lowest to highest.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Tunable {
    pub id: String,
    pub key: String,
    pub setting: String,
    pub menu: String,
    pub levels: Vec<String>,
    /// The level Preflight suggests when the GPU is maxed.
    pub target: String,
    pub gain: String,
    pub cost: String,
    #[serde(default)]
    pub requires: Option<String>,
    /// What the menu calls each level (same order as `levels`), when the
    /// file stores numbers.
    #[serde(default)]
    pub level_labels: Vec<String>,
}

impl Tunable {
    fn label(&self, raw: &str, labels: &BTreeMap<String, String>) -> String {
        match self
            .levels
            .iter()
            .position(|l| l.eq_ignore_ascii_case(raw))
            .and_then(|i| self.level_labels.get(i))
        {
            Some(l) => l.clone(),
            None => label_of(labels, raw),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub load: String,
    /// How much the picture moves: "high" (shooters, racing), "medium",
    /// "low" (city builders). Fast motion needs more stream bitrate.
    #[serde(default = "medium")]
    pub motion: String,
    pub profile_version: u32,
    pub verified: String,
    pub steam_appid: Option<u32>,
    pub linux_supported: bool,
    pub processes: Vec<String>,
    #[serde(default)]
    pub detect_windows: Vec<String>,
    pub settings_files: Vec<SettingsFile>,
    pub findings: Vec<FindingRule>,
    #[serde(default)]
    pub preset: Option<PresetRule>,
    /// Heavy settings listed in the order to lower them (most gain, least loss first).
    #[serde(default)]
    pub tunables: Vec<Tunable>,
    /// Raw value -> what the menu says, for values the generic cleaner can't handle.
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    /// The game has an FPS limit in its own menus (Cities: Skylines II doesn't).
    #[serde(default = "yes")]
    pub fps_limit: bool,
    /// Graphics API on Windows/Proton ("dx11", "dx12", "vulkan"): decides
    /// which outside-the-game FPS limiter fits.
    #[serde(default)]
    pub api: Option<String>,
    /// The settings file only stores settings the player changed: a missing
    /// setting is at its default, not unreadable.
    #[serde(default)]
    pub sparse_file: bool,
    /// Where the game stores its display mode.
    #[serde(default)]
    pub display: Option<DisplayRule>,
}

fn medium() -> String {
    "medium".into()
}

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DisplayRule {
    /// "2560x1440"-style resolution key.
    #[serde(default)]
    pub resolution: Option<String>,
    #[serde(default)]
    pub refresh: Option<String>,
    /// Width and height stored separately (CS2).
    #[serde(default)]
    pub width: Option<String>,
    #[serde(default)]
    pub height: Option<String>,
    /// Divide the stored refresh by this (Cities stores 60000 for 60 Hz).
    #[serde(default = "one")]
    pub refresh_scale: f64,
}

fn one() -> f64 {
    1.0
}

pub fn profiles() -> Vec<Profile> {
    // Parsed once; every scan and poll reuses them.
    static CACHE: std::sync::OnceLock<Vec<Profile>> = std::sync::OnceLock::new();
    CACHE
        .get_or_init(|| {
            PROFILE_FILES
                .iter()
                .filter_map(|s| match serde_json::from_str::<Profile>(s) {
                    Ok(p) => Some(p),
                    Err(e) => {
                        log::error!("Built-in game profile failed to load: {e}");
                        None
                    }
                })
                .collect()
        })
        .clone()
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct FileResult {
    /// Path with the user's home folder replaced, safe to show in reports.
    pub shown_path: String,
    pub found: bool,
    pub format: String,
    pub read_ok: bool,
    pub settings_count: usize,
    pub error: Option<String>,
    /// Every place that was checked, in order (paths scrubbed).
    pub searched: Vec<String>,
    pub note: Option<String>,
    /// When the game last saved this file (unix time).
    pub modified: Option<i64>,
    #[serde(skip)]
    pub real_path: Option<PathBuf>,
    /// Settings that don't live in a file (Unity's registry values), already
    /// made safe for reports.
    #[serde(skip)]
    pub inline_text: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Finding {
    pub rule_id: String,
    pub effect: String,
    pub label: String,
    pub menu: String,
    pub key: String,
    pub value: String,
    pub change_to: Option<String>,
    pub gain: Option<String>,
    pub cost: Option<String>,
}

/// A setting Preflight knows about and its current value, flagged or not.
#[derive(Clone, Debug, Serialize)]
pub struct CurrentSetting {
    pub rule_id: String,
    pub setting: String,
    pub effect: String,
    pub menu: String,
    /// Shown value, e.g. "On", "Off", "Quality", "240".
    pub value: String,
    pub flagged: bool,
    pub change_to: Option<String>,
    pub gain: Option<String>,
    pub cost: Option<String>,
}

/// The game's preset and what to step down to.
#[derive(Clone, Debug, Serialize)]
pub struct PresetState {
    pub setting: String,
    pub menu: String,
    pub value: String,
    pub custom: bool,
    /// One level lower, if there is one.
    pub lower: Option<String>,
}

/// A heavy setting's current level and Preflight's suggestion.
#[derive(Clone, Debug, Serialize)]
pub struct TunableState {
    pub id: String,
    pub setting: String,
    pub menu: String,
    pub now: String,
    /// Set when the current level is above the suggested one.
    pub change_to: Option<String>,
    pub gain: String,
    pub cost: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct GameResult {
    pub id: String,
    pub name: String,
    pub load: String,
    pub profile_version: u32,
    pub verified: String,
    pub installed: bool,
    pub install_hint: String,
    pub running: bool,
    pub platform_note: Option<String>,
    pub files: Vec<FileResult>,
    pub findings: Vec<Finding>,
    /// Rule keys that matched nothing in the file: helps fix profiles.
    pub unmatched_rules: Vec<String>,
    /// Setting names found (no values), only kept when a rule matched
    /// nothing, to help fix the profile.
    pub key_names: Vec<String>,
    /// Every known setting with its current value (for "Your setup").
    pub current: Vec<CurrentSetting>,
    pub preset: Option<PresetState>,
    pub tunables: Vec<TunableState>,
    /// The in-game FPS limit, when one is set.
    pub fps_cap: Option<u32>,
    /// Plain names of settings Preflight expected but couldn't find
    /// (usually a game update renamed them).
    pub unreadable: Vec<String>,
    /// The game has its own FPS limit setting.
    pub has_fps_limit: bool,
    pub api: Option<String>,
    /// Runs through Proton (its settings live in a compatdata prefix).
    pub proton: bool,
    /// The game's own display mode, e.g. "2560x1440" and 60 Hz.
    pub display_res: Option<String>,
    pub display_hz: Option<f32>,
    /// Set when there's no hand-made profile and the settings were read
    /// automatically: the engine ("Unity", "Unreal"...) or "automatic".
    pub auto: Option<String>,
    /// Program names of the game (lower case).
    pub processes: Vec<String>,
    /// How many settings were read, and how many the advice uses.
    pub settings_read: usize,
    pub settings_used: usize,
    /// What couldn't be read, in plain words.
    pub read_gap: Option<String>,
    /// "Steam", "Heroic", "Lutris"... (where to add launch options).
    pub launcher: String,
    /// "high", "medium" or "low" motion.
    pub motion: String,
}

impl GameResult {
    pub fn relevant(&self) -> bool {
        self.installed || self.running || self.files.iter().any(|f| f.found)
    }
}

// ---------------------------------------------------------------------------
// Steam libraries

pub fn steam_roots() -> Vec<PathBuf> {
    let mut v = Vec::new();
    #[cfg(windows)]
    {
        if let Some(p) = crate::winsys::steam_path() {
            v.push(p);
        }
        v.push(PathBuf::from(r"C:\Program Files (x86)\Steam"));
    }
    #[cfg(not(windows))]
    if let Some(h) = crate::util::home_dir() {
        v.push(h.join(".steam/steam"));
        v.push(h.join(".local/share/Steam"));
        v.push(h.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"));
    }
    let mut out: Vec<PathBuf> = Vec::new();
    for p in v {
        if p.join("steamapps").exists() {
            let c = std::fs::canonicalize(&p).unwrap_or(p);
            if !out.contains(&c) {
                out.push(c);
            }
        }
    }
    out
}

/// Every Steam library folder (the main one plus extra drives).
pub fn steam_libraries() -> Vec<PathBuf> {
    let mut libs: Vec<PathBuf> = Vec::new();
    for root in steam_roots() {
        if !libs.contains(&root) {
            libs.push(root.clone());
        }
        let vdf = root.join("steamapps/libraryfolders.vdf");
        if let Ok(s) = std::fs::read_to_string(&vdf) {
            for line in s.lines() {
                let t = line.trim();
                if t.starts_with("\"path\"") {
                    let parts: Vec<&str> = t.split('"').collect();
                    if let Some(p) = parts.get(3) {
                        let pb = PathBuf::from(p.replace("\\\\", "\\"));
                        let c = std::fs::canonicalize(&pb).unwrap_or(pb);
                        if c.join("steamapps").exists() && !libs.contains(&c) {
                            libs.push(c);
                        }
                    }
                }
            }
        }
    }
    libs
}

fn steam_installed(appid: u32, libs: &[PathBuf]) -> Option<PathBuf> {
    libs.iter()
        .find(|l| {
            l.join(format!("steamapps/appmanifest_{appid}.acf"))
                .exists()
        })
        .cloned()
}

// ---------------------------------------------------------------------------
// Path expansion (environment variables, ~, {steam}, and * folders)

fn expand(pattern: &str, steam_roots: &[PathBuf], libs: &[PathBuf]) -> Vec<PathBuf> {
    let mut bases: Vec<String> = Vec::new();
    if pattern.contains("{steam}") {
        for r in steam_roots {
            bases.push(pattern.replace("{steam}", &r.to_string_lossy()));
        }
    } else if pattern.contains("{steamlib}") {
        for l in libs {
            bases.push(pattern.replace("{steamlib}", &l.to_string_lossy()));
        }
    } else {
        bases.push(pattern.to_string());
    }
    let mut out = Vec::new();
    for b in bases {
        let mut s = b;
        for var in ["LOCALAPPDATA", "APPDATA", "USERPROFILE", "PROGRAMDATA"] {
            let tag = format!("%{var}%");
            if s.contains(&tag) {
                match std::env::var(var) {
                    Ok(v) => s = s.replace(&tag, &v),
                    Err(_) => continue,
                }
            }
        }
        if s.starts_with('%') {
            continue;
        }
        if let Some(rest) = s.strip_prefix("~/")
            && let Some(h) = crate::util::home_dir()
        {
            s = h.join(rest).to_string_lossy().to_string();
        }
        out.extend(glob(&s));
    }
    out
}

/// Expands '*' path segments by listing folders. No other wildcards.
fn glob(path: &str) -> Vec<PathBuf> {
    let norm = path.replace('\\', "/");
    if !norm.contains('*') {
        return vec![PathBuf::from(if cfg!(windows) {
            norm.replace('/', "\\")
        } else {
            norm
        })];
    }
    let mut parts = norm.split('/').peekable();
    let mut current: Vec<PathBuf> = vec![];
    let first = parts.next().unwrap_or("");
    current.push(if first.is_empty() {
        PathBuf::from("/")
    } else {
        PathBuf::from(format!("{first}/"))
    });
    for seg in parts {
        if seg.is_empty() {
            continue;
        }
        let mut next = Vec::new();
        for c in &current {
            if seg == "*" {
                if let Ok(rd) = std::fs::read_dir(c) {
                    for e in rd.flatten() {
                        if e.path().is_dir() {
                            next.push(e.path());
                        }
                    }
                }
            } else if seg.contains('*') {
                // "options*.cst": a name pattern (files or folders).
                if let Ok(rd) = std::fs::read_dir(c) {
                    let mut hits: Vec<PathBuf> = rd
                        .flatten()
                        .filter(|e| wild_match(&e.file_name().to_string_lossy(), seg))
                        .map(|e| e.path())
                        .collect();
                    hits.sort();
                    next.extend(hits);
                }
            } else {
                next.push(c.join(seg));
            }
        }
        current = next;
        if current.len() > 64 {
            current.truncate(64);
        }
    }
    current
}

/// Case-insensitive match with '*' wildcards.
fn wild_match(name: &str, pat: &str) -> bool {
    let n = name.to_lowercase();
    let p = pat.to_lowercase();
    let parts: Vec<&str> = p.split('*').collect();
    let mut pos = 0usize;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i == 0 {
            if !n.starts_with(part) {
                return false;
            }
            pos = part.len();
        } else if let Some(f) = n[pos..].find(part) {
            pos += f + part.len();
        } else {
            return false;
        }
    }
    parts
        .last()
        .map(|l| l.is_empty() || n.ends_with(l))
        .unwrap_or(true)
}

fn shown(p: &Path) -> String {
    crate::util::scrub(&p.to_string_lossy())
}

// ---------------------------------------------------------------------------
// Settings file parsers. Every parser produces a flat "name -> value" map.

pub fn parse(format: &str, text: &str) -> Result<BTreeMap<String, String>, String> {
    match format {
        "ini" => Ok(parse_ini(text)),
        "json" => parse_json(text),
        "kv" => Ok(parse_kv(text)),
        "lines" => Ok(parse_lines(text)),
        "xml" => Ok(parse_xml(text)),
        "coc" => parse_coc(text),
        other => Err(format!("format '{other}' can't be read yet")),
    }
}

fn parse_ini(text: &str) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    let mut section = String::new();
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with(';') || t.starts_with('#') {
            continue;
        }
        if t.starts_with('[') && t.ends_with(']') {
            section = t[1..t.len() - 1].to_string();
            continue;
        }
        if let Some((k, v)) = t.split_once('=') {
            let key = if section.is_empty() {
                k.trim().to_string()
            } else {
                format!("{}/{}", section, k.trim())
            };
            m.insert(key, v.trim().to_string());
        }
    }
    m
}

fn parse_json(text: &str) -> Result<BTreeMap<String, String>, String> {
    let t = text.trim_start_matches('\u{feff}');
    let v: serde_json::Value = serde_json::from_str(t).map_err(|e| format!("JSON error: {e}"))?;
    let mut m = BTreeMap::new();
    flatten_json("", &v, &mut m);
    Ok(m)
}

fn flatten_json(prefix: &str, v: &serde_json::Value, m: &mut BTreeMap<String, String>) {
    use serde_json::Value as V;
    match v {
        V::Object(o) => {
            // {"name": "...", "value": ...} pairs (used by Cyberpunk and others)
            if let (Some(V::String(n)), Some(val)) = (o.get("name"), o.get("value")) {
                let key = if prefix.is_empty() {
                    n.clone()
                } else {
                    format!("{prefix}/{n}")
                };
                m.insert(key, scalar(val));
                return;
            }
            // {"value": ..., "version": ...} (used by Halo Infinite): the
            // setting's value belongs to the parent name.
            if o.contains_key("value")
                && o.contains_key("version")
                && !prefix.is_empty()
                && o.len() <= 3
                && let Some(val) = o.get("value")
                && !val.is_object()
                && !val.is_array()
            {
                m.insert(prefix.to_string(), scalar(val));
                return;
            }
            let group = o
                .get("group_name")
                .and_then(|g| g.as_str())
                .map(|s| s.trim_matches('/').to_string());
            for (k, val) in o {
                if k == "group_name" {
                    continue;
                }
                let p = match (&group, prefix.is_empty()) {
                    (Some(g), _) => g.clone(),
                    (None, true) => k.clone(),
                    (None, false) => format!("{prefix}/{k}"),
                };
                let p = if group.is_some() && !matches!(val, V::Array(_)) {
                    format!("{p}/{k}")
                } else {
                    p
                };
                flatten_json(&p, val, m);
            }
        }
        V::Array(a) => {
            for (i, val) in a.iter().enumerate() {
                if val.is_object() {
                    flatten_json(prefix, val, m);
                } else {
                    m.insert(format!("{prefix}[{i}]"), scalar(val));
                }
            }
        }
        _ => {
            m.insert(prefix.to_string(), scalar(v));
        }
    }
}

fn scalar(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Cities: Skylines II "Settings.coc": a section name on its own line, then a
/// JSON object. Lists of objects tagged with "@type" are keyed by that type,
/// e.g. "Graphics Settings/DepthOfFieldQualitySettings/enabled".
fn parse_coc(text: &str) -> Result<BTreeMap<String, String>, String> {
    let mut m = BTreeMap::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    let mut sections = 0;
    while i < lines.len() {
        let name = lines[i].trim();
        if name.is_empty() || name.starts_with('{') || name.starts_with('}') {
            i += 1;
            continue;
        }
        // Find the JSON block that follows.
        let mut j = i + 1;
        while j < lines.len() && lines[j].trim().is_empty() {
            j += 1;
        }
        if j >= lines.len() || !lines[j].trim_start().starts_with('{') {
            i += 1;
            continue;
        }
        let mut depth = 0i32;
        let mut block = String::new();
        let mut k = j;
        while k < lines.len() {
            for ch in lines[k].chars() {
                match ch {
                    '{' | '[' => depth += 1,
                    '}' | ']' => depth -= 1,
                    _ => {}
                }
            }
            block.push_str(lines[k]);
            block.push('\n');
            k += 1;
            if depth <= 0 {
                break;
            }
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&block) {
            flatten_coc(name, &v, &mut m);
            sections += 1;
        }
        i = k;
    }
    if sections == 0 {
        return Err("no sections found".into());
    }
    Ok(m)
}

fn flatten_coc(prefix: &str, v: &serde_json::Value, m: &mut BTreeMap<String, String>) {
    use serde_json::Value as V;
    match v {
        V::Object(o) => {
            for (k, val) in o {
                if k == "@type" {
                    continue;
                }
                flatten_coc(&format!("{prefix}/{k}"), val, m);
            }
        }
        V::Array(a) => {
            for (i, val) in a.iter().enumerate() {
                let typed = val
                    .get("@type")
                    .and_then(|t| t.as_str())
                    .map(|t| t.rsplit('.').next().unwrap_or(t).to_string());
                match typed {
                    // Quality groups: drop the list name, key by the group type.
                    Some(t) => flatten_coc(
                        &format!(
                            "{}/{t}",
                            prefix.rsplit_once('/').map(|x| x.0).unwrap_or(prefix)
                        ),
                        val,
                        m,
                    ),
                    None if val.is_object() => flatten_coc(&format!("{prefix}[{i}]"), val, m),
                    None => {
                        m.insert(format!("{prefix}[{i}]"), scalar(val));
                    }
                }
            }
        }
        _ => {
            m.insert(prefix.to_string(), scalar(v));
        }
    }
}

/// Simple XML: `<A><B>text</B></A>` gives "A/B" = text, and
/// `<B value="3" />` gives "A/B" = 3 (GTA V style). Enough for settings files.
fn parse_xml(text: &str) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    let mut stack: Vec<String> = Vec::new();
    let mut rest = text;
    while let Some(lt) = rest.find('<') {
        let before = &rest[..lt];
        let after = &rest[lt + 1..];
        let Some(gt) = after.find('>') else { break };
        let tag = &after[..gt];
        rest = &after[gt + 1..];
        if tag.starts_with('?') || tag.starts_with('!') {
            continue;
        }
        if let Some(name) = tag.strip_prefix('/') {
            let t = before.trim();
            if !t.is_empty() && stack.last().map(|s| s == name.trim()).unwrap_or(false) {
                m.insert(stack.join("/"), t.to_string());
            }
            stack.pop();
            continue;
        }
        let self_closing = tag.ends_with('/');
        let body = tag.trim_end_matches('/');
        let name = body.split_whitespace().next().unwrap_or("").to_string();
        let value_attr = body
            .find("value=\"")
            .map(|i| &body[i + 7..])
            .and_then(|v| v.find('"').map(|j| v[..j].to_string()));
        stack.push(name);
        if let Some(v) = value_attr {
            m.insert(stack.join("/"), v);
        }
        if self_closing {
            stack.pop();
        }
    }
    m
}

/// Valve-style "key" "value" files.
fn parse_kv(text: &str) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    for line in text.lines() {
        let parts: Vec<&str> = line.split('"').collect();
        if parts.len() >= 5 {
            let k = parts[1].trim();
            let v = parts[3].trim();
            if !k.is_empty() {
                m.insert(k.to_string(), v.to_string());
            }
        }
    }
    m
}

/// Loose "key: value" / "key = value" lines, with "Section {" blocks.
fn parse_lines(text: &str) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    let mut section: Vec<String> = Vec::new();
    let mut pending: Option<String> = None;
    for line in text.lines() {
        let t = line.trim().trim_end_matches(',');
        if t.is_empty() {
            continue;
        }
        if t == "{" {
            if let Some(p) = pending.take() {
                section.push(p);
            }
            continue;
        }
        if t == "}" {
            section.pop();
            continue;
        }
        let split = t.find([':', '=']);
        match split {
            Some(i) => {
                let k = t[..i].trim().trim_matches('"');
                let v = t[i + 1..].trim().trim_matches('"');
                if v == "{" {
                    section.push(k.to_string());
                    continue;
                }
                let key = if section.is_empty() {
                    k.to_string()
                } else {
                    format!("{}/{}", section.join("/"), k)
                };
                m.insert(key, v.to_string());
            }
            None => {
                if let Some(stripped) = t.strip_suffix('{') {
                    section.push(stripped.trim().trim_matches('"').to_string());
                } else {
                    pending = Some(t.trim_matches('"').to_string());
                }
            }
        }
    }
    m
}

// ---------------------------------------------------------------------------
// Rule matching

/// A pattern matches if the key contains it. "a&b" needs both parts, and a
/// trailing "$" means the key must END with that text.
fn key_matches(key: &str, pattern: &str) -> bool {
    let k = key.to_lowercase().replace([' ', '_'], "");
    pattern.split('&').all(|part| {
        let p = part.to_lowercase().replace([' ', '_'], "");
        match p.strip_suffix('$') {
            Some(end) => k.ends_with(end),
            None => k.contains(&p),
        }
    })
}

fn num(v: &str) -> Option<f64> {
    v.trim().trim_matches('"').parse::<f64>().ok()
}

fn truthy(v: &str) -> bool {
    let l = v.trim().trim_matches('"').to_lowercase();
    matches!(l.as_str(), "true" | "on" | "yes" | "enabled")
        || num(&l).map(|n| n != 0.0).unwrap_or(false)
}

fn is_off(v: &str) -> bool {
    let l = v.trim().trim_matches('"').to_lowercase();
    matches!(
        l.as_str(),
        "off" | "false" | "0" | "none" | "disabled" | "native" | ""
    ) || l.ends_with("_off")
        || l.ends_with(" off")
}

fn condition(when: &str, v: &str) -> bool {
    if let Some(n) = when.strip_prefix("gte:") {
        return matches!((num(v), n.parse::<f64>()), (Some(a), Ok(b)) if a >= b);
    }
    if let Some(n) = when.strip_prefix("lte:") {
        return matches!((num(v), n.parse::<f64>()), (Some(a), Ok(b)) if a <= b);
    }
    if let Some(n) = when.strip_prefix("eq:") {
        return v.trim().eq_ignore_ascii_case(n);
    }
    match when {
        "truthy" => truthy(v),
        "falsy" => !truthy(v),
        "off" => is_off(v),
        _ => false,
    }
}

fn setting_name(r: &FindingRule) -> String {
    if let Some(s) = &r.setting {
        return s.clone();
    }
    let l = r.label.trim_end_matches('.');
    for suf in [" is on", " is off", " are on", " are off"] {
        if let Some(x) = l.strip_suffix(suf) {
            return x.to_string();
        }
    }
    l.to_string()
}

/// Turns raw config values into what the game's menu would say.
pub fn pretty_value(v: &str) -> String {
    let t = v.trim().trim_matches('"');
    match t.to_lowercase().as_str() {
        "true" | "1" | "on" | "yes" | "enabled" => "On".into(),
        "false" | "0" | "off" | "no" | "disabled" | "none" => "Off".into(),
        _ => {
            if let Ok(f) = t.parse::<f64>() {
                if f.fract() == 0.0 {
                    format!("{}", f as i64)
                } else {
                    format!("{f:.1}")
                }
            } else if t.starts_with("UI-") && t.contains('-') {
                // Cyberpunk stores menu text ids like "UI-Settings-Video-QualitySetting-Off".
                pretty_value(t.rsplit('-').next().unwrap_or(t))
            } else if !t.contains(' ')
                && !t.contains('_')
                && t.chars().skip(1).any(|c| c.is_uppercase())
                && t.chars().all(|c| c.is_ascii_alphabetic())
                && t.chars().filter(|c| c.is_uppercase()).count() < t.len() / 2
            {
                // "BorderlessWindowed" -> "Borderless windowed" (but leave "DLSS", "FSR" alone).
                let mut out = String::new();
                for (i, ch) in t.chars().enumerate() {
                    if i > 0 && ch.is_uppercase() {
                        out.push(' ');
                        out.extend(ch.to_lowercase());
                    } else {
                        out.push(ch);
                    }
                }
                out
            } else {
                let s = t.replace('_', " ");
                let mut c = s.chars();
                c.next()
                    .map(|f| format!("{}{}", f.to_uppercase(), c.as_str()))
                    .unwrap_or_default()
            }
        }
    }
}

/// Current value of every rule's setting, flagged or not.
fn current_settings(
    rules: &[FindingRule],
    settings: &BTreeMap<String, String>,
    findings: &[Finding],
) -> Vec<CurrentSetting> {
    let mut out = Vec::new();
    for r in rules {
        let flagged = findings.iter().find(|f| f.rule_id == r.id);
        if !main_switch_on(r.requires.as_deref(), settings) {
            continue;
        }
        let raw = flagged.map(|f| f.value.clone()).or_else(|| {
            if r.when == "off" {
                // Upscalers: show the one that's on, if any.
                settings
                    .iter()
                    .filter(|(k, _)| r.keys.iter().any(|p| key_matches(k, p)))
                    .find(|(_, v)| !is_off(v))
                    .map(|(k, v)| {
                        let name = k.rsplit('/').next().unwrap_or(k).to_uppercase();
                        format!("{name}: {}", pretty_value(v))
                    })
            } else {
                settings
                    .iter()
                    .find(|(k, _)| r.keys.iter().any(|p| key_matches(k, p)))
                    .map(|(_, v)| v.clone())
            }
        });
        let Some(raw) = raw else { continue };
        let mut value = if raw.contains(": ") {
            raw.clone()
        } else {
            pretty_value(&raw)
        };
        // An FPS limit of 0 (or less) means unlimited.
        if r.effect == "uncapped_fps"
            && raw
                .trim()
                .trim_matches('"')
                .parse::<f64>()
                .map(|n| n <= 0.0)
                .unwrap_or(false)
        {
            value = "Off (unlimited)".into();
        }
        if r.when == "off" && flagged.is_some() {
            value = "Off".into();
        }
        // An extra number, like the FPS limit value.
        if let Some(extra) = settings
            .iter()
            .find(|(k, _)| r.value_keys.iter().any(|p| key_matches(k, p)))
            .map(|(_, v)| pretty_value(v))
            && value == "On"
        {
            value = extra;
        }
        out.push(CurrentSetting {
            rule_id: r.id.clone(),
            setting: setting_name(r),
            effect: r.effect.clone(),
            menu: r.menu.clone(),
            value,
            flagged: flagged.is_some(),
            // The fix text only applies when the rule fired (8x MSAA → 4x), never to a setting that's already fine.
            change_to: r.change_to.clone().filter(|_| flagged.is_some()),
            gain: r.gain.clone().filter(|_| flagged.is_some()),
            cost: r.cost.clone().filter(|_| flagged.is_some()),
        });
    }
    out
}

/// A setting that depends on a main switch (ray tracing) only counts when
/// the switch is on. No switch, or the switch isn't in the file: counts.
fn main_switch_on(requires: Option<&str>, settings: &BTreeMap<String, String>) -> bool {
    match requires {
        None => true,
        Some(p) => settings
            .iter()
            .find(|(k, _)| key_matches(k, p))
            .map(|(_, v)| truthy(v))
            .unwrap_or(true),
    }
}

fn find_value<'a>(settings: &'a BTreeMap<String, String>, pattern: &str) -> Option<&'a String> {
    settings
        .iter()
        .find(|(k, _)| key_matches(k, pattern))
        .map(|(_, v)| v)
}

fn label_of(labels: &BTreeMap<String, String>, raw: &str) -> String {
    labels
        .get(raw)
        .cloned()
        .unwrap_or_else(|| pretty_value(raw))
}

/// The preset and one level lower.
fn preset_state(p: &Profile, settings: &BTreeMap<String, String>) -> Option<PresetState> {
    let r = p.preset.as_ref()?;
    let raw = find_value(settings, &r.key)?;
    let custom = r.custom.iter().any(|c| c.eq_ignore_ascii_case(raw));
    let idx = r.levels.iter().position(|l| l.eq_ignore_ascii_case(raw));
    let lower = idx
        .filter(|i| *i > 0)
        .map(|i| label_of(&p.labels, &r.levels[i - 1]));
    Some(PresetState {
        setting: r.setting.clone(),
        menu: r.menu.clone(),
        value: label_of(&p.labels, raw),
        custom: custom || idx.is_none(),
        lower,
    })
}

/// Each heavy setting's level, and whether it's above the suggested one.
fn tunable_states(p: &Profile, settings: &BTreeMap<String, String>) -> Vec<TunableState> {
    let mut out = Vec::new();
    for t in &p.tunables {
        if !main_switch_on(t.requires.as_deref(), settings) {
            continue;
        }
        let Some(raw) = find_value(settings, &t.key) else {
            continue;
        };
        let now = t.levels.iter().position(|l| l.eq_ignore_ascii_case(raw));
        let target = t
            .levels
            .iter()
            .position(|l| l.eq_ignore_ascii_case(&t.target));
        let change_to = match (now, target) {
            (Some(n), Some(g)) if n > g => Some(t.label(&t.levels[g], &p.labels)),
            _ => None,
        };
        out.push(TunableState {
            id: t.id.clone(),
            setting: t.setting.clone(),
            menu: t.menu.clone(),
            now: t.label(raw, &p.labels),
            change_to,
            gain: t.gain.clone(),
            cost: t.cost.clone(),
        });
    }
    out
}

/// The FPS limit number when the limit is on.
fn fps_cap_of(rules: &[FindingRule], settings: &BTreeMap<String, String>) -> Option<u32> {
    let r = rules.iter().find(|r| r.effect == "uncapped_fps")?;
    let (_, on) = settings
        .iter()
        .find(|(k, _)| r.keys.iter().any(|p| key_matches(k, p)))?;
    if r.value_keys.is_empty() {
        // The key itself holds the number (0 = no limit).
        return num(on)
            .filter(|n| *n > 0.0 && *n < 1000.0)
            .map(|n| n as u32);
    }
    if !truthy(on) {
        return None;
    }
    let v = settings
        .iter()
        .find(|(k, _)| r.value_keys.iter().any(|p| key_matches(k, p)))
        .map(|(_, v)| v)?;
    num(v).filter(|n| *n > 0.0).map(|n| n as u32)
}

/// The game's own resolution and refresh rate, when its file stores them.
fn display_of(p: &Profile, settings: &BTreeMap<String, String>) -> (Option<String>, Option<f32>) {
    let Some(d) = &p.display else {
        return (None, None);
    };
    let res = d
        .resolution
        .as_ref()
        .and_then(|k| find_value(settings, k))
        .map(|v| v.trim_matches('"').to_string())
        .filter(|v| v.contains('x'))
        .or_else(|| {
            let w = d
                .width
                .as_ref()
                .and_then(|k| find_value(settings, k))
                .and_then(|v| num(v))?;
            let h = d
                .height
                .as_ref()
                .and_then(|k| find_value(settings, k))
                .and_then(|v| num(v))?;
            (w >= 320.0 && h >= 200.0).then(|| format!("{}x{}", w as u32, h as u32))
        });
    let hz = d
        .refresh
        .as_ref()
        .and_then(|k| find_value(settings, k))
        .and_then(|v| num(v))
        .map(|n| (n / d.refresh_scale) as f32)
        .filter(|h| *h > 10.0 && *h < 1000.0);
    (res, hz)
}

/// Settings the profile expects but the file doesn't have.
fn unreadable_of(
    p: &Profile,
    settings: &BTreeMap<String, String>,
    unmatched: &[String],
) -> Vec<String> {
    if p.sparse_file {
        // Missing just means "at its default".
        return Vec::new();
    }
    let mut v: Vec<String> = p
        .findings
        .iter()
        .filter(|r| unmatched.contains(&r.id) && r.effect != "info")
        .map(setting_name)
        .collect();
    if let Some(r) = &p.preset
        && find_value(settings, &r.key).is_none()
    {
        v.push(r.setting.clone());
    }
    for t in &p.tunables {
        if find_value(settings, &t.key).is_none() {
            v.push(t.setting.clone());
        }
    }
    v
}

fn evaluate(
    rules: &[FindingRule],
    settings: &BTreeMap<String, String>,
) -> (Vec<Finding>, Vec<String>) {
    let mut findings = Vec::new();
    let mut unmatched = Vec::new();
    let mut claimed: std::collections::HashSet<String> = std::collections::HashSet::new();
    for r in rules {
        if !main_switch_on(r.requires.as_deref(), settings) {
            continue;
        }
        let matching: Vec<(&String, &String)> = settings
            .iter()
            .filter(|(k, _)| r.keys.iter().any(|p| key_matches(k, p)))
            .filter(|(k, _)| !claimed.contains(*k))
            .collect();
        if matching.is_empty() {
            unmatched.push(r.id.clone());
            continue;
        }
        let hit = if r.when == "off" {
            // "off" means: every matching key is off (no upscaler at all)
            if matching.iter().all(|(_, v)| is_off(v)) {
                matching.first().copied()
            } else {
                None
            }
        } else {
            matching
                .iter()
                .find(|(_, v)| condition(&r.when, v))
                .copied()
        };
        if let Some((k, v)) = hit {
            claimed.insert(k.clone());
            findings.push(Finding {
                rule_id: r.id.clone(),
                effect: r.effect.clone(),
                label: r.label.clone(),
                menu: r.menu.clone(),
                key: k.clone(),
                value: v.clone(),
                change_to: r.change_to.clone(),
                gain: r.gain.clone(),
                cost: r.cost.clone(),
            });
        }
    }
    (findings, unmatched)
}

/// Depth-limited search for a file name (case-insensitive). Reads folder
/// listings only.
#[cfg(not(windows))]
fn find_file(dir: &Path, name: &str, depth: u32, visited: &mut u32) -> Option<PathBuf> {
    if depth == 0 || *visited > 20_000 {
        return None;
    }
    let rd = std::fs::read_dir(dir).ok()?;
    let mut subdirs = Vec::new();
    for e in rd.flatten() {
        *visited += 1;
        let p = e.path();
        let ft = match e.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if ft.is_symlink() {
            continue;
        }
        if ft.is_file() && e.file_name().to_string_lossy().to_lowercase() == name {
            return Some(p);
        }
        if ft.is_dir() {
            subdirs.push(p);
        }
    }
    for d in subdirs {
        if let Some(h) = find_file(&d, name, depth - 1, visited) {
            return Some(h);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Scan

pub fn process_matches(proc_name: &str, wanted: &str) -> bool {
    let p = proc_name.to_lowercase();
    let w = wanted.to_lowercase();
    if p == w {
        return true;
    }
    // Linux truncates process names to 15 characters.
    p.len() >= 15 && w.starts_with(&p)
}

/// Reads games without a profile that are only known by program name.
pub fn read_unknown(hints: &[String]) -> Vec<GameResult> {
    let profs = profiles();
    let known_exes: Vec<String> = profs.iter().flat_map(|p| p.processes.clone()).collect();
    let known_ids: Vec<u32> = profs.iter().filter_map(|p| p.steam_appid).collect();
    crate::autoread::scan(
        &[],
        hints,
        &known_exes,
        &known_ids,
        &steam_roots(),
        &steam_libraries(),
    )
}

/// Every setting name a profile uses, so the "read vs used" count is honest.
fn profile_patterns(p: &Profile) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for r in &p.findings {
        v.extend(r.keys.iter().cloned());
        v.extend(r.value_keys.iter().cloned());
        v.extend(r.requires.iter().cloned());
    }
    if let Some(r) = &p.preset {
        v.push(r.key.clone());
    }
    for t in &p.tunables {
        v.push(t.key.clone());
        v.extend(t.requires.iter().cloned());
    }
    if let Some(d) = &p.display {
        v.extend(d.resolution.iter().cloned());
        v.extend(d.refresh.iter().cloned());
        v.extend(d.width.iter().cloned());
        v.extend(d.height.iter().cloned());
    }
    v
}

/// Reads every game with a profile, then games without one: those running
/// now (`running`) and those only known by program name (`hints`, e.g.
/// the game from the last test).
pub fn scan(
    procs: &[(String, f32)],
    running: &[crate::hardware::GameProc],
    hints: &[String],
    sys: &mut Option<sysinfo::System>,
) -> Vec<GameResult> {
    crate::logger::activity("game scan");
    let roots = steam_roots();
    let libs = steam_libraries();
    crate::logger::info_changed(
        "steamlibs",
        format!("Steam libraries found: {}", libs.len()),
    );
    let mut out = Vec::new();
    // Minecraft has its own reader (every launcher and modpack).
    out.extend(crate::minecraft::scan(sys));
    for p in profiles().into_iter().filter(|p| p.id != "minecraft") {
        let mut installed = false;
        let mut hint = String::new();
        let mut install_lib: Option<PathBuf> = None;
        if let Some(id) = p.steam_appid
            && let Some(lib) = steam_installed(id, &libs)
        {
            installed = true;
            hint = format!("Steam library {}", shown(&lib));
            install_lib = Some(lib);
        }
        // Check the library the game is installed in first.
        let mut ordered_libs: Vec<PathBuf> = install_lib.iter().cloned().collect();
        for l in &libs {
            if !ordered_libs.contains(l) {
                ordered_libs.push(l.clone());
            }
        }
        #[cfg(windows)]
        for d in &p.detect_windows {
            if expand(d, &roots, &libs).iter().any(|x| x.exists()) {
                installed = true;
                if hint.is_empty() {
                    hint = "settings file present".into();
                }
            }
        }
        let running = procs
            .iter()
            .any(|(n, _)| p.processes.iter().any(|w| process_matches(n, w)));
        let platform_note = if cfg!(not(windows)) && !p.linux_supported {
            Some(format!(
                "{} doesn't run on Linux yet, so this profile is Windows-only for now.",
                p.name
            ))
        } else {
            None
        };

        let mut files = Vec::new();
        let mut settings = BTreeMap::new();
        for sf in &p.settings_files {
            let mut candidates: Vec<PathBuf> = Vec::new();
            #[cfg(windows)]
            if let Some(w) = &sf.windows {
                candidates.extend(expand(w, &roots, &ordered_libs));
            }
            #[cfg(not(windows))]
            {
                if let Some(l) = &sf.linux {
                    candidates.extend(expand(l, &roots, &ordered_libs));
                }
                if let (Some(rel), Some(id)) = (&sf.proton, p.steam_appid) {
                    for lib in &ordered_libs {
                        candidates.push(
                            lib.join(format!(
                                "steamapps/compatdata/{id}/pfx/drive_c/users/steamuser"
                            ))
                            .join(rel),
                        );
                    }
                }
            }
            let mut searched: Vec<String> = Vec::new();
            for c in candidates.iter().map(|c| shown(c)) {
                if !searched.contains(&c) {
                    searched.push(c);
                }
            }
            let mut note = None;
            let mut found = candidates.iter().find(|c| c.is_file()).cloned();
            // Fallback: look for the file by name anywhere in the game's
            // Proton user folder (some games save somewhere unexpected).
            #[cfg(not(windows))]
            if found.is_none()
                && let (Some(rel), Some(id)) = (&sf.proton, p.steam_appid)
            {
                let fname = std::path::Path::new(rel)
                    .file_name()
                    .map(|f| f.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                for lib in &ordered_libs {
                    let base = lib.join(format!(
                        "steamapps/compatdata/{id}/pfx/drive_c/users/steamuser"
                    ));
                    if let Some(hit) = find_file(&base, &fname, 9, &mut 0) {
                        note = Some(format!("Found in an unexpected place: {}", shown(&hit)));
                        found = Some(hit);
                        break;
                    }
                }
            }
            match found {
                Some(path) => {
                    let mut fr = FileResult {
                        shown_path: shown(&path),
                        found: true,
                        format: sf.format.clone(),
                        read_ok: false,
                        settings_count: 0,
                        error: None,
                        searched: searched.clone(),
                        note: note.clone(),
                        modified: std::fs::metadata(&path)
                            .and_then(|m| m.modified())
                            .ok()
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_secs() as i64),
                        real_path: Some(path.clone()),
                        inline_text: None,
                    };
                    match std::fs::read(&path) {
                        Ok(bytes) => {
                            let text = String::from_utf8_lossy(&bytes);
                            match parse(&sf.format, &text) {
                                Ok(m) => {
                                    fr.read_ok = true;
                                    fr.settings_count = m.len();
                                    settings.extend(m);
                                }
                                Err(e) => fr.error = Some(e),
                            }
                        }
                        Err(e) => fr.error = Some(format!("could not open: {e}")),
                    }
                    crate::logger::info_changed(
                        &format!("file:{}:{}", p.id, fr.shown_path),
                        format!(
                            "{}: settings file {} ({} settings{})",
                            p.name,
                            fr.shown_path,
                            fr.settings_count,
                            fr.error
                                .as_ref()
                                .map(|e| format!(", {e}"))
                                .unwrap_or_default()
                        ),
                    );
                    files.push(fr);
                }
                None => {
                    let first = candidates
                        .first()
                        .map(|c| shown(c))
                        .unwrap_or_else(|| "(no location for this system)".into());
                    files.push(FileResult {
                        shown_path: first,
                        found: false,
                        format: sf.format.clone(),
                        read_ok: false,
                        settings_count: 0,
                        error: None,
                        searched: searched.clone(),
                        note: sf.legacy.then(|| "An older place the game no longer uses, so not finding it is normal.".to_string()),
                        modified: None,
                        real_path: None,
                        inline_text: None,
                    });
                }
            }
        }
        let (findings, unmatched) = if settings.is_empty() {
            (Vec::new(), Vec::new())
        } else {
            evaluate(&p.findings, &settings)
        };
        let unreadable = if settings.is_empty() {
            Vec::new()
        } else {
            unreadable_of(&p, &settings, &unmatched)
        };
        // Setting names only help when something couldn't be read.
        let key_names: Vec<String> = if unreadable.is_empty() {
            Vec::new()
        } else {
            settings.keys().take(400).cloned().collect()
        };
        let mut current = current_settings(&p.findings, &settings, &findings);
        // Effects that cost stream bitrate (film grain, motion blur...), found
        // by the automatic reader when the profile doesn't list them.
        let covered: Vec<String> = current
            .iter()
            .map(|c| c.setting.clone())
            .chain(p.tunables.iter().map(|t| t.setting.clone()))
            .collect();
        let extra = crate::autoread::bitrate_effects(&settings, &p.name, &covered);
        let extra_used = extra.len();
        current.extend(extra);
        let preset = preset_state(&p, &settings);
        let tunables = tunable_states(&p, &settings);
        let fps_cap = if p.fps_limit {
            fps_cap_of(&p.findings, &settings)
        } else {
            None
        };
        let proton = files.iter().any(|f| {
            f.found
                && f.real_path
                    .as_ref()
                    .map(|r| r.to_string_lossy().contains("compatdata"))
                    .unwrap_or(false)
        });
        let (display_res, display_hz) = display_of(&p, &settings);
        let pats = profile_patterns(&p);
        let settings_used = settings
            .keys()
            .filter(|k| pats.iter().any(|pt| key_matches(k, pt)))
            .count()
            + extra_used;
        if installed || running || files.iter().any(|f| f.found) {
            crate::logger::info_changed(
                &format!("game:{}", p.id),
                format!(
                    "{}: installed={} running={} findings={} unmatched rules={:?} unreadable={:?}",
                    p.name,
                    installed,
                    running,
                    findings.len(),
                    unmatched,
                    unreadable
                ),
            );
        }
        out.push(GameResult {
            id: p.id.clone(),
            name: p.name.clone(),
            load: p.load.clone(),
            profile_version: p.profile_version,
            verified: p.verified.clone(),
            installed,
            install_hint: hint,
            running,
            platform_note,
            files,
            findings,
            unmatched_rules: unmatched,
            key_names,
            current,
            preset,
            tunables,
            fps_cap,
            unreadable,
            has_fps_limit: p.fps_limit,
            api: p.api.clone(),
            proton,
            display_res,
            display_hz,
            auto: None,
            processes: p.processes.iter().map(|x| x.to_lowercase()).collect(),
            settings_read: settings.len(),
            settings_used,
            read_gap: None,
            launcher: if p.steam_appid.is_some() && installed {
                "Steam".into()
            } else {
                String::new()
            },
            motion: p.motion.clone(),
        });
    }
    // Games without a profile.
    let profs = profiles();
    let known_exes: Vec<String> = profs.iter().flat_map(|p| p.processes.clone()).collect();
    let known_ids: Vec<u32> = profs.iter().filter_map(|p| p.steam_appid).collect();
    out.extend(crate::autoread::scan(
        running,
        hints,
        &known_exes,
        &known_ids,
        &roots,
        &libs,
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_profiles_load() {
        assert_eq!(profiles().len(), PROFILE_FILES.len());
    }

    #[test]
    fn cyberpunk_style_json() {
        let j = r#"{"version":3,"data":[{"group_name":"/graphics/raytracing","options":[{"name":"RayTracing","type":"Bool","value":true},{"name":"RayTracedPathTracing","value":false}]},{"group_name":"/graphics/dlss","options":[{"name":"DLSS","value":"Off"}]}]}"#;
        let m = parse("json", j).unwrap();
        assert_eq!(
            m.get("graphics/raytracing/RayTracing").map(|s| s.as_str()),
            Some("true")
        );
        let p = profiles()
            .into_iter()
            .find(|p| p.id == "cyberpunk2077")
            .unwrap();
        let (f, _) = evaluate(&p.findings, &m);
        assert!(f.iter().any(|x| x.rule_id == "raytracing"));
        assert!(!f.iter().any(|x| x.rule_id == "pathtracing"));
    }

    #[test]
    fn fortnite_ini() {
        let t = "[ScalabilityGroups]\nsg.ShadowQuality=3\n[/Script/FortniteGame.FortGameUserSettings]\nFrameRateLimit=0.000000\nbUseVSync=False\n";
        let m = parse("ini", t).unwrap();
        let p = profiles().into_iter().find(|p| p.id == "fortnite").unwrap();
        let (f, _) = evaluate(&p.findings, &m);
        let ids: Vec<_> = f.iter().map(|x| x.rule_id.as_str()).collect();
        assert!(ids.contains(&"shadows"));
        assert!(ids.contains(&"fps_uncapped"));
        assert!(!ids.contains(&"vsync"));
    }

    #[test]
    fn kv_and_lines() {
        let m = parse(
            "kv",
            "\"video.cfg\"\n{\n\t\"setting.mat_vsync\"\t\t\"1\"\n}\n",
        )
        .unwrap();
        assert_eq!(m.get("setting.mat_vsync").map(|s| s.as_str()), Some("1"));
        let m2 = parse(
            "lines",
            "Graphics Settings\n{\n  \"vSync\": true,\n  depthOfField = false\n}\n",
        )
        .unwrap();
        assert_eq!(
            m2.get("Graphics Settings/vSync").map(|s| s.as_str()),
            Some("true")
        );
    }

    #[test]
    fn halo_value_version_pairs() {
        let j = r#"{"spec_control_vsync":{"value":0,"version":1},"spec_control_target_framerate":{"value":0,"version":2}}"#;
        let m = parse("json", j).unwrap();
        assert_eq!(m.get("spec_control_vsync").map(|s| s.as_str()), Some("0"));
        assert!(!m.contains_key("spec_control_vsync/version"));
        let p = profiles()
            .into_iter()
            .find(|p| p.id == "halo_infinite")
            .unwrap();
        let (f, _) = evaluate(&p.findings, &m);
        let ids: Vec<_> = f.iter().map(|x| x.rule_id.as_str()).collect();
        assert!(!ids.contains(&"vsync"));
        assert!(ids.contains(&"fps_uncapped"));
    }

    #[test]
    fn cyberpunk_real_keys_no_duplicates() {
        let j = r#"{"version":3,"data":[
          {"group_name":"/graphics/raytracing","options":[
            {"name":"RayTracing","value":true},{"name":"RayTracedPathTracing","value":true},
            {"name":"RayTracedPathTracingForPhotoMode","value":true}]},
          {"group_name":"/graphics/presets","options":[
            {"name":"DLSS","value":"Quality"},{"name":"DLSS_NewSharpness","value":0.5},
            {"name":"FSR2","value":"Off"},{"name":"FSR3","value":"Off"},{"name":"FSR4","value":"Off"},{"name":"XESS","value":"Off"}]}]}"#;
        let m = parse("json", j).unwrap();
        let p = profiles()
            .into_iter()
            .find(|p| p.id == "cyberpunk2077")
            .unwrap();
        let (f, _) = evaluate(&p.findings, &m);
        let keys: Vec<_> = f.iter().map(|x| x.key.as_str()).collect();
        let mut dedup = keys.clone();
        dedup.dedup();
        assert_eq!(keys.len(), dedup.len());
        assert!(
            f.iter()
                .any(|x| x.rule_id == "pathtracing" && x.key.ends_with("RayTracedPathTracing"))
        );
        assert!(
            f.iter()
                .any(|x| x.rule_id == "raytracing" && x.key.ends_with("/RayTracing"))
        );
        assert!(!f.iter().any(|x| x.rule_id == "upscaler_off"));
        // Path tracing only in photo mode must not count.
        let m2 = parse("json", r#"{"data":[{"group_name":"/graphics/raytracing","options":[{"name":"RayTracedPathTracing","value":false},{"name":"RayTracedPathTracingForPhotoMode","value":true}]}]}"#).unwrap();
        let (f2, _) = evaluate(&p.findings, &m2);
        assert!(!f2.iter().any(|x| x.rule_id == "pathtracing"));
    }

    #[test]
    fn linux_truncated_process_names() {
        assert!(process_matches("Cyberpunk2077.e", "cyberpunk2077.exe"));
        assert!(!process_matches("cyber", "cyberpunk2077.exe"));
    }
}

#[cfg(test)]
mod current_tests {
    use super::*;

    #[test]
    fn cyberpunk_fps_limit_and_values() {
        let p = profiles()
            .into_iter()
            .find(|p| p.id == "cyberpunk2077")
            .unwrap();
        let mut s = BTreeMap::new();
        s.insert(
            "video/display/MaximumFPS_OnOff".to_string(),
            "true".to_string(),
        );
        s.insert(
            "video/display/MaximumFPS_Value".to_string(),
            "240".to_string(),
        );
        s.insert("video/display/VSync".to_string(), "false".to_string());
        s.insert(
            "graphics/raytracing/RayTracedPathTracing".to_string(),
            "true".to_string(),
        );
        s.insert("graphics/presets/DLSS".to_string(), "Quality".to_string());
        let (f, unmatched) = evaluate(&p.findings, &s);
        assert!(
            !unmatched.contains(&"fps_uncapped".to_string()),
            "FPS limit key must match now"
        );
        assert!(
            !f.iter().any(|x| x.rule_id == "fps_uncapped"),
            "capped at 240 is not uncapped"
        );
        let cur = current_settings(&p.findings, &s, &f);
        let fps = cur.iter().find(|c| c.rule_id == "fps_uncapped").unwrap();
        assert_eq!(fps.setting, "FPS limit");
        assert_eq!(fps.value, "240");
        let pt = cur.iter().find(|c| c.rule_id == "pathtracing").unwrap();
        assert_eq!(pt.value, "On");
        assert!(pt.flagged);
        let up = cur.iter().find(|c| c.rule_id == "upscaler_off").unwrap();
        assert!(up.value.contains("Quality"), "{}", up.value);
        // Uncapped
        s.insert(
            "video/display/MaximumFPS_OnOff".to_string(),
            "false".to_string(),
        );
        let (f, _) = evaluate(&p.findings, &s);
        assert!(f.iter().any(|x| x.rule_id == "fps_uncapped"));
        let cur = current_settings(&p.findings, &s, &f);
        assert_eq!(
            cur.iter()
                .find(|c| c.rule_id == "fps_uncapped")
                .unwrap()
                .value,
            "Off"
        );
    }
}

#[cfg(test)]
mod real_file_tests {
    use super::*;

    /// A real Cyberpunk 2077 settings file from a test PC (Nov 2026).
    #[test]
    fn cyberpunk_real_user_file() {
        let text = include_str!("testdata/cyberpunk_usersettings.json");
        let m = parse("json", text).unwrap();
        let p = profiles()
            .into_iter()
            .find(|p| p.id == "cyberpunk2077")
            .unwrap();
        let (f, unmatched) = evaluate(&p.findings, &m);
        assert!(
            unmatched.is_empty(),
            "every rule should find its setting: {unmatched:?}"
        );
        assert!(unreadable_of(&p, &m, &unmatched).is_empty());
        // Ray tracing is off; DLSS is on; capped at 240.
        assert!(!f.iter().any(|x| x.effect == "rt_on"));
        assert!(!f.iter().any(|x| x.rule_id == "upscaler_off"));
        assert!(!f.iter().any(|x| x.rule_id == "fps_uncapped"));
        assert_eq!(fps_cap_of(&p.findings, &m), Some(240));
        let cur = current_settings(&p.findings, &m, &f);
        assert_eq!(
            cur.iter().find(|c| c.rule_id == "vsync").unwrap().value,
            "Off"
        );
        let ps = preset_state(&p, &m).unwrap();
        assert!(ps.custom);
        assert_eq!(ps.value, "Custom");
        let t = tunable_states(&p, &m);
        let get = |id: &str| t.iter().find(|x| x.id == id).unwrap();
        assert_eq!(get("ssr").now, "Ultra");
        assert_eq!(get("ssr").change_to.as_deref(), Some("High"));
        assert_eq!(get("fog").change_to.as_deref(), Some("High"));
        assert_eq!(get("clouds").change_to.as_deref(), Some("High"));
        assert_eq!(get("cascaded").change_to.as_deref(), Some("Medium"));
        assert_eq!(pretty_value("BorderlessWindowed"), "Borderless windowed");
        assert_eq!(pretty_value("DLSS"), "DLSS");
        assert_eq!(pretty_value("UI-Settings-Video-QualitySetting-Off"), "Off");
    }

    /// Real Counter-Strike 2 files from a test PC (Nov 2026).
    #[test]
    fn cs2_real_files() {
        let mut m = parse("kv", include_str!("testdata/cs2_video.txt")).unwrap();
        m.extend(parse("kv", include_str!("testdata/cs2_user_convars.vcfg")).unwrap());
        m.extend(parse("kv", include_str!("testdata/cs2_machine_convars.vcfg")).unwrap());
        let p = profiles().into_iter().find(|p| p.id == "cs2").unwrap();
        let (f, unmatched) = evaluate(&p.findings, &m);
        assert!(
            unreadable_of(&p, &m, &unmatched).is_empty(),
            "{:?}",
            unreadable_of(&p, &m, &unmatched)
        );
        assert_eq!(fps_cap_of(&p.findings, &m), Some(400));
        assert!(
            !f.iter().any(|x| x.effect == "uncapped_fps"
                || x.effect == "vsync_on"
                || x.effect == "heavy")
        );
        let cur = current_settings(&p.findings, &m, &f);
        assert!(cur.iter().any(|c| c.rule_id == "cmaa"), "CMAA2 shown");
        let rx = cur.iter().find(|c| c.rule_id == "reflex").unwrap();
        assert_eq!((rx.value.as_str(), rx.flagged), ("On", false));
        let t = tunable_states(&p, &m);
        let get = |id: &str| t.iter().find(|x| x.id == id).unwrap();
        assert_eq!(get("shadows").now, "Low");
        assert_eq!(get("ao").now, "Disabled");
        assert_eq!(get("particles").now, "Low");
        assert_eq!(get("shader").now, "Low");
        assert_eq!(get("textures").now, "Low");
        assert!(
            t.iter().all(|x| x.change_to.is_none()),
            "everything is already low"
        );
        assert_eq!(display_of(&p, &m).0.as_deref(), Some("2560x1440"));
        // Very High particles and High AO would be lowered.
        m.insert("setting.videocfg_particle_detail".into(), "3".into());
        m.insert("setting.videocfg_ao_detail".into(), "2".into());
        let t = tunable_states(&p, &m);
        assert_eq!(
            t.iter()
                .find(|x| x.id == "particles")
                .unwrap()
                .change_to
                .as_deref(),
            Some("Medium")
        );
        assert_eq!(
            t.iter()
                .find(|x| x.id == "ao")
                .unwrap()
                .change_to
                .as_deref(),
            Some("Disabled")
        );
    }

    #[test]
    fn preset_steps_down_and_main_switch() {
        let p = profiles()
            .into_iter()
            .find(|p| p.id == "cyberpunk2077")
            .unwrap();
        let mut m = BTreeMap::new();
        m.insert(
            "graphics/presets/QuickPresets".to_string(),
            "Ultra".to_string(),
        );
        let ps = preset_state(&p, &m).unwrap();
        assert!(!ps.custom);
        assert_eq!(ps.lower.as_deref(), Some("High"));
        // A rule that needs a main switch is ignored while the switch is off.
        let rule = FindingRule {
            id: "x".into(),
            keys: vec!["sub$".into()],
            when: "truthy".into(),
            effect: "heavy".into(),
            label: "Sub is on".into(),
            menu: String::new(),
            change_to: None,
            gain: None,
            cost: None,
            setting: None,
            value_keys: vec![],
            requires: Some("main$".into()),
        };
        let mut s = BTreeMap::new();
        s.insert("a/main".to_string(), "false".to_string());
        s.insert("a/sub".to_string(), "true".to_string());
        assert!(evaluate(std::slice::from_ref(&rule), &s).0.is_empty());
        s.insert("a/main".to_string(), "true".to_string());
        assert_eq!(evaluate(std::slice::from_ref(&rule), &s).0.len(), 1);
    }
}

#[cfg(test)]
mod new_profile_tests {
    use super::*;

    #[test]
    fn xml_and_wildcards() {
        let m = parse_xml(
            "<?xml version=\"1.0\"?>\n<Settings><graphics><Grass_Quality value=\"3\" /><VSync value=\"0\"/></graphics><config><RayTracingQuality>2</RayTracingQuality></config></Settings>",
        );
        assert_eq!(
            m.get("Settings/graphics/Grass_Quality").map(|s| s.as_str()),
            Some("3")
        );
        assert_eq!(
            m.get("Settings/config/RayTracingQuality")
                .map(|s| s.as_str()),
            Some("2")
        );
        assert!(wild_match("options.4.cod24.cst", "options*.cst"));
        assert!(!wild_match("config.ini", "options*.cst"));
        let gta = profiles().into_iter().find(|p| p.id == "gta5").unwrap();
        let t = tunable_states(&gta, &m);
        let grass = t.iter().find(|x| x.id == "grass").unwrap();
        assert_eq!(
            (grass.now.as_str(), grass.change_to.as_deref()),
            ("Ultra", Some("Very High"))
        );
        let er = profiles()
            .into_iter()
            .find(|p| p.id == "elden_ring")
            .unwrap();
        assert!(
            evaluate(&er.findings, &m)
                .0
                .iter()
                .any(|f| f.rule_id == "raytracing")
        );
    }

    #[test]
    fn minecraft_options() {
        let m = parse(
            "lines",
            "version:3953\nmaxFps:260\nenableVsync:false\nrenderDistance:32\ngraphicsMode:2\n",
        )
        .unwrap();
        let p = profiles()
            .into_iter()
            .find(|p| p.id == "minecraft")
            .unwrap();
        let (f, unmatched) = evaluate(&p.findings, &m);
        assert!(unmatched.is_empty());
        let ids: Vec<_> = f.iter().map(|x| x.rule_id.as_str()).collect();
        assert!(
            ids.contains(&"fps_uncapped")
                && ids.contains(&"render_distance")
                && ids.contains(&"fabulous")
                && !ids.contains(&"vsync")
        );
    }

    #[test]
    fn new_profiles_are_marked_unverified() {
        for id in [
            "valorant",
            "apex",
            "warzone",
            "league",
            "minecraft",
            "marvel_rivals",
            "elden_ring",
            "gta5",
        ] {
            let p = profiles()
                .into_iter()
                .find(|p| p.id == id)
                .unwrap_or_else(|| panic!("{id} missing"));
            assert!(p.verified.starts_with("Unverified"), "{id}");
        }
    }
}

#[cfg(test)]
mod cities_tests {
    use super::*;

    /// The real Cities: Skylines II file from a test PC (Nov 2026).
    #[test]
    fn cities_real_settings_coc() {
        let m = parse("coc", include_str!("testdata/cities2_settings.coc")).unwrap();
        assert_eq!(
            m.get("Graphics Settings/DepthOfFieldQualitySettings/enabled")
                .map(|s| s.as_str()),
            Some("true")
        );
        assert_eq!(
            m.get("Graphics Settings/ShadowsQualitySettings/directionalShadowResolution")
                .map(|s| s.as_str()),
            Some("4096")
        );
        assert_eq!(
            m.get("General Settings/autoSave").map(|s| s.as_str()),
            Some("true")
        );
        let p = profiles()
            .into_iter()
            .find(|p| p.id == "cities_skylines_2")
            .unwrap();
        assert!(!p.fps_limit);
        let (f, unmatched) = evaluate(&p.findings, &m);
        assert!(
            unreadable_of(&p, &m, &unmatched).is_empty(),
            "missing = default in this file"
        );
        assert!(
            !f.iter().any(|x| x.rule_id == "gi"),
            "global illumination is already off"
        );
        let t = tunable_states(&p, &m);
        let get = |id: &str| t.iter().find(|x| x.id == id).unwrap();
        assert_eq!(
            (get("dof").now.as_str(), get("dof").change_to.as_deref()),
            ("On", Some("Off"))
        );
        assert_eq!(get("motionblur").change_to.as_deref(), Some("Off"));
        assert_eq!(get("clouds").change_to.as_deref(), Some("Off"));
        assert_eq!(
            (
                get("shadows").now.as_str(),
                get("shadows").change_to.as_deref()
            ),
            ("4096", Some("2048"))
        );
        assert_eq!(
            (
                get("outlines").now.as_str(),
                get("outlines").change_to.as_deref()
            ),
            ("8x", Some("4x"))
        );
        assert_eq!(display_of(&p, &m), (None, Some(60.0)));
    }
}
