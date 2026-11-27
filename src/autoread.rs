//! Reads the settings of games Preflight has no hand-made profile for.
//!
//! It works out where the game is installed (Steam, Heroic, Lutris, Bottles,
//! Epic, Game Pass or just the running program's folder), which engine it
//! uses (Unity, Unreal, Source, Godot), and then reads that engine's usual
//! settings places, plus any settings-looking files in folders named after
//! the game. Everything is read-only. Save games, logs and anything with
//! account details are skipped, and nothing in the game's memory is touched.

use crate::games::{CurrentSetting, FileResult, Finding, GameResult, PresetState, TunableState};
use crate::hardware::GameProc;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Where the game is

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Located {
    pub name: String,
    pub exe: String,
    pub appid: Option<u32>,
    pub install: Option<PathBuf>,
    /// Wine/Proton prefix: the folder that holds drive_c.
    pub prefix: Option<PathBuf>,
    /// "Steam", "Heroic", "Lutris", "Bottles", "Epic", "Game Pass" or "".
    pub launcher: String,
}

#[derive(Clone, Debug)]
struct Manifest {
    appid: u32,
    name: String,
    installdir: String,
    lib: PathBuf,
}

fn steam_manifests(libs: &[PathBuf]) -> Vec<Manifest> {
    let mut out = Vec::new();
    for lib in libs {
        let Ok(rd) = std::fs::read_dir(lib.join("steamapps")) else {
            continue;
        };
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if !(n.starts_with("appmanifest_") && n.ends_with(".acf")) {
                continue;
            }
            let Ok(t) = std::fs::read_to_string(e.path()) else {
                continue;
            };
            let kv = crate::games::parse("kv", &t).unwrap_or_default();
            let (Some(id), Some(name), Some(dir)) = (
                kv.get("appid").and_then(|v| v.parse().ok()),
                kv.get("name"),
                kv.get("installdir"),
            ) else {
                continue;
            };
            out.push(Manifest {
                appid: id,
                name: name.clone(),
                installdir: dir.clone(),
                lib: lib.clone(),
            });
        }
    }
    out
}

/// Lower-case letters and digits only: "Moon Sweepers" -> "moonsweepers".
pub fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// The program's name without its file ending and Unreal's build tags.
fn exe_stem(exe: &str) -> String {
    let s = exe.to_lowercase();
    let s = s
        .trim_end_matches(".exe")
        .trim_end_matches(".x86_64")
        .trim_end_matches(".x86");
    let s = s
        .split("-win64")
        .next()
        .unwrap_or(s)
        .split("-linux")
        .next()
        .unwrap_or(s);
    s.to_string()
}

fn pretty_name(s: &str) -> String {
    let s = s.replace(['_', '-'], " ");
    let mut out = String::new();
    let mut prev_lower = false;
    for ch in s.chars() {
        if ch.is_uppercase() && prev_lower {
            out.push(' ');
        }
        prev_lower = ch.is_lowercase();
        out.push(ch);
    }
    let mut c = out.trim().chars();
    c.next()
        .map(|f| format!("{}{}", f.to_uppercase(), c.as_str()))
        .unwrap_or_default()
}

fn steam_by_installdir(path: &Path, manifests: &[Manifest]) -> Option<(Manifest, PathBuf)> {
    let s = path.to_string_lossy().replace('\\', "/");
    let low = s.to_lowercase();
    let i = low.find("/steamapps/common/")?;
    let rest = &s[i + "/steamapps/common/".len()..];
    let dir = rest.split('/').next()?;
    let m = manifests
        .iter()
        .find(|m| m.installdir.eq_ignore_ascii_case(dir))?
        .clone();
    let install = PathBuf::from(&s[..i]).join("steamapps/common").join(dir);
    Some((m, install))
}

fn proton_prefix(m: &Manifest, libs: &[PathBuf]) -> Option<PathBuf> {
    std::iter::once(&m.lib)
        .chain(libs.iter())
        .map(|l| l.join(format!("steamapps/compatdata/{}/pfx", m.appid)))
        .find(|p| p.join("drive_c").is_dir())
}

/// Folders that hold many games: the game's own folder is the one below it.
fn install_from_exe(exe_path: &Path) -> Option<PathBuf> {
    const CONTAINERS: [&str; 12] = [
        "program files",
        "program files (x86)",
        "games",
        "gog games",
        "epic games",
        "drive_c",
        "xboxgames",
        "ea games",
        "heroic",
        "lutris",
        "common",
        "windowsapps",
    ];
    let mut cur = exe_path.parent()?.to_path_buf();
    for _ in 0..5 {
        let parent = cur.parent()?;
        let pname = parent
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if CONTAINERS.contains(&pname.as_str()) {
            return Some(cur);
        }
        // Unreal: <Game>/<Project>/Binaries/Win64/x.exe
        let cname = cur
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if cname == "binaries" {
            return parent.parent().map(|p| p.to_path_buf());
        }
        cur = parent.to_path_buf();
    }
    exe_path.parent().map(|p| p.to_path_buf())
}

/// Heroic (Epic and GOG) and Legendary list installed games with titles.
fn heroic_title(install: &Path) -> Option<(String, Option<PathBuf>)> {
    let home = crate::util::home_dir()?;
    let inst = norm(&install.to_string_lossy());
    let mut configs = vec![
        home.join(".config/legendary/installed.json"),
        home.join(".config/heroic/legendaryConfig/legendary/installed.json"),
        home.join(".config/heroic/gog_store/installed.json"),
    ];
    configs.push(home.join(".var/app/com.heroicgameslauncher.hgl/config/heroic/legendaryConfig/legendary/installed.json"));
    configs.push(
        home.join(".var/app/com.heroicgameslauncher.hgl/config/heroic/gog_store/installed.json"),
    );
    for c in configs {
        let Ok(t) = std::fs::read_to_string(&c) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&t) else {
            continue;
        };
        let entries: Vec<&serde_json::Value> = match &v {
            serde_json::Value::Object(o) if o.contains_key("installed") => o["installed"]
                .as_array()
                .map(|a| a.iter().collect())
                .unwrap_or_default(),
            serde_json::Value::Object(o) => o.values().collect(),
            _ => Vec::new(),
        };
        for e in entries {
            let p = e.get("install_path").and_then(|x| x.as_str()).unwrap_or("");
            if !p.is_empty() && norm(p) == inst {
                let title = e
                    .get("title")
                    .and_then(|x| x.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| {
                        install
                            .file_name()
                            .map(|n| pretty_name(&n.to_string_lossy()))
                            .unwrap_or_default()
                    });
                let app = e
                    .get("app_name")
                    .or_else(|| e.get("appName"))
                    .and_then(|x| x.as_str())
                    .unwrap_or("");
                let prefix = heroic_prefix(&home, app);
                return Some((title, prefix));
            }
        }
    }
    None
}

fn heroic_prefix(home: &Path, app: &str) -> Option<PathBuf> {
    if app.is_empty() {
        return None;
    }
    for base in [
        home.join(".config/heroic/GamesConfig"),
        home.join(".var/app/com.heroicgameslauncher.hgl/config/heroic/GamesConfig"),
    ] {
        let Ok(t) = std::fs::read_to_string(base.join(format!("{app}.json"))) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&t) else {
            continue;
        };
        if let Some(p) = v
            .get(app)
            .and_then(|g| g.get("winePrefix"))
            .and_then(|p| p.as_str())
        {
            let p = PathBuf::from(p);
            // Proton prefixes put drive_c inside "pfx".
            return Some(if p.join("pfx/drive_c").is_dir() {
                p.join("pfx")
            } else {
                p
            });
        }
    }
    None
}

/// Lutris game files: "exe:" and "prefix:" lines.
fn lutris_lookup(
    exe_path: Option<&Path>,
    stem: &str,
) -> Option<(String, Option<PathBuf>, Option<PathBuf>)> {
    let home = crate::util::home_dir()?;
    for d in [
        home.join(".config/lutris/games"),
        home.join(".local/share/lutris/games"),
        home.join(".var/app/net.lutris.Lutris/data/lutris/games"),
    ] {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let Ok(t) = std::fs::read_to_string(e.path()) else {
                continue;
            };
            let field = |k: &str| {
                t.lines().find_map(|l| {
                    l.trim()
                        .strip_prefix(k)
                        .map(|v| v.trim().trim_matches(['"', '\'']).to_string())
                })
            };
            let exe = field("exe:").unwrap_or_default();
            let hit = match exe_path {
                Some(p) => {
                    !exe.is_empty()
                        && norm(&exe).ends_with(&norm(
                            &p.file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_default(),
                        ))
                        && (norm(&p.to_string_lossy()).contains(&norm(&exe)) || exe.contains(stem))
                }
                None => exe_stem(exe.rsplit(['/', '\\']).next().unwrap_or("")) == stem,
            };
            if hit {
                let stem = e
                    .path()
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                let name = t
                    .lines()
                    .find_map(|l| {
                        l.strip_prefix("name:")
                            .map(|v| v.trim().trim_matches(['"', '\'']).to_string())
                    })
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| {
                        pretty_name(stem.rsplit_once('-').map(|x| x.0).unwrap_or(&stem))
                    });
                let prefix = field("prefix:").map(PathBuf::from);
                let install = Some(PathBuf::from(&exe))
                    .filter(|p| p.is_absolute())
                    .and_then(|p| install_from_exe(&p));
                return Some((name, prefix, install));
            }
        }
    }
    None
}

/// Bottles keep each "bottle" (a Wine prefix) in one folder.
fn bottles_prefix(exe_path: &Path) -> Option<PathBuf> {
    let s = exe_path.to_string_lossy().replace('\\', "/");
    let i = s.find("/bottles/")?;
    let rest = &s[i + "/bottles/".len()..];
    let name = rest.split('/').next()?;
    let p = PathBuf::from(&s[..i]).join("bottles").join(name);
    p.join("drive_c").is_dir().then_some(p)
}

#[cfg(windows)]
fn epic_title(install: &Path) -> Option<String> {
    let dir = PathBuf::from(r"C:\ProgramData\Epic\EpicGamesLauncher\Data\Manifests");
    let inst = norm(&install.to_string_lossy());
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let Ok(t) = std::fs::read_to_string(e.path()) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&t) else {
            continue;
        };
        if v.get("InstallLocation")
            .and_then(|x| x.as_str())
            .map(|p| norm(p) == inst)
            .unwrap_or(false)
        {
            return v
                .get("DisplayName")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string());
        }
    }
    None
}

fn cache_path() -> PathBuf {
    crate::util::dirs().data.join("known_games.json")
}

fn load_cache() -> BTreeMap<String, Located> {
    std::fs::read_to_string(cache_path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save_cache(c: &BTreeMap<String, Located>) {
    if let Ok(t) = serde_json::to_string_pretty(c) {
        let _ = std::fs::write(cache_path(), t);
    }
}

/// Works out where a running game lives.
fn locate_running(p: &GameProc, manifests: &[Manifest], libs: &[PathBuf]) -> Option<Located> {
    let path = p.path.clone()?;
    let stem = exe_stem(&p.exe);
    let mut loc = Located {
        exe: p.exe.clone(),
        ..Default::default()
    };
    if let Some((m, install)) = steam_by_installdir(&path, manifests) {
        loc.name = m.name.clone();
        loc.appid = Some(m.appid);
        loc.install = Some(install);
        loc.prefix = p
            .compat
            .as_ref()
            .map(|c| c.join("pfx"))
            .filter(|x| x.join("drive_c").is_dir())
            .or_else(|| proton_prefix(&m, libs));
        loc.launcher = "Steam".into();
        return Some(loc);
    }
    if let Some(id) = p.steam_appid
        && let Some(m) = manifests.iter().find(|m| m.appid == id)
    {
        loc.name = m.name.clone();
        loc.appid = Some(id);
        loc.install = Some(m.lib.join("steamapps/common").join(&m.installdir));
        loc.prefix = proton_prefix(m, libs);
        loc.launcher = "Steam".into();
        return Some(loc);
    }
    loc.install = install_from_exe(&path);
    loc.prefix = p
        .wineprefix
        .clone()
        .or_else(|| p.compat.as_ref().map(|c| c.join("pfx")));
    if let Some((title, prefix)) = loc.install.as_deref().and_then(heroic_title) {
        loc.name = title;
        loc.prefix = loc.prefix.or(prefix);
        loc.launcher = "Heroic".into();
    } else if let Some((name, prefix, _)) = lutris_lookup(Some(&path), &stem) {
        loc.name = name;
        loc.prefix = loc.prefix.or(prefix);
        loc.launcher = "Lutris".into();
    } else if let Some(b) = bottles_prefix(&path) {
        loc.prefix = Some(b);
        loc.launcher = "Bottles".into();
    }
    #[cfg(windows)]
    if loc.name.is_empty() {
        if let Some(t) = loc.install.as_deref().and_then(epic_title) {
            loc.name = t;
            loc.launcher = "Epic".into();
        } else if path.to_string_lossy().to_lowercase().contains("xboxgames")
            || path
                .to_string_lossy()
                .to_lowercase()
                .contains("windowsapps")
        {
            loc.launcher = "Game Pass".into();
        }
    }
    if loc.name.is_empty() {
        loc.name = loc
            .install
            .as_ref()
            .and_then(|i| i.file_name())
            .map(|n| pretty_name(&n.to_string_lossy()))
            .unwrap_or_else(|| pretty_name(&stem));
    }
    Some(loc)
}

/// A game seen before (only its program name is known now, e.g. from the
/// last test): the saved location, or a Steam game with a matching name.
fn locate_hint(
    exe: &str,
    cache: &BTreeMap<String, Located>,
    manifests: &[Manifest],
    libs: &[PathBuf],
) -> Option<Located> {
    if let Some(l) = cache.get(exe) {
        return Some(l.clone());
    }
    let stem = norm(&exe_stem(exe));
    if stem.len() < 4 {
        return None;
    }
    let m = manifests
        .iter()
        .find(|m| norm(&m.installdir) == stem || norm(&m.name) == stem)?;
    let install = m.lib.join("steamapps/common").join(&m.installdir);
    // Make sure the program really is in that folder.
    find_named(&install, exe, 4)?;
    Some(Located {
        name: m.name.clone(),
        exe: exe.to_string(),
        appid: Some(m.appid),
        install: Some(install),
        prefix: proton_prefix(m, libs),
        launcher: "Steam".into(),
    })
}

fn find_named(dir: &Path, name: &str, depth: u32) -> Option<PathBuf> {
    let mut count = 0u32;
    walk(
        dir,
        depth,
        &mut count,
        4000,
        &mut |p, is_dir| {
            !is_dir
                && p.file_name()
                    .map(|n| n.to_string_lossy().eq_ignore_ascii_case(name))
                    .unwrap_or(false)
        },
        &|_: &Path| true,
    )
    .into_iter()
    .next()
}

/// Depth-limited folder walk. `want` picks files (or folders), `enter`
/// decides which folders to go into. Links are not followed.
fn walk(
    dir: &Path,
    depth: u32,
    count: &mut u32,
    max: u32,
    want: &mut dyn FnMut(&Path, bool) -> bool,
    enter: &dyn Fn(&Path) -> bool,
) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if depth == 0 || *count > max {
        return out;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return out;
    };
    let mut subdirs = Vec::new();
    for e in rd.flatten() {
        *count += 1;
        if *count > max {
            break;
        }
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_symlink() {
            continue;
        }
        let p = e.path();
        if want(&p, ft.is_dir()) {
            out.push(p.clone());
        }
        if ft.is_dir() && enter(&p) {
            subdirs.push(p);
        }
    }
    for d in subdirs {
        out.extend(walk(&d, depth - 1, count, max, want, enter));
    }
    out
}

// ---------------------------------------------------------------------------
// Which engine

#[derive(Clone, Debug, PartialEq)]
pub enum Engine {
    Unity { company: String, product: String },
    Unreal { project: String },
    Source,
    Godot,
    Unknown,
}

impl Engine {
    pub fn label(&self) -> &'static str {
        match self {
            Engine::Unity { .. } => "Unity",
            Engine::Unreal { .. } => "Unreal",
            Engine::Source => "Source",
            Engine::Godot => "Godot",
            Engine::Unknown => "",
        }
    }
}

pub fn detect_engine(install: &Path, exe: &str) -> Engine {
    let Ok(rd) = std::fs::read_dir(install) else {
        return Engine::Unknown;
    };
    let entries: Vec<std::fs::DirEntry> = rd.flatten().collect();
    // Unity: "<Game>_Data/app.info" holds the company and product names,
    // which are also where Unity saves settings.
    for e in &entries {
        let n = e.file_name().to_string_lossy().to_string();
        if n.ends_with("_Data")
            && e.path().is_dir()
            && let Ok(t) = std::fs::read_to_string(e.path().join("app.info"))
        {
            let mut l = t.lines();
            let company = l.next().unwrap_or("").trim().to_string();
            let product = l.next().unwrap_or("").trim().to_string();
            if !product.is_empty() {
                return Engine::Unity { company, product };
            }
        }
    }
    let names: Vec<String> = entries
        .iter()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    let has = |n: &str| names.iter().any(|x| x.eq_ignore_ascii_case(n));
    if has("UnityPlayer.dll") || has("UnityPlayer.so") {
        return Engine::Unity {
            company: String::new(),
            product: String::new(),
        };
    }
    // Unreal: an "Engine" folder next to "<Project>/Binaries".
    if has("Engine") {
        for e in &entries {
            let n = e.file_name().to_string_lossy().to_string();
            if !n.eq_ignore_ascii_case("Engine") && e.path().join("Binaries").is_dir() {
                return Engine::Unreal { project: n };
            }
        }
        let stem = exe_stem(exe);
        if !stem.is_empty() {
            return Engine::Unreal { project: stem };
        }
    }
    if exe.to_lowercase().contains("-win64-shipping")
        || exe.to_lowercase().contains("-wingdk-shipping")
    {
        return Engine::Unreal {
            project: exe_stem(exe),
        };
    }
    // Source 2: game/bin; Source: <mod>/gameinfo.txt.
    if install.join("game/bin").is_dir()
        || entries.iter().any(|e| {
            e.path().join("gameinfo.txt").is_file() || e.path().join("gameinfo.gi").is_file()
        })
    {
        return Engine::Source;
    }
    if names.iter().any(|n| n.to_lowercase().ends_with(".pck")) {
        return Engine::Godot;
    }
    Engine::Unknown
}

// ---------------------------------------------------------------------------
// Where settings are kept

/// The user folders inside a Wine/Proton prefix (or on Windows itself).
fn user_roots(loc: &Located) -> Vec<PathBuf> {
    let mut homes: Vec<PathBuf> = Vec::new();
    if let Some(pfx) = &loc.prefix
        && let Ok(rd) = std::fs::read_dir(pfx.join("drive_c/users"))
    {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_lowercase();
            if n != "public" && e.path().is_dir() {
                homes.push(e.path());
            }
        }
    }
    #[cfg(windows)]
    if let Ok(p) = std::env::var("USERPROFILE") {
        homes.push(PathBuf::from(p));
    }
    let mut out = Vec::new();
    for h in homes {
        for sub in [
            "AppData/Local",
            "AppData/LocalLow",
            "AppData/Roaming",
            "Documents",
            "Documents/My Games",
            "My Documents",
            "My Documents/My Games",
            "Saved Games",
            "Local Settings/Application Data",
            "Application Data",
        ] {
            let p = h.join(sub);
            if p.is_dir() && !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out
}

/// Native Linux games keep settings in these.
#[allow(unused_mut)]
fn native_roots() -> Vec<PathBuf> {
    let mut v = Vec::new();
    #[cfg(not(windows))]
    if let Some(h) = crate::util::home_dir() {
        for s in [
            ".config",
            ".local/share",
            ".config/unity3d",
            ".config/Epic",
            ".local/share/godot/app_userdata",
        ] {
            let p = h.join(s);
            if p.is_dir() {
                v.push(p);
            }
        }
    }
    v
}

fn tokens(loc: &Located, engine: &Engine) -> Vec<String> {
    let mut t: Vec<String> = vec![norm(&loc.name), norm(&exe_stem(&loc.exe))];
    if let Some(i) = loc.install.as_ref().and_then(|i| i.file_name()) {
        t.push(norm(&i.to_string_lossy()));
    }
    match engine {
        Engine::Unity { product, .. } if !product.is_empty() => t.push(norm(product)),
        Engine::Unreal { project } => t.push(norm(project)),
        _ => {}
    }
    // Drop "game" and similar endings that make the names differ.
    let extra: Vec<String> = t
        .iter()
        .filter_map(|x| x.strip_suffix("game").map(|s| s.to_string()))
        .collect();
    t.extend(extra);
    t.retain(|x| x.len() >= 3);
    t.sort();
    t.dedup();
    t
}

fn name_matches(folder: &str, toks: &[String]) -> bool {
    let f = norm(folder);
    if f.len() < 3 {
        return false;
    }
    toks.iter().any(|t| {
        f == *t
            || (t.len() >= 5 && f.contains(t.as_str()))
            || (f.len() >= 6 && t.contains(f.as_str()))
    })
}

/// Folders named after the game, one or two levels down ("LocalLow/Company/Game").
fn matching_folders(roots: &[PathBuf], toks: &[String]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for r in roots {
        let Ok(rd) = std::fs::read_dir(r) else {
            continue;
        };
        for e in rd.flatten() {
            if !e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let n = e.file_name().to_string_lossy().to_string();
            if name_matches(&n, toks) {
                out.push(e.path());
                continue;
            }
            // One level deeper: company folders and "Packages/<pkg>/LocalCache/Local".
            if let Ok(rd2) = std::fs::read_dir(e.path()) {
                for e2 in rd2.flatten().take(200) {
                    if e2.file_type().map(|t| t.is_dir()).unwrap_or(false)
                        && name_matches(&e2.file_name().to_string_lossy(), toks)
                    {
                        out.push(e2.path());
                    }
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

const SKIP_DIRS: [&str; 30] = [
    "logs",
    "log",
    "crashes",
    "crashdumps",
    "crashreports",
    "cache",
    "caches",
    "shadercache",
    "shadercaches",
    "pipelinecache",
    "gpucache",
    "webcache",
    "cef",
    "screenshots",
    "saves",
    "savegames",
    "savegame",
    "save",
    "savedata",
    "demos",
    "replays",
    "bepinex",
    "mods",
    "plugins",
    "movies",
    "paks",
    "localization",
    "unity",
    "il2cpp_data",
    "mono",
];

const SKIP_NAME_PARTS: [&str; 26] = [
    "log",
    "crash",
    "achievement",
    "stats",
    "telemetry",
    "analytic",
    "account",
    "credential",
    "token",
    "auth",
    "cookie",
    "friend",
    "chat",
    "license",
    "eula",
    "readme",
    "changelog",
    "steam_api",
    "steam_emu",
    "manifest",
    "locali",
    "keybind",
    "input",
    "unins",
    "backup",
    "dump",
];

const EXTS: [&str; 15] = [
    "ini", "cfg", "json", "xml", "txt", "yaml", "yml", "toml", "conf", "vcfg", "coc", "settings",
    "prefs", "config", "dat",
];

fn settings_like(p: &Path) -> bool {
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    // Save games are never read ("profsave" is Frostbite's settings file).
    if name.contains("save") && !name.contains("profsave") {
        return false;
    }
    if SKIP_NAME_PARTS.iter().any(|s| name.contains(s)) {
        return false;
    }
    let ext = p
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if ext == "dat" {
        return name.contains("setting") || name.contains("config") || name.contains("option");
    }
    if ext.is_empty() {
        return ["setting", "config", "profile", "option", "prefs"]
            .iter()
            .any(|w| name.contains(w));
    }
    EXTS.contains(&ext.as_str())
}

fn enter_dir(p: &Path) -> bool {
    let n = p
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    !SKIP_DIRS.contains(&n.as_str()) && !n.starts_with('.')
}

// ---------------------------------------------------------------------------
// Unity: PlayerPrefs

/// A decoded registry or prefs value.
#[derive(Clone, Debug, PartialEq)]
pub enum PrefValue {
    Int(i64),
    Float(f64),
    Text(String),
}

impl PrefValue {
    fn as_setting(&self) -> String {
        match self {
            PrefValue::Int(i) => i.to_string(),
            PrefValue::Float(f) => {
                if f.fract() == 0.0 {
                    format!("{}", *f as i64)
                } else {
                    format!("{f:.3}")
                }
            }
            PrefValue::Text(s) => s.clone(),
        }
    }
}

/// Unity adds "_h12345" to every saved name.
fn unity_key(name: &str) -> String {
    match name.rfind("_h") {
        Some(i) if name[i + 2..].chars().all(|c| c.is_ascii_digit()) && i + 2 < name.len() => {
            name[..i].to_string()
        }
        _ => name.to_string(),
    }
}

fn decode_bytes(bytes: &[u8]) -> PrefValue {
    let trimmed: Vec<u8> = bytes.iter().copied().take_while(|b| *b != 0).collect();
    let printable = !trimmed.is_empty()
        && trimmed
            .iter()
            .all(|b| b.is_ascii_graphic() || *b == b' ' || *b >= 0x80);
    if printable && std::str::from_utf8(&trimmed).is_ok() {
        return PrefValue::Text(String::from_utf8_lossy(&trimmed).to_string());
    }
    if bytes.len() == 8 {
        let mut a = [0u8; 8];
        a.copy_from_slice(bytes);
        return PrefValue::Float(f64::from_le_bytes(a));
    }
    if bytes.len() == 4 {
        let mut a = [0u8; 4];
        a.copy_from_slice(bytes);
        return PrefValue::Float(f32::from_le_bytes(a) as f64);
    }
    PrefValue::Text(String::new())
}

/// Reads one section of a Wine registry file (user.reg). Only the lines of
/// that section are looked at.
pub fn read_wine_reg(text: &str, key: &str) -> Option<Vec<(String, PrefValue)>> {
    let want = format!("[{}]", key.replace('\\', "\\\\")).to_lowercase();
    let mut lines = text.lines();
    loop {
        let l = lines.next()?;
        if l.to_lowercase().starts_with(&want) {
            break;
        }
    }
    let mut out = Vec::new();
    let mut pending = String::new();
    for l in lines {
        if l.starts_with('[') {
            break;
        }
        if l.starts_with('#') || l.trim().is_empty() {
            continue;
        }
        // Long values continue on the next line after a backslash.
        pending.push_str(l.trim());
        if pending.ends_with('\\') {
            pending.pop();
            continue;
        }
        let line = std::mem::take(&mut pending);
        let Some(rest) = line.strip_prefix('"') else {
            continue;
        };
        let Some(end) = rest.find("\"=") else {
            continue;
        };
        let name = rest[..end].replace("\\\\", "\\");
        let val = &rest[end + 2..];
        let v = if let Some(h) = val.strip_prefix("dword:") {
            u32::from_str_radix(h, 16)
                .map(|n| PrefValue::Int(n as i32 as i64))
                .ok()
        } else if let Some(s) = val.strip_prefix('"') {
            Some(PrefValue::Text(s.trim_end_matches('"').to_string()))
        } else if let Some(i) = val.find(':').filter(|_| val.starts_with("hex")) {
            let bytes: Vec<u8> = val[i + 1..]
                .split(',')
                .filter_map(|b| u8::from_str_radix(b.trim(), 16).ok())
                .collect();
            Some(decode_bytes(&bytes))
        } else {
            None
        };
        if let Some(v) = v {
            out.push((unity_key(&name), v));
        }
    }
    Some(out)
}

/// Native Linux Unity games: ~/.config/unity3d/<Company>/<Product>/prefs
#[cfg_attr(windows, allow(dead_code))]
pub fn read_unity_prefs_xml(text: &str) -> Vec<(String, PrefValue)> {
    use base64::Engine as _;
    let mut out = Vec::new();
    for part in text.split("<pref ").skip(1) {
        let attr = |a: &str| {
            part.find(&format!("{a}=\""))
                .map(|i| &part[i + a.len() + 2..])
                .and_then(|r| r.find('"').map(|j| r[..j].to_string()))
        };
        let (Some(name), Some(ty)) = (attr("name"), attr("type")) else {
            continue;
        };
        let Some(gt) = part.find('>') else { continue };
        let body = part[gt + 1..]
            .split("</pref>")
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        let v = match ty.as_str() {
            "int" => body.parse().ok().map(PrefValue::Int),
            "float" => body.parse().ok().map(PrefValue::Float),
            _ => {
                let decoded = base64::engine::general_purpose::STANDARD
                    .decode(body.as_bytes())
                    .ok()
                    .and_then(|b| String::from_utf8(b).ok());
                Some(PrefValue::Text(decoded.unwrap_or(body)))
            }
        };
        if let Some(v) = v {
            out.push((name, v));
        }
    }
    out
}

#[cfg(windows)]
fn read_windows_reg(key: &str) -> Option<Vec<(String, PrefValue)>> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, RegType};
    let k = RegKey::predef(HKEY_CURRENT_USER).open_subkey(key).ok()?;
    let mut out = Vec::new();
    for (name, v) in k.enum_values().flatten().take(2000) {
        let pv = match v.vtype {
            RegType::REG_DWORD if v.bytes.len() >= 4 => {
                PrefValue::Int(
                    i32::from_le_bytes([v.bytes[0], v.bytes[1], v.bytes[2], v.bytes[3]]) as i64,
                )
            }
            RegType::REG_QWORD if v.bytes.len() >= 8 => {
                let mut a = [0u8; 8];
                a.copy_from_slice(&v.bytes[..8]);
                PrefValue::Float(f64::from_le_bytes(a))
            }
            RegType::REG_SZ => PrefValue::Text(
                String::from_utf16_lossy(
                    &v.bytes
                        .chunks(2)
                        .map(|c| u16::from_le_bytes([c[0], *c.get(1).unwrap_or(&0)]))
                        .collect::<Vec<_>>(),
                )
                .trim_end_matches('\0')
                .to_string(),
            ),
            _ => decode_bytes(&v.bytes),
        };
        out.push((unity_key(&name), pv));
    }
    Some(out)
}

/// Long numbers in setting names (Steam IDs, account numbers) are hidden.
pub fn hide_numbers(name: &str) -> String {
    let mut out = String::new();
    let mut digits = String::new();
    for ch in name.chars().chain(std::iter::once('\0')) {
        if ch.is_ascii_digit() {
            digits.push(ch);
            continue;
        }
        if digits.len() >= 8 {
            out.push_str("<number>");
        } else {
            out.push_str(&digits);
        }
        digits.clear();
        if ch != '\0' {
            out.push(ch);
        }
    }
    out
}

/// A text value that is really a bundle of settings (JSON, or key=value
/// lines). Returns the settings inside, flattened ("Graphics/Shadows").
fn unpack_text(t: &str) -> Option<BTreeMap<String, String>> {
    let s = t.trim();
    let parsed = if s.starts_with('{') || s.starts_with('[') {
        crate::games::parse("json", s).ok()
    } else if s.lines().count() >= 3 && s.lines().filter(|l| l.contains('=')).count() >= 3 {
        crate::games::parse("ini", s).ok()
    } else {
        None
    }?;
    (!parsed.is_empty()).then_some(parsed)
}

/// Text values are only kept when they look like a setting (short, no
/// spaces in odd places); anything else could be a player name, so it's
/// replaced before it reaches the settings list or a report.
fn safe_pref(name: &str, v: &PrefValue) -> String {
    match v {
        PrefValue::Text(s) => {
            let low = name.to_lowercase();
            let private = [
                "name", "user", "player", "account", "email", "steam", "lobby", "friend", "clan",
                "token", "id", "server", "ip", "host", "chat", "nick", "tag",
            ]
            .iter()
            .any(|w| low.contains(w));
            let looks_setting = s.len() <= 24
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "._-x: ".contains(c));
            if private || !looks_setting {
                "(text removed by Preflight)".into()
            } else {
                s.clone()
            }
        }
        other => other.as_setting(),
    }
}

/// Reads a Unity game's saved settings (PlayerPrefs). Returns a file entry,
/// the settings and a redacted text copy for bug reports.
fn unity_prefs(
    company: &str,
    product: &str,
    loc: &Located,
) -> Option<(FileResult, BTreeMap<String, String>, String)> {
    if product.is_empty() {
        return None;
    }
    let key = format!("Software\\{company}\\{product}");
    // (where it was read, the settings, the file if there is one)
    type Found = (String, Vec<(String, PrefValue)>, Option<PathBuf>);
    let mut found: Option<Found> = None;
    if let Some(pfx) = &loc.prefix {
        let reg = pfx.join("user.reg");
        if let Ok(t) = std::fs::read_to_string(&reg)
            && let Some(v) = read_wine_reg(&t, &key)
        {
            found = Some((
                format!(
                    "{} [{}]",
                    crate::util::scrub(&reg.to_string_lossy()),
                    key.replace('\\', "\\\\")
                ),
                v,
                Some(reg),
            ));
        }
    }
    #[cfg(not(windows))]
    if found.is_none()
        && let Some(h) = crate::util::home_dir()
    {
        let p = h
            .join(".config/unity3d")
            .join(company)
            .join(product)
            .join("prefs");
        if let Ok(t) = std::fs::read_to_string(&p) {
            found = Some((
                crate::util::scrub(&p.to_string_lossy()),
                read_unity_prefs_xml(&t),
                Some(p),
            ));
        }
    }
    #[cfg(windows)]
    if found.is_none() {
        if let Some(v) = read_windows_reg(&key) {
            found = Some((format!("Registry: HKEY_CURRENT_USER\\{key}"), v, None));
        }
    }
    let (shown, values, real) = found?;
    let mut map = BTreeMap::new();
    let mut text = String::new();
    for (k, v) in &values {
        let k = &hide_numbers(k);
        // Some games pack all their settings into one text value (JSON):
        // unpack it, keep the settings, drop private fields inside.
        if let PrefValue::Text(t) = v
            && let Some(inner) = unpack_text(t)
        {
            for (ik, iv) in inner {
                let full = format!("{k}/{}", hide_numbers(&ik));
                let safe = safe_pref(&ik, &PrefValue::Text(iv.clone()));
                let safe = if safe.starts_with('(') && iv.parse::<f64>().is_ok() {
                    iv.clone()
                } else {
                    safe
                };
                if !safe.starts_with('(') {
                    map.insert(full.clone(), safe.clone());
                }
                text.push_str(&format!("{full} = {safe}\n"));
            }
            continue;
        }
        let safe = safe_pref(k, v);
        if !safe.starts_with('(') {
            map.insert(k.clone(), safe.clone());
        }
        text.push_str(&format!("{k} = {safe}\n"));
    }
    let modified = real.as_ref().and_then(|p| modified_of(p));
    let fr = FileResult {
        shown_path: shown,
        found: true,
        format: "unity prefs".into(),
        read_ok: true,
        settings_count: map.len(),
        error: None,
        searched: vec![],
        note: Some(format!("Unity saved settings for {product}")),
        modified,
        real_path: None,
        inline_text: Some(text.clone()),
    };
    Some((fr, map, text))
}

fn modified_of(p: &Path) -> Option<i64> {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
}

// ---------------------------------------------------------------------------
// Engine-specific places

fn unreal_files(project: &str, roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut bases: Vec<PathBuf> = roots.to_vec();
    #[cfg(not(windows))]
    if let Some(h) = crate::util::home_dir() {
        bases.push(h.join(".config/Epic"));
    }
    #[cfg(windows)]
    if let Ok(l) = std::env::var("LOCALAPPDATA") {
        // Game Pass: Packages/<pkg>/LocalCache/Local/<Project>
        if let Ok(rd) = std::fs::read_dir(PathBuf::from(&l).join("Packages")) {
            for e in rd.flatten() {
                bases.push(e.path().join("LocalCache/Local"));
            }
        }
    }
    for b in bases {
        let Ok(rd) = std::fs::read_dir(&b) else {
            continue;
        };
        for e in rd.flatten() {
            if !e
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(project)
            {
                continue;
            }
            let cfg = e.path().join("Saved/Config");
            for plat in [
                "Windows",
                "WindowsNoEditor",
                "WindowsClient",
                "WinGDK",
                "Linux",
                "LinuxNoEditor",
                "LinuxClient",
            ] {
                for f in ["GameUserSettings.ini", "Engine.ini"] {
                    let p = cfg.join(plat).join(f);
                    if p.is_file() {
                        out.push(p);
                    }
                }
            }
        }
    }
    out
}

fn source_files(install: &Path, appid: Option<u32>, libs_roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut count = 0;
    out.extend(walk(
        install,
        4,
        &mut count,
        6000,
        &mut |p, d| {
            let n = p
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            !d && p
                .parent()
                .and_then(|x| x.file_name())
                .map(|x| x.to_string_lossy().eq_ignore_ascii_case("cfg"))
                .unwrap_or(false)
                && (n == "video.txt"
                    || n == "config.cfg"
                    || n.ends_with("machine_convars.vcfg")
                    || n == "autoexec.cfg")
        },
        &|p| {
            enter_dir(p)
                && !p
                    .file_name()
                    .map(|n| n.to_string_lossy().to_lowercase().ends_with("maps"))
                    .unwrap_or(false)
        },
    ));
    if let Some(id) = appid {
        for r in libs_roots {
            if let Ok(rd) = std::fs::read_dir(r.join("userdata")) {
                for u in rd.flatten() {
                    let cfg = u.path().join(format!("{id}/local/cfg"));
                    if let Ok(rd2) = std::fs::read_dir(&cfg) {
                        for f in rd2.flatten() {
                            let n = f.file_name().to_string_lossy().to_lowercase();
                            if n.contains("video") || n.contains("convars") {
                                out.push(f.path());
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Understanding settings without a profile

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    FrameGen,
    Upscaler,
    RayTracing,
    Reflex,
    RefreshRate,
    FpsLimit,
    VSync,
    Resolution,
    ResWidth,
    ResHeight,
    RenderScale,
    DisplayMode,
    Preset,
    Shadows,
    Volumetrics,
    GlobalIllum,
    Reflections,
    AmbientOcclusion,
    PostProcess,
    Effects,
    Foliage,
    ViewDistance,
    Shading,
    AntiAliasing,
    Textures,
    MotionBlur,
    DepthOfField,
    FilmGrain,
    ChromaticAb,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::FrameGen => "Frame generation",
            Kind::Upscaler => "Upscaler (DLSS/FSR/XeSS)",
            Kind::RayTracing => "Ray tracing",
            Kind::Reflex => "Low latency (Reflex/Anti-Lag)",
            Kind::RefreshRate => "Refresh rate",
            Kind::FpsLimit => "FPS limit",
            Kind::VSync => "V-Sync",
            Kind::Resolution | Kind::ResWidth | Kind::ResHeight => "Resolution",
            Kind::RenderScale => "Render scale",
            Kind::DisplayMode => "Display mode",
            Kind::Preset => "Graphics preset",
            Kind::Shadows => "Shadows",
            Kind::Volumetrics => "Volumetric effects (fog, clouds)",
            Kind::GlobalIllum => "Global illumination",
            Kind::Reflections => "Reflections",
            Kind::AmbientOcclusion => "Ambient occlusion",
            Kind::PostProcess => "Post-processing",
            Kind::Effects => "Effects and particles",
            Kind::Foliage => "Foliage",
            Kind::ViewDistance => "View distance",
            Kind::Shading => "Shading",
            Kind::AntiAliasing => "Anti-aliasing",
            Kind::Textures => "Textures",
            Kind::MotionBlur => "Motion blur",
            Kind::DepthOfField => "Depth of field",
            Kind::FilmGrain => "Film grain",
            Kind::ChromaticAb => "Chromatic aberration",
        }
    }
    fn heavy(self) -> bool {
        matches!(
            self,
            Kind::Shadows
                | Kind::Volumetrics
                | Kind::GlobalIllum
                | Kind::Reflections
                | Kind::AmbientOcclusion
                | Kind::PostProcess
                | Kind::Effects
                | Kind::Foliage
                | Kind::ViewDistance
                | Kind::Shading
                | Kind::AntiAliasing
        )
    }
    /// Effects that add fine noise or smear: they cost stream bitrate.
    pub fn costs_bitrate(self) -> bool {
        matches!(
            self,
            Kind::MotionBlur | Kind::FilmGrain | Kind::ChromaticAb | Kind::DepthOfField
        )
    }
}

/// (kind, name fragments). Checked in order; the first match wins.
const PATTERNS: &[(Kind, &[&str])] = &[
    (
        Kind::FrameGen,
        &["framegeneration", "framegen", "dlssg", "fgenabled"],
    ),
    (
        Kind::Upscaler,
        &[
            "dlss",
            "fsr",
            "xess",
            "upscal",
            "superresolution",
            "temporalsuper",
        ],
    ),
    (
        Kind::RayTracing,
        &[
            "raytrac",
            "pathtrac",
            "rtgi",
            "rtreflection",
            "rtshadow",
            "rtxon",
            "rayreconstruction",
        ],
    ),
    (Kind::Reflex, &["reflex", "lowlatency", "antilag"]),
    (
        Kind::RefreshRate,
        &["refreshrate", "refreshratenumerator", "monitorhz"],
    ),
    (
        Kind::FpsLimit,
        &[
            "fpsmax",
            "maxfps",
            "fpslimit",
            "frameratelimit",
            "framelimit",
            "maxframerate",
            "framecap",
            "fpscap",
            "targetframerate",
            "targetfps",
            "frameratecap",
            "limitfps",
            "fpstarget",
        ],
    ),
    (
        Kind::VSync,
        &[
            "vsync",
            "verticalsync",
            "waitforvertical",
            "vblank",
            "syncinterval",
        ],
    ),
    (
        Kind::ResHeight,
        &[
            "resolutionsizey",
            "resolutionheight",
            "screenheight",
            "defaultresheight",
            "resolutiony",
            "screenresy",
            "displayheight",
        ],
    ),
    (
        Kind::ResWidth,
        &[
            "resolutionsizex",
            "resolutionwidth",
            "screenwidth",
            "defaultres",
            "resolutionx",
            "screenresx",
            "displaywidth",
        ],
    ),
    (
        Kind::RenderScale,
        &[
            "resolutionquality",
            "resolutionscale",
            "renderscale",
            "renderresolution",
            "screenpercentage",
            "resolutionpercent",
        ],
    ),
    (Kind::Resolution, &["resolution"]),
    (
        Kind::DisplayMode,
        &[
            "fullscreenmode",
            "windowmode",
            "displaymode",
            "screenmode",
            "fullscreen",
            "windowed",
        ],
    ),
    (
        Kind::Preset,
        &[
            "qualitypreset",
            "graphicspreset",
            "overallquality",
            "unitygraphicsquality",
            "graphicsquality",
            "qualitylevel",
            "quickpreset",
            "presetquality",
            "globalquality",
            "videopreset",
            "preset",
        ],
    ),
    (Kind::Shadows, &["shadow"]),
    (
        Kind::Volumetrics,
        &["volumetric", "fog", "cloud", "godray", "lightshaft"],
    ),
    (
        Kind::GlobalIllum,
        &["globalillumination", "ssgi", "giquality"],
    ),
    (Kind::Reflections, &["reflection", "ssr"]),
    (
        Kind::AmbientOcclusion,
        &[
            "ambientocclusion",
            "ssao",
            "hbao",
            "gtao",
            "aodetail",
            "aoquality",
        ],
    ),
    (Kind::MotionBlur, &["motionblur"]),
    (
        Kind::DepthOfField,
        &["depthoffield", "dofquality", "dofenabled", "enabledof"],
    ),
    (Kind::FilmGrain, &["filmgrain", "grain"]),
    (Kind::ChromaticAb, &["chromatic"]),
    (Kind::PostProcess, &["postprocess"]),
    (
        Kind::Effects,
        &[
            "effectsquality",
            "particle",
            "vfx",
            "effectquality",
            "sgeffects",
        ],
    ),
    (Kind::Foliage, &["foliage", "grass", "vegetation"]),
    (
        Kind::ViewDistance,
        &[
            "viewdistance",
            "drawdistance",
            "lodquality",
            "lodbias",
            "detaildistance",
        ],
    ),
    (
        Kind::Shading,
        &["shadingquality", "shaderquality", "sgshading"],
    ),
    (
        Kind::AntiAliasing,
        &[
            "antialias",
            "msaa",
            "fxaa",
            "smaa",
            "cmaa",
            "aamode",
            "aaquality",
            "taaquality",
        ],
    ),
    (
        Kind::Textures,
        &[
            "texturequality",
            "texturedetail",
            "textureresolution",
            "texturesize",
            "sgtexture",
        ],
    ),
];

/// Name parts that mean the key isn't a graphics setting (or isn't the
/// current value).
const NOT_GRAPHICS: [&str; 34] = [
    "audio",
    "sound",
    "volume",
    "mouse",
    "sensitiv",
    "controller",
    "gamepad",
    "keybind",
    "binding",
    "input",
    "subtitle",
    "language",
    "voice",
    "chat",
    "hud",
    "crosshair",
    "colorblind",
    "tutorial",
    "benchmark",
    "recommend",
    "index",
    "detected",
    "supported",
    "lastuser",
    "lastconfirmed",
    "desired",
    "version",
    "tooltip",
    "hint",
    "prompt",
    "menu",
    "background",
    "unfocus",
    "inactive",
];

#[derive(Clone, Debug)]
pub struct Hit {
    pub kind: Kind,
    pub key: String,
    pub raw: String,
}

/// The last two parts of a setting's name, letters and digits only.
fn short_key(key: &str) -> String {
    let parts: Vec<&str> = key.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
    let tail = if parts.len() >= 2 {
        format!("{}{}", parts[parts.len() - 2], parts[parts.len() - 1])
    } else {
        parts.last().map(|s| s.to_string()).unwrap_or_default()
    };
    norm(&tail)
}

pub fn classify(key: &str, raw: &str) -> Option<Kind> {
    let v = raw.trim().trim_matches('"');
    if v.is_empty() || v.len() > 32 || v.contains('(') || v.contains('{') {
        return None;
    }
    let last = norm(key.rsplit(['/', '\\']).next().unwrap_or(key));
    let k = short_key(key);
    if NOT_GRAPHICS.iter().any(|w| last.contains(w))
        || (last.contains("default") && !last.contains("defaultres"))
    {
        return None;
    }
    // FPS limits for menus, tools or a minimised window don't count.
    if (last.contains("fps") || last.contains("framerate"))
        && (last.ends_with("ui")
            || last.contains("ui2")
            || last.contains("tools")
            || last.contains("loading")
            || last.contains("idle")
            || last.contains("battery"))
    {
        return None;
    }
    for (kind, pats) in PATTERNS {
        // Match on the setting's own name first, then with its group.
        if pats.iter().any(|p| last.contains(p))
            || (last.len() <= 8 && pats.iter().any(|p| k.contains(p)))
        {
            let kind = *kind;
            // A "resolution" that isn't WxH is a render scale or an index.
            if kind == Kind::Resolution
                && !(v.contains('x') && v.split('x').all(|n| n.trim().parse::<u32>().is_ok()))
            {
                return None;
            }
            if kind == Kind::Preset && last.contains("shadow") {
                return Some(Kind::Shadows);
            }
            return Some(kind);
        }
    }
    None
}

/// Every graphics setting recognised in a settings map.
pub fn recognise(settings: &BTreeMap<String, String>) -> Vec<Hit> {
    settings
        .iter()
        .filter_map(|(k, v)| {
            classify(k, v).map(|kind| Hit {
                kind,
                key: k.clone(),
                raw: v.trim().trim_matches('"').to_string(),
            })
        })
        .collect()
}

/// A level on a known scale.
#[derive(Clone, Debug, PartialEq)]
struct Level {
    rank: u8,
    label: String,
    /// Label of the "High" level on the same scale.
    high: String,
}

const WORDS: [(&str, u8); 14] = [
    ("verylow", 1),
    ("low", 1),
    ("medium", 2),
    ("normal", 2),
    ("mid", 2),
    ("high", 3),
    ("veryhigh", 4),
    ("ultra", 4),
    ("epic", 4),
    ("cinematic", 5),
    ("extreme", 5),
    ("max", 5),
    ("insane", 5),
    ("psycho", 5),
];

fn level_of(key: &str, raw: &str) -> Option<Level> {
    let v = norm(raw.rsplit(['-', '.']).next().unwrap_or(raw));
    if let Some((_, r)) = WORDS.iter().find(|(w, _)| *w == v) {
        return Some(Level {
            rank: *r,
            label: crate::games::pretty_value(raw.rsplit('-').next().unwrap_or(raw)),
            high: "High".into(),
        });
    }
    // Unreal scalability groups: 0 Low, 1 Medium, 2 High, 3 Epic, 4 Cinematic.
    let last = key.rsplit(['/', '\\']).next().unwrap_or(key).to_lowercase();
    if last.starts_with("sg.") {
        let n: i64 = raw.trim().parse::<f64>().ok()? as i64;
        let label = ["Low", "Medium", "High", "Epic", "Cinematic"]
            .get(n as usize)?
            .to_string();
        return Some(Level {
            rank: (n + 1).clamp(1, 5) as u8,
            label,
            high: "High".into(),
        });
    }
    None
}

/// Display mode numbers mean different things per engine.
fn display_label(key: &str, raw: &str) -> String {
    let k = key.to_lowercase();
    let n = num(raw).map(|n| n as i64);
    if k.contains("screenmanager") {
        // Unity
        return match n {
            Some(0) => "Exclusive fullscreen".into(),
            Some(1) => "Fullscreen window (borderless)".into(),
            Some(2) => "Maximized window".into(),
            Some(3) => "Windowed".into(),
            _ => crate::games::pretty_value(raw),
        };
    }
    if k.contains("fullscreenmode") {
        // Unreal
        return match n {
            Some(0) => "Fullscreen".into(),
            Some(1) => "Windowed fullscreen (borderless)".into(),
            Some(2) => "Windowed".into(),
            _ => crate::games::pretty_value(raw),
        };
    }
    if k.contains("windowed") {
        return match on(raw) {
            Some(true) => "Windowed".into(),
            Some(false) => "Fullscreen".into(),
            None => crate::games::pretty_value(raw),
        };
    }
    match on(raw) {
        Some(true) => "Fullscreen".into(),
        Some(false) => "Windowed".into(),
        None => crate::games::pretty_value(raw),
    }
}

/// Is an on/off-style effect on? Level words ("High") count as on.
fn effect_on(raw: &str) -> bool {
    let v = raw.trim().trim_matches('"').to_lowercase();
    match on(&v) {
        Some(b) => b,
        None => {
            !(v.ends_with("off")
                || v.ends_with("-off")
                || v == "none"
                || v == "disabled"
                || v.is_empty())
        }
    }
}

/// Bitrate-costly effects in a profile game's settings that its profile
/// doesn't already show.
pub fn bitrate_effects(
    settings: &BTreeMap<String, String>,
    game: &str,
    covered: &[String],
) -> Vec<CurrentSetting> {
    let mut out: Vec<CurrentSetting> = Vec::new();
    for h in merge_hits(vec![recognise(settings)])
        .into_iter()
        .filter(|h| h.kind.costs_bitrate())
    {
        let name = h.kind.name();
        let n = name.to_lowercase();
        if covered
            .iter()
            .any(|c| c.to_lowercase().contains(&n) || n.contains(&c.to_lowercase()))
            || out.iter().any(|c| c.setting == name)
        {
            continue;
        }
        let is_on = effect_on(&h.raw);
        out.push(CurrentSetting {
            rule_id: format!("auto_{}", norm(name)),
            setting: name.into(),
            effect: "bitrate".into(),
            menu: format!("{game} › Settings › Graphics"),
            value: if is_on {
                crate::games::pretty_value(&h.raw)
            } else {
                "Off".into()
            },
            flagged: is_on,
            change_to: is_on.then(|| "Off".to_string()),
            gain: Some("A cleaner stream picture at the same bitrate".into()),
            cost: Some("Only a look you may like".into()),
        });
    }
    out
}

fn on(raw: &str) -> Option<bool> {
    match raw.trim().trim_matches('"').to_lowercase().as_str() {
        "true" | "on" | "yes" | "enabled" | "1" => Some(true),
        "false" | "off" | "no" | "disabled" | "0" | "none" => Some(false),
        _ => None,
    }
}

fn num(raw: &str) -> Option<f64> {
    raw.trim().trim_matches('"').parse::<f64>().ok()
}

/// What the reader produced for one game, before it's turned into a GameResult.
#[derive(Clone, Debug, Default)]
pub struct Reading {
    pub files: Vec<FileResult>,
    /// Settings from every file, engine places first (they win on clashes).
    pub settings: BTreeMap<String, String>,
    pub hits: Vec<Hit>,
    pub engine: String,
    /// Plain words for what couldn't be read, if anything.
    pub gap: Option<String>,
}

/// One answer per setting: the first file that has a setting wins
/// (engine places come before files found by name).
fn merge_hits(per_file: Vec<Vec<Hit>>) -> Vec<Hit> {
    // Within one file, prefer "ShadowQuality" over "DynamicShadows" and the
    // refresh rate's numerator over its denominator.
    let rank = |h: &Hit| {
        let k = h.key.to_lowercase();
        if k.contains("quality")
            || k.contains("level")
            || k.contains("numerator")
            || k.ends_with("mode")
            || k.contains("detail")
        {
            0
        } else if k.contains("dynamic") || k.contains("denominator") {
            2
        } else {
            1
        }
    };
    let mut out: Vec<Hit> = Vec::new();
    for mut hits in per_file {
        hits.sort_by_key(|h| (h.kind, rank(h)));
        for h in hits {
            if !out.iter().any(|o| o.kind == h.kind) {
                out.push(h);
            }
        }
    }
    out
}

fn guess_format(p: &Path, text: &str) -> &'static str {
    let ext = p
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let t = text.trim_start_matches('\u{feff}').trim_start();
    if t.starts_with('{') || t.starts_with('[') && ext == "json" {
        return "json";
    }
    if t.starts_with('<') || ext == "xml" {
        return "xml";
    }
    if ext == "coc" {
        return "coc";
    }
    if t.starts_with('"') || ext == "vcfg" {
        return "kv";
    }
    let lines: Vec<&str> = t
        .lines()
        .take(60)
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with(['#', ';', '/']))
        .collect();
    let eq = lines.iter().filter(|l| l.contains('=')).count();
    let colon = lines
        .iter()
        .filter(|l| l.contains(':') && !l.contains('='))
        .count();
    if eq > 0 && eq >= colon {
        return "ini";
    }
    if colon > 0 {
        return "lines";
    }
    "spaced"
}

/// "key value" lines (Frostbite's PROFSAVE_profile, Source's config.cfg).
fn parse_spaced(text: &str) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with("//") || t.starts_with('#') || t.starts_with("bind") {
            continue;
        }
        if let Some((k, v)) = t.split_once(char::is_whitespace) {
            m.insert(
                k.trim_matches('"').to_string(),
                v.trim().trim_matches('"').to_string(),
            );
        }
    }
    m
}

fn read_settings_file(p: &Path) -> Result<BTreeMap<String, String>, String> {
    let bytes = std::fs::read(p).map_err(|e| format!("could not open: {e}"))?;
    if bytes.len() > 512 * 1024 {
        return Err(format!("too big to read ({} KB)", bytes.len() / 1024));
    }
    let head = &bytes[..bytes.len().min(4096)];
    let bad = head
        .iter()
        .filter(|b| **b == 0 || (**b < 9) || (**b > 13 && **b < 32))
        .count();
    if bad > head.len() / 20 {
        return Err("saved in a binary format Preflight can't read".into());
    }
    let text = String::from_utf8_lossy(&bytes);
    match guess_format(p, &text) {
        "spaced" => Ok(parse_spaced(&text)),
        f => crate::games::parse(f, &text).or_else(|_| Ok(parse_spaced(&text))),
    }
}

fn file_result(
    p: &Path,
    format: &str,
    res: &Result<BTreeMap<String, String>, String>,
) -> FileResult {
    FileResult {
        shown_path: crate::util::scrub(&p.to_string_lossy()),
        found: true,
        format: format.into(),
        read_ok: res.is_ok(),
        settings_count: res.as_ref().map(|m| m.len()).unwrap_or(0),
        error: res.as_ref().err().cloned(),
        searched: vec![],
        note: None,
        modified: modified_of(p),
        real_path: Some(p.to_path_buf()),
        inline_text: None,
    }
}

/// Where one game's settings are: found once, then re-read cheaply.
#[derive(Clone, Debug, Default)]
struct Discovery {
    engine: String,
    unity: Option<(String, String)>,
    is_unity: bool,
    /// (file, found in an engine's own place)
    files: Vec<(PathBuf, bool)>,
}

static DISCOVERED: std::sync::Mutex<Vec<(String, std::time::Instant, Discovery)>> =
    std::sync::Mutex::new(Vec::new());

/// Looks for a game's settings files. Folder searches are the slow part,
/// so results are kept for 5 minutes; the files themselves are re-read on
/// every scan.
fn discover(loc: &Located, steam_roots: &[PathBuf]) -> Discovery {
    let key = format!(
        "{}|{}",
        loc.exe,
        loc.install
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default()
    );
    if let Ok(cache) = DISCOVERED.lock()
        && let Some((_, t, d)) = cache.iter().find(|(k, _, _)| *k == key)
        && t.elapsed().as_secs() < 300
    {
        return d.clone();
    }
    let mut d = Discovery::default();
    let engine = loc
        .install
        .as_deref()
        .map(|i| detect_engine(i, &loc.exe))
        .unwrap_or(Engine::Unknown);
    d.engine = engine.label().to_string();
    let roots = user_roots(loc);
    match &engine {
        Engine::Unity { company, product } => {
            d.is_unity = true;
            if !product.is_empty() {
                d.unity = Some((company.clone(), product.clone()));
            }
        }
        Engine::Unreal { project } => d
            .files
            .extend(unreal_files(project, &roots).into_iter().map(|p| (p, true))),
        Engine::Source => {
            if let Some(i) = &loc.install {
                d.files.extend(
                    source_files(i, loc.appid, steam_roots)
                        .into_iter()
                        .map(|p| (p, true)),
                );
            }
        }
        _ => {}
    }
    let toks = tokens(loc, &engine);
    let mut search_roots = roots.clone();
    search_roots.extend(native_roots());
    #[cfg(windows)]
    for v in ["LOCALAPPDATA", "APPDATA"] {
        if let Ok(p) = std::env::var(v) {
            search_roots.push(PathBuf::from(&p));
        }
    }
    #[cfg(windows)]
    if let Ok(l) = std::env::var("LOCALAPPDATA") {
        if let Ok(rd) = std::fs::read_dir(PathBuf::from(l).join("Packages")) {
            for e in rd.flatten() {
                search_roots.push(e.path().join("LocalCache/Local"));
                search_roots.push(e.path().join("LocalCache/Roaming"));
            }
        }
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    for folder in matching_folders(&search_roots, &toks) {
        let mut count = 0;
        candidates.extend(walk(
            &folder,
            6,
            &mut count,
            1500,
            &mut |p, d| !d && settings_like(p),
            &enter_dir,
        ));
    }
    // Steam Cloud keeps some games' settings here.
    if let Some(id) = loc.appid {
        for r0 in steam_roots {
            if let Ok(rd) = std::fs::read_dir(r0.join("userdata")) {
                for u in rd.flatten() {
                    let mut count = 0;
                    candidates.extend(walk(
                        &u.path().join(id.to_string()),
                        4,
                        &mut count,
                        400,
                        &mut |p, d| !d && settings_like(p),
                        &enter_dir,
                    ));
                }
            }
        }
    }
    // A few games keep settings next to the program.
    if let Some(i) = &loc.install {
        let mut count = 0;
        candidates.extend(walk(
            i,
            3,
            &mut count,
            3000,
            &mut |p, d| {
                let n = p
                    .file_name()
                    .map(|n| n.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                !d && settings_like(p)
                    && [
                        "setting", "config", "option", "video", "graphic", "prefs", "user",
                    ]
                    .iter()
                    .any(|w| n.contains(w))
                    && !n.contains("boot")
                    && !n.contains("runtime")
            },
            &|p| {
                enter_dir(p)
                    && !p
                        .file_name()
                        .map(|n| n.to_string_lossy().ends_with("_Data"))
                        .unwrap_or(false)
            },
        ));
    }
    // Newest first: the file the game saved last is the one in use.
    candidates.sort_by_key(|p| std::cmp::Reverse(modified_of(p).unwrap_or(0)));
    let mut kept = 0;
    for p in candidates.into_iter().take(60) {
        if kept >= 8 || d.files.iter().any(|(f, _)| *f == p) {
            continue;
        }
        // Keep files that hold graphics settings, or settings-named files
        // that couldn't be read (so the gap can be explained).
        match read_settings_file(&p) {
            Ok(m) if recognise(&m).len() >= 2 => {
                d.files.push((p, false));
                kept += 1;
            }
            Err(_) => {
                let n = p
                    .file_name()
                    .map(|n| n.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                if ["setting", "config", "option", "profile", "graphic", "video"]
                    .iter()
                    .any(|w| n.contains(w))
                {
                    d.files.push((p, false));
                    kept += 1;
                }
            }
            _ => {}
        }
    }
    if let Ok(mut cache) = DISCOVERED.lock() {
        cache.retain(|(k, _, _)| *k != key);
        cache.push((key, std::time::Instant::now(), d.clone()));
    }
    d
}

/// Reads everything Preflight can find for one game.
pub fn read_game(loc: &Located, steam_roots: &[PathBuf]) -> Reading {
    let d = discover(loc, steam_roots);
    let mut r = Reading {
        engine: d.engine.clone(),
        ..Default::default()
    };
    let mut per_file: Vec<Vec<Hit>> = Vec::new();
    if let Some((company, product)) = &d.unity
        && let Some((fr, map, _)) = unity_prefs(company, product, loc)
    {
        per_file.push(recognise(&map));
        for (k, v) in map {
            r.settings.entry(k).or_insert(v);
        }
        r.files.push(fr);
    }
    let mut seen: Vec<PathBuf> = Vec::new();
    for (p, _engine_place) in &d.files {
        let canon = std::fs::canonicalize(p).unwrap_or_else(|_| p.clone());
        if seen.contains(&canon) {
            continue;
        }
        seen.push(canon);
        let res = read_settings_file(p);
        let hits = res.as_ref().map(recognise).unwrap_or_default();
        let fmt = match &res {
            Ok(_) => std::fs::read(p)
                .ok()
                .map(|b| guess_format(p, &String::from_utf8_lossy(&b[..b.len().min(8192)])))
                .unwrap_or("text"),
            Err(_) => "unknown",
        };
        r.files.push(file_result(p, fmt, &res));
        if let Ok(m) = res {
            for (k, v) in m {
                r.settings.entry(k).or_insert(v);
            }
        }
        per_file.push(hits);
    }
    r.hits = merge_hits(per_file);
    r.gap = if r.files.is_empty() {
        Some(if d.is_unity {
            "It's a Unity game, but it hasn't saved any settings yet. Open its settings menu and change something, then rescan.".into()
        } else {
            "Preflight couldn't find where this game keeps its settings.".into()
        })
    } else if r.files.iter().all(|f| !f.read_ok) {
        Some(
            "This game keeps its settings in a format Preflight can't read (binary or encrypted)."
                .into(),
        )
    } else if r.hits.is_empty() {
        Some("Preflight found this game's settings files but didn't recognise any graphics settings in them.".into())
    } else {
        None
    };
    r
}

// ---------------------------------------------------------------------------
// Turning a reading into advice

fn hit_of(hits: &[Hit], k: Kind) -> Option<&Hit> {
    hits.iter().find(|h| h.kind == k)
}

pub fn to_game_result(loc: &Located, rd: Reading, running: bool) -> GameResult {
    let name = loc.name.clone();
    let gfx = format!("{name} › Settings › Graphics");
    let video = format!("{name} › Settings › Video / Display");
    let menu_for = |k: Kind| match k {
        Kind::FpsLimit
        | Kind::VSync
        | Kind::Resolution
        | Kind::ResWidth
        | Kind::ResHeight
        | Kind::RefreshRate
        | Kind::DisplayMode
        | Kind::RenderScale
        | Kind::Upscaler
        | Kind::FrameGen
        | Kind::Reflex => video.clone(),
        _ => gfx.clone(),
    };
    let hits = &rd.hits;
    let mut findings: Vec<Finding> = Vec::new();
    let mut current: Vec<CurrentSetting> = Vec::new();
    let mut tunables: Vec<TunableState> = Vec::new();
    let mut fps_cap = None;
    let finding = |effect: &str, label: &str, h: &Hit, to: Option<&str>, menu: String| Finding {
        rule_id: format!("auto_{}", norm(h.kind.name())),
        effect: effect.into(),
        label: label.into(),
        menu,
        key: h.key.clone(),
        value: h.raw.clone(),
        change_to: to.map(|s| s.to_string()),
        gain: None,
        cost: None,
    };

    if let Some(h) = hit_of(hits, Kind::FpsLimit) {
        match num(&h.raw) {
            Some(n) if n > 0.0 && n < 999.0 => fps_cap = Some(n as u32),
            Some(_) => findings.push(finding(
                "uncapped_fps",
                "FPS limit is off (unlimited)",
                h,
                None,
                menu_for(Kind::FpsLimit),
            )),
            None if on(&h.raw) == Some(false) || h.raw.to_lowercase().contains("unlimit") => {
                findings.push(finding(
                    "uncapped_fps",
                    "FPS limit is off (unlimited)",
                    h,
                    None,
                    menu_for(Kind::FpsLimit),
                ))
            }
            None => {}
        }
    }
    if let Some(h) = hit_of(hits, Kind::RayTracing)
        && (on(&h.raw) == Some(true)
            || level_of(&h.key, &h.raw)
                .map(|l| l.rank >= 1)
                .unwrap_or(false))
    {
        let mut f = finding(
            "rt_on",
            "Ray tracing is on",
            h,
            Some("Off"),
            menu_for(Kind::RayTracing),
        );
        f.gain = Some("Large GPU headroom".into());
        f.cost = Some("Less realistic lighting and reflections".into());
        findings.push(f);
    }
    let ups: Vec<&Hit> = hits.iter().filter(|h| h.kind == Kind::Upscaler).collect();
    if !ups.is_empty()
        && ups
            .iter()
            .all(|h| on(&h.raw) == Some(false) || h.raw.eq_ignore_ascii_case("off"))
    {
        findings.push(finding(
            "upscaler_off",
            "Upscaling is off",
            ups[0],
            Some("Quality"),
            menu_for(Kind::Upscaler),
        ));
    }
    if let Some(h) = hit_of(hits, Kind::VSync)
        && on(&h.raw) == Some(true)
    {
        findings.push(finding(
            "vsync_on",
            "V-Sync is on",
            h,
            None,
            menu_for(Kind::VSync),
        ));
    }

    // Heavy settings, most GPU saved first.
    for h in hits.iter().filter(|h| h.kind.heavy()) {
        let Some(l) = level_of(&h.key, &h.raw) else {
            continue;
        };
        let change_to = (l.rank > 3).then(|| l.high.clone());
        let (gain, cost) = match h.kind {
            Kind::Shadows => (
                "Often the biggest single GPU saving",
                "Softer, shorter shadows",
            ),
            Kind::Volumetrics => (
                "Big GPU saving in foggy or cloudy scenes",
                "Less detailed fog and clouds",
            ),
            Kind::GlobalIllum => ("Big GPU saving", "Flatter indirect lighting"),
            Kind::Reflections => ("Good GPU saving", "Blurrier reflections"),
            Kind::AmbientOcclusion => ("Some GPU saving", "Slightly flatter contact shadows"),
            Kind::AntiAliasing => ("Some GPU saving", "Slightly more jagged edges"),
            _ => ("Some GPU saving", "A small drop in detail"),
        };
        tunables.push(TunableState {
            id: format!("auto_{}", norm(h.kind.name())),
            setting: h.kind.name().into(),
            menu: menu_for(h.kind),
            now: l.label.clone(),
            change_to,
            gain: gain.into(),
            cost: cost.into(),
        });
    }

    // The preset: a named level, or Unreal's groups when they all match.
    let preset = hit_of(hits, Kind::Preset)
        .and_then(|h| {
            if let Some(l) = level_of(&h.key, &h.raw) {
                let lower = WORDS
                    .iter()
                    .filter(|(_, r)| *r == l.rank.saturating_sub(1) && *r > 0)
                    .map(|(w, _)| crate::games::pretty_value(w))
                    .next();
                return Some(PresetState {
                    setting: "Graphics preset".into(),
                    menu: gfx.clone(),
                    value: l.label,
                    custom: false,
                    lower,
                });
            }
            let n = num(&h.raw)? as i64;
            (0..=10).contains(&n).then(|| PresetState {
                setting: "Graphics preset".into(),
                menu: gfx.clone(),
                value: format!("Level {n}"),
                custom: false,
                lower: (n > 0).then(|| format!("Level {} (one step lower)", n - 1)),
            })
        })
        .or_else(|| {
            let sg: Vec<Level> = hits
                .iter()
                .filter(|h| h.key.to_lowercase().contains("sg.") && h.kind != Kind::RenderScale)
                .filter_map(|h| level_of(&h.key, &h.raw))
                .collect();
            if sg.len() < 3 {
                return None;
            }
            let same = sg.iter().all(|l| l.rank == sg[0].rank);
            let lower = ["Low", "Medium", "High", "Epic", "Cinematic"]
                .get((sg[0].rank as usize).saturating_sub(2))
                .filter(|_| sg[0].rank > 1)
                .map(|s| s.to_string());
            Some(PresetState {
                setting: "Graphics preset".into(),
                menu: gfx.clone(),
                value: if same {
                    sg[0].label.clone()
                } else {
                    "Custom".into()
                },
                custom: !same,
                lower: if same { lower } else { None },
            })
        });

    // Display mode.
    let (w, hgt) = (
        hit_of(hits, Kind::ResWidth).and_then(|h| num(&h.raw)),
        hit_of(hits, Kind::ResHeight).and_then(|h| num(&h.raw)),
    );
    let display_res = hit_of(hits, Kind::Resolution)
        .map(|h| h.raw.replace(' ', ""))
        .or_else(|| match (w, hgt) {
            (Some(a), Some(b)) if a >= 320.0 && b >= 200.0 => {
                Some(format!("{}x{}", a as u32, b as u32))
            }
            _ => None,
        });
    let display_hz = hit_of(hits, Kind::RefreshRate)
        .and_then(|h| num(&h.raw))
        .map(|n| if n > 1000.0 { n / 1000.0 } else { n } as f32)
        .filter(|h| *h > 10.0 && *h < 1000.0);

    // Every recognised setting, for the "Your setup" table.
    let mut shown_kinds: Vec<Kind> = Vec::new();
    for h in hits {
        let k = match h.kind {
            Kind::ResWidth | Kind::ResHeight => Kind::Resolution,
            k => k,
        };
        if shown_kinds.contains(&k) {
            continue;
        }
        shown_kinds.push(k);
        // The resolution is shown as the game's display mode instead.
        if k == Kind::Resolution && display_res.is_some() {
            continue;
        }
        let value = match k {
            Kind::Resolution => display_res
                .clone()
                .unwrap_or_else(|| crate::games::pretty_value(&h.raw)),
            Kind::FpsLimit => fps_cap
                .map(|c| c.to_string())
                .unwrap_or_else(|| "Off (unlimited)".into()),
            Kind::RenderScale => num(&h.raw)
                .map(|n| {
                    if n <= 2.0 {
                        format!("{:.0}%", n * 100.0)
                    } else {
                        format!("{n:.0}%")
                    }
                })
                .unwrap_or_else(|| h.raw.clone()),
            Kind::DisplayMode => display_label(&h.key, &h.raw),
            Kind::Preset if level_of(&h.key, &h.raw).is_none() && num(&h.raw).is_some() => {
                format!("Level {}", h.raw.trim())
            }
            Kind::AntiAliasing
                if h.key.to_lowercase().contains("msaa")
                    && num(&h.raw).map(|n| n >= 2.0).unwrap_or(false) =>
            {
                format!("{}x MSAA", h.raw.trim())
            }
            _ => level_of(&h.key, &h.raw)
                .map(|l| l.label)
                .unwrap_or_else(|| crate::games::pretty_value(&h.raw)),
        };
        let effect = if findings.iter().any(|f| f.key == h.key) {
            findings
                .iter()
                .find(|f| f.key == h.key)
                .map(|f| f.effect.clone())
                .unwrap_or_default()
        } else if k.costs_bitrate() {
            "bitrate".into()
        } else if k.heavy() {
            "heavy".into()
        } else if matches!(k, Kind::FrameGen | Kind::Upscaler) {
            "info".into()
        } else {
            "setting".into()
        };
        let flagged = findings
            .iter()
            .any(|f| f.key == h.key && f.effect != "vsync_on")
            || tunables
                .iter()
                .any(|t| t.setting == k.name() && t.change_to.is_some())
            || (k.costs_bitrate() && effect_on(&h.raw));
        let change_to = if k.costs_bitrate() && effect_on(&h.raw) {
            Some("Off".to_string())
        } else {
            tunables
                .iter()
                .find(|t| t.setting == k.name())
                .and_then(|t| t.change_to.clone())
        };
        current.push(CurrentSetting {
            rule_id: format!("auto_{}", norm(k.name())),
            setting: k.name().into(),
            effect,
            menu: menu_for(k),
            value,
            flagged,
            change_to,
            gain: k
                .costs_bitrate()
                .then(|| "A cleaner stream picture at the same bitrate".to_string()),
            cost: k
                .costs_bitrate()
                .then(|| "Only a look you may like".to_string()),
        });
    }

    let used = hits.len();
    let proton = loc.prefix.is_some() && cfg!(not(windows));
    let engine_text = if rd.engine.is_empty() {
        String::new()
    } else {
        format!(", {} engine", rd.engine)
    };
    let key_names = if hits.is_empty() {
        rd.settings.keys().take(200).cloned().collect()
    } else {
        Vec::new()
    };
    GameResult {
        id: format!(
            "auto:{}",
            loc.appid
                .map(|a| a.to_string())
                .unwrap_or_else(|| exe_stem(&loc.exe))
        ),
        name,
        load: "gpu".into(),
        profile_version: 0,
        verified: format!("Read automatically{engine_text}, not hand-checked"),
        installed: true,
        install_hint: if loc.launcher.is_empty() {
            String::new()
        } else {
            loc.launcher.clone()
        },
        running,
        platform_note: None,
        files: rd.files,
        findings,
        unmatched_rules: vec![],
        key_names,
        current,
        preset,
        tunables,
        fps_cap,
        unreadable: vec![],
        has_fps_limit: hit_of(hits, Kind::FpsLimit).is_some(),
        api: None,
        proton,
        display_res,
        display_hz,
        auto: Some(if rd.engine.is_empty() {
            "automatic".into()
        } else {
            rd.engine.clone()
        }),
        processes: vec![loc.exe.clone()],
        settings_read: rd.settings.len(),
        settings_used: used,
        read_gap: rd.gap,
        launcher: loc.launcher.clone(),
        motion: "medium".into(),
    }
}

/// Finds and reads games without a profile: running ones, and ones only
/// known by program name (e.g. from the last test).
pub fn scan(
    running: &[GameProc],
    hints: &[String],
    known_exes: &[String],
    known_appids: &[u32],
    steam_roots: &[PathBuf],
    libs: &[PathBuf],
) -> Vec<GameResult> {
    let manifests = steam_manifests(libs);
    let mut cache = load_cache();
    let mut out: Vec<GameResult> = Vec::new();
    let mut done: Vec<String> = Vec::new();
    let is_known = |exe: &str, appid: Option<u32>| {
        known_exes
            .iter()
            .any(|k| crate::games::process_matches(exe, k))
            || appid.map(|a| known_appids.contains(&a)).unwrap_or(false)
    };
    let mut cache_changed = false;
    for p in running {
        if is_known(&p.exe, p.steam_appid) || done.contains(&p.exe) {
            continue;
        }
        let Some(loc) = locate_running(p, &manifests, libs) else {
            continue;
        };
        if is_known(&loc.exe, loc.appid) || out.iter().any(|g| g.name == loc.name) {
            continue;
        }
        done.push(p.exe.clone());
        if cache.get(&loc.exe) != Some(&loc) {
            cache.insert(loc.exe.clone(), loc.clone());
            cache_changed = true;
        }
        let rd = read_game(&loc, steam_roots);
        log_reading(&loc, &rd);
        out.push(to_game_result(&loc, rd, true));
    }
    for h in hints {
        let h = h.to_lowercase();
        if done.contains(&h) || is_known(&h, None) {
            continue;
        }
        done.push(h.clone());
        let Some(loc) = locate_hint(&h, &cache, &manifests, libs) else {
            continue;
        };
        if is_known(&loc.exe, loc.appid) || out.iter().any(|g| g.name == loc.name) {
            continue;
        }
        let rd = read_game(&loc, steam_roots);
        log_reading(&loc, &rd);
        out.push(to_game_result(&loc, rd, false));
    }
    if cache_changed {
        // Keep the list short: the 40 most recent games.
        while cache.len() > 40 {
            if let Some(k) = cache.keys().next().cloned() {
                cache.remove(&k);
            }
        }
        save_cache(&cache);
    }
    out
}

fn log_reading(loc: &Located, rd: &Reading) {
    crate::logger::info_changed(
        &format!("auto:{}", loc.exe),
        format!(
            "{} (no profile, {}{}): {} file(s), {} settings, {} recognised{}",
            loc.name,
            if rd.engine.is_empty() {
                "engine unknown"
            } else {
                &rd.engine
            },
            if loc.launcher.is_empty() {
                String::new()
            } else {
                format!(", {}", loc.launcher)
            },
            rd.files.len(),
            rd.settings.len(),
            rd.hits.len(),
            rd.gap
                .as_ref()
                .map(|g| format!(" · {g}"))
                .unwrap_or_default()
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unity_wine_registry() {
        let reg = "WINE REGISTRY Version 2\n\n[Software\\\\Moon Studio\\\\Moon Sweepers] 1731600000\n#time=1db\n\"Screenmanager Resolution Width_h182942802\"=dword:00000a00\n\"Screenmanager Resolution Height_h2627697771\"=dword:000005a0\n\"Screenmanager Fullscreen mode_h3630240806\"=dword:00000001\n\"UnityGraphicsQuality_h1669003810\"=dword:00000004\n\"PlayerName_h123\"=hex:4d,61,74,74,00\n\"VSync_h99\"=dword:00000000\n\"MaxFPS_h5\"=hex(4):00,00,00,00,00,00,6e,40\n\n[Software\\\\Other] 1\n\"x\"=dword:1\n";
        let v = read_wine_reg(reg, "Software\\Moon Studio\\Moon Sweepers").unwrap();
        assert!(v.contains(&(
            "Screenmanager Resolution Width".into(),
            PrefValue::Int(2560)
        )));
        assert!(v.contains(&("UnityGraphicsQuality".into(), PrefValue::Int(4))));
        assert!(
            v.iter()
                .any(|(k, val)| k == "MaxFPS" && *val == PrefValue::Float(240.0))
        );
        assert!(
            !v.iter().any(|(k, _)| k == "x"),
            "other sections are not read"
        );
        // A player name never makes it into the settings or the report text.
        let name = v.iter().find(|(k, _)| k == "PlayerName").unwrap();
        assert_eq!(safe_pref(&name.0, &name.1), "(text removed by Preflight)");
    }

    #[test]
    fn unity_reading_gives_advice() {
        let loc = Located {
            name: "Moon Sweepers".into(),
            exe: "moonsweepers.exe".into(),
            launcher: "Steam".into(),
            ..Default::default()
        };
        let mut map = BTreeMap::new();
        map.insert(
            "Screenmanager Resolution Width".to_string(),
            "2560".to_string(),
        );
        map.insert(
            "Screenmanager Resolution Height".to_string(),
            "1440".to_string(),
        );
        map.insert("UnityGraphicsQuality".to_string(), "4".to_string());
        map.insert("VSync".to_string(), "0".to_string());
        map.insert("MaxFPS".to_string(), "0".to_string());
        map.insert("ShadowQuality".to_string(), "Ultra".to_string());
        map.insert("MotionBlur".to_string(), "1".to_string());
        let hits = recognise(&map);
        let rd = Reading {
            settings: map,
            hits,
            engine: "Unity".into(),
            ..Default::default()
        };
        let g = to_game_result(&loc, rd, true);
        assert_eq!(g.display_res.as_deref(), Some("2560x1440"));
        assert!(g.findings.iter().any(|f| f.effect == "uncapped_fps"));
        assert_eq!(g.preset.as_ref().map(|p| p.value.as_str()), Some("Level 4"));
        let sh = g.tunables.iter().find(|t| t.setting == "Shadows").unwrap();
        assert_eq!(sh.change_to.as_deref(), Some("High"));
        assert!(
            g.current
                .iter()
                .any(|c| c.setting == "Motion blur" && c.effect == "bitrate" && c.flagged)
        );
        assert_eq!(g.settings_used, 7);
        assert!(g.verified.contains("automatically"));
    }

    #[test]
    fn unreal_file_from_halo_mcc() {
        let text = include_str!("testdata/mcc_gameusersettings.ini");
        let m = crate::games::parse("ini", text).unwrap();
        let hits = recognise(&m);
        let kinds: Vec<Kind> = hits.iter().map(|h| h.kind).collect();
        for k in [
            Kind::Shadows,
            Kind::VSync,
            Kind::ResWidth,
            Kind::ResHeight,
            Kind::DisplayMode,
            Kind::AntiAliasing,
            Kind::PostProcess,
            Kind::Foliage,
        ] {
            assert!(kinds.contains(&k), "{k:?} not recognised; got {kinds:?}");
        }
        // The long throttle lists are ignored, not misread.
        assert!(
            !hits
                .iter()
                .any(|h| h.key.contains("GraphicsQualityThrottles"))
        );
        let loc = Located {
            name: "Halo MCC".into(),
            exe: "mcc-win64-shipping.exe".into(),
            ..Default::default()
        };
        let rd = Reading {
            hits: merge_hits(vec![hits]),
            settings: m,
            engine: "Unreal".into(),
            ..Default::default()
        };
        let g = to_game_result(&loc, rd, true);
        assert_eq!(g.display_res.as_deref(), Some("1280x720"));
        assert_eq!(
            g.preset.as_ref().map(|p| p.value.as_str()),
            Some("Medium"),
            "all groups at 1 = Medium"
        );
        assert!(g.findings.iter().any(|f| f.effect == "vsync_on"));
    }

    #[test]
    fn source_files_recognised() {
        let m = crate::games::parse("kv", include_str!("testdata/cs2_video.txt")).unwrap();
        let kinds: Vec<Kind> = recognise(&m).iter().map(|h| h.kind).collect();
        for k in [
            Kind::Shadows,
            Kind::ResWidth,
            Kind::ResHeight,
            Kind::VSync,
            Kind::AntiAliasing,
            Kind::AmbientOcclusion,
            Kind::Effects,
            Kind::Upscaler,
            Kind::Reflex,
        ] {
            assert!(kinds.contains(&k), "{k:?} missing from {kinds:?}");
        }
        let m2 =
            crate::games::parse("kv", include_str!("testdata/cs2_machine_convars.vcfg")).unwrap();
        let fps = recognise(&m2)
            .into_iter()
            .find(|h| h.kind == Kind::FpsLimit)
            .unwrap();
        assert_eq!(fps.key, "fps_max", "menu and tools limits are ignored");
        assert_eq!(fps.raw, "400");
    }

    #[test]
    fn frostbite_and_spaced_files() {
        let m = parse_spaced(
            "GstRender.ShadowQuality 3\nGstRender.FrameRateLimit 200.000000\nGstRender.VSyncMode 0\nGstRender.MotionBlurEnable 1\nbind w forward\n",
        );
        let hits = recognise(&m);
        assert!(
            hits.iter()
                .any(|h| h.kind == Kind::FpsLimit && h.raw.starts_with("200"))
        );
        assert!(hits.iter().any(|h| h.kind == Kind::MotionBlur));
        assert!(!m.keys().any(|k| k == "bind"));
    }

    #[test]
    fn native_unity_prefs() {
        let xml = "<unity_prefs version_major=\"1\" version_minor=\"1\">\n\t<pref name=\"Screenmanager Resolution Width\" type=\"int\">1920</pref>\n\t<pref name=\"Quality\" type=\"string\">SGlnaA==</pref>\n</unity_prefs>";
        let v = read_unity_prefs_xml(xml);
        assert!(v.contains(&(
            "Screenmanager Resolution Width".into(),
            PrefValue::Int(1920)
        )));
        assert!(v.contains(&("Quality".into(), PrefValue::Text("High".into()))));
    }

    #[test]
    fn names_and_engines() {
        assert_eq!(exe_stem("MCC-Win64-Shipping.exe"), "mcc");
        assert_eq!(norm("Moon Sweepers"), "moonsweepers");
        assert!(name_matches("Moon Sweepers", &["moonsweepers".into()]));
        assert!(!name_matches("Temp", &["moonsweepers".into()]));
        let dir = std::env::temp_dir().join(format!("pf-engine-{}", fastrand::u32(..)));
        std::fs::create_dir_all(dir.join("Moonsweepers_Data")).unwrap();
        std::fs::write(
            dir.join("Moonsweepers_Data/app.info"),
            "Moon Studio\nMoon Sweepers",
        )
        .unwrap();
        assert_eq!(
            detect_engine(&dir, "moonsweepers.exe"),
            Engine::Unity {
                company: "Moon Studio".into(),
                product: "Moon Sweepers".into()
            }
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn skips_saves_logs_and_accounts() {
        assert!(!settings_like(Path::new("/x/SaveGame01.json")));
        assert!(!settings_like(Path::new("/x/Player.log")));
        assert!(!settings_like(Path::new("/x/account.json")));
        assert!(settings_like(Path::new("/x/PROFSAVE_profile")));
        assert!(settings_like(Path::new("/x/GameUserSettings.ini")));
        assert!(settings_like(Path::new("/x/settings.json")));
    }

    #[test]
    fn unity_packed_settings_and_ids() {
        let json = r#"{"Graphics":{"Quality":"Ultra","VSync":false,"TargetFPS":-1,"Shadows":"High"},"PlayerName":"PlayerOne"}"#;
        let hex: Vec<String> = json
            .bytes()
            .chain(std::iter::once(0))
            .map(|b| format!("{b:02x}"))
            .collect();
        let reg = format!(
            "[Software\\\\DragDropGames\\\\MoonSweepers] 1\n\"MoonCleaners.UserSettings.v1_h1\"=hex:{}\n\"MoonCleaners.Achievements.76561190000000001_h2\"=hex:61,62,00\n",
            hex.join(",")
        );
        let v = read_wine_reg(&reg, "Software\\DragDropGames\\MoonSweepers").unwrap();
        assert_eq!(v.len(), 2);
        let dir = std::env::temp_dir().join(format!("pf-unity-{}", fastrand::u32(..)));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("user.reg"), &reg).unwrap();
        let loc = Located {
            name: "Moon Sweepers".into(),
            exe: "moonsweepers.exe".into(),
            prefix: Some(dir.clone()),
            ..Default::default()
        };
        let (_, map, text) = unity_prefs("DragDropGames", "MoonSweepers", &loc).unwrap();
        assert_eq!(
            map.get("MoonCleaners.UserSettings.v1/Graphics/Shadows")
                .map(|s| s.as_str()),
            Some("High")
        );
        assert_eq!(
            map.get("MoonCleaners.UserSettings.v1/Graphics/TargetFPS")
                .map(|s| s.as_str()),
            Some("-1")
        );
        assert!(!text.contains("PlayerOne"), "player name removed");
        assert!(
            !text.contains("76561190000000001"),
            "Steam ID removed: {text}"
        );
        let hits = recognise(&map);
        assert!(
            hits.iter().any(|h| h.kind == Kind::FpsLimit)
                && hits.iter().any(|h| h.kind == Kind::Shadows)
        );
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            crate::util::scrub_steam_ids("Steam/userdata/123456789/730 and 76561190000000001"),
            "Steam/userdata/<id>/730 and <steam id>"
        );
        let r = crate::report::redact_private_lines(
            &(0..12)
                .map(|i| format!("  \"name\": \"Opt{i}\","))
                .collect::<Vec<_>>()
                .join("\n"),
        );
        assert!(
            !r.contains("removed"),
            "setting names kept in structured files"
        );
        assert!(
            crate::report::redact_private_lines("\"name\"\t\t\"PlayerOne\"").contains("removed")
        );
    }
}
