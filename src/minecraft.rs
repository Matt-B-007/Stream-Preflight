//! Minecraft (Java and Bedrock) and modpacks from any launcher: the
//! official launcher, CurseForge, Prism, MultiMC, Modrinth, ATLauncher and
//! GDLauncher. Read-only. The running game is found from its launch details
//! (the game folder Java was started with); only that folder is read.
//! Mods are recognised by file name only.

use crate::games::{CurrentSetting, FileResult, Finding, GameResult, TunableState};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub struct Instance {
    /// The game folder (holds options.txt, mods/, config/).
    pub dir: PathBuf,
    /// "All the Mods 10", or "" for plain Minecraft.
    pub pack: String,
    pub launcher: String,
    pub bedrock: bool,
}

/// Game folders of Minecraft instances running now, from Java's launch
/// details (`--gameDir <folder>`, or its working folder).
pub fn running(sys: &mut Option<sysinfo::System>) -> Vec<PathBuf> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, UpdateKind};
    let Some(sys) = sys.as_mut() else { return Vec::new() };
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_cmd(UpdateKind::OnlyIfNotSet).with_cwd(UpdateKind::OnlyIfNotSet).with_exe(UpdateKind::OnlyIfNotSet));
    let mut out = Vec::new();
    for p in sys.processes().values() {
        let name = p.name().to_string_lossy().to_lowercase();
        if name == "minecraft.windows.exe" {
            out.push(PathBuf::from("bedrock"));
            continue;
        }
        if !(name.starts_with("java") || name.starts_with("javaw")) {
            continue;
        }
        let args: Vec<String> = p.cmd().iter().map(|a| a.to_string_lossy().to_string()).collect();
        let joined = args.join(" ").to_lowercase();
        if !(joined.contains("net.minecraft") || joined.contains("minecraft") || joined.contains("cpw.mods") || joined.contains("fabric") || joined.contains("quilt")) {
            continue;
        }
        let dir = args.iter().position(|a| a == "--gameDir").and_then(|i| args.get(i + 1)).map(PathBuf::from).or_else(|| p.cwd().map(|c| c.to_path_buf()));
        if let Some(d) = dir {
            if d.join("options.txt").is_file() || d.join("mods").is_dir() || d.join("saves").is_dir() {
                if !out.contains(&d) {
                    out.push(d);
                }
            }
        }
    }
    out
}

/// Every instance folder from the launchers Preflight knows.
pub fn all_instances() -> Vec<Instance> {
    let mut out = Vec::new();
    let Some(home) = crate::util::home_dir() else { return out };
    let mut add = |dir: PathBuf, pack: String, launcher: &str| {
        if dir.join("options.txt").is_file() && !out.iter().any(|i: &Instance| i.dir == dir) {
            out.push(Instance { dir, pack, launcher: launcher.into(), bedrock: false });
        }
    };
    // Official launcher.
    #[cfg(windows)]
    if let Ok(a) = std::env::var("APPDATA") {
        add(PathBuf::from(a).join(".minecraft"), String::new(), "Minecraft Launcher");
    }
    #[cfg(not(windows))]
    {
        add(home.join(".minecraft"), String::new(), "Minecraft Launcher");
        add(home.join(".var/app/com.mojang.Minecraft/.minecraft"), String::new(), "Minecraft Launcher");
    }
    // Launchers that keep one folder per instance: (folder of instances, sub-folder, launcher).
    #[allow(unused_mut)]
    let mut roots: Vec<(PathBuf, &[&str], &str)> = vec![
        (home.join("curseforge/minecraft/Instances"), &[""], "CurseForge"),
        (home.join("Documents/curseforge/minecraft/Instances"), &[""], "CurseForge"),
        (home.join(".local/share/PrismLauncher/instances"), &[".minecraft", "minecraft"], "Prism Launcher"),
        (home.join(".var/app/org.prismlauncher.PrismLauncher/data/PrismLauncher/instances"), &[".minecraft", "minecraft"], "Prism Launcher"),
        (home.join(".local/share/multimc/instances"), &[".minecraft", "minecraft"], "MultiMC"),
        (home.join(".local/share/ModrinthApp/profiles"), &[""], "Modrinth"),
        (home.join(".local/share/com.modrinth.theseus/profiles"), &[""], "Modrinth"),
        (home.join(".local/share/ATLauncher/instances"), &[""], "ATLauncher"),
        (home.join("ATLauncher/instances"), &[""], "ATLauncher"),
        (home.join(".config/gdlauncher_carbon/data/instances"), &["instance"], "GDLauncher"),
    ];
    #[cfg(windows)]
    if let Ok(a) = std::env::var("APPDATA") {
        let a = PathBuf::from(a);
        roots.push((a.join("PrismLauncher/instances"), &[".minecraft", "minecraft"], "Prism Launcher"));
        roots.push((a.join("ModrinthApp/profiles"), &[""], "Modrinth"));
        roots.push((a.join("com.modrinth.theseus/profiles"), &[""], "Modrinth"));
        roots.push((a.join("gdlauncher_carbon/data/instances"), &["instance"], "GDLauncher"));
    }
    for (root, subs, launcher) in roots {
        let Ok(rd) = std::fs::read_dir(&root) else { continue };
        for e in rd.flatten().take(200) {
            let name = e.file_name().to_string_lossy().to_string();
            for sub in subs.iter() {
                let dir = if sub.is_empty() { e.path() } else { e.path().join(sub) };
                add(dir, pack_name(&e.path()).unwrap_or_else(|| name.clone()), launcher);
            }
        }
    }
    // Bedrock (Windows).
    #[cfg(windows)]
    for d in bedrock_dirs() {
        if d.join("options.txt").is_file() {
            out.push(Instance { dir: d, pack: String::new(), launcher: "Bedrock".into(), bedrock: true });
        }
    }
    out
}

#[cfg(windows)]
fn bedrock_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Ok(l) = std::env::var("LOCALAPPDATA") {
        v.push(PathBuf::from(l).join("Packages/Microsoft.MinecraftUWP_8wekyb3d8bbwe/LocalState/games/com.mojang/minecraftpe"));
    }
    if let Ok(a) = std::env::var("APPDATA") {
        if let Ok(rd) = std::fs::read_dir(PathBuf::from(a).join("Minecraft Bedrock/Users")) {
            for e in rd.flatten() {
                v.push(e.path().join("games/com.mojang/minecraftpe"));
            }
        }
    }
    v
}

/// A modpack's own name from its launcher's files.
fn pack_name(instance: &Path) -> Option<String> {
    // CurseForge
    if let Ok(t) = std::fs::read_to_string(instance.join("minecraftinstance.json")) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&t) {
            if let Some(n) = v["name"].as_str().filter(|s| !s.is_empty()) {
                return Some(n.to_string());
            }
        }
    }
    // Prism / MultiMC
    if let Ok(t) = std::fs::read_to_string(instance.join("instance.cfg")) {
        if let Some(n) = t.lines().find_map(|l| l.strip_prefix("name=")).filter(|s| !s.is_empty()) {
            return Some(n.trim().to_string());
        }
    }
    // Modrinth (profile.json)
    if let Ok(t) = std::fs::read_to_string(instance.join("profile.json")) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&t) {
            if let Some(n) = v["metadata"]["name"].as_str().or(v["name"].as_str()) {
                return Some(n.to_string());
            }
        }
    }
    None
}

fn instance_for(dir: &Path, all: &[Instance]) -> Instance {
    if dir == Path::new("bedrock") {
        return all.iter().find(|i| i.bedrock).cloned().unwrap_or(Instance { dir: dir.into(), pack: String::new(), launcher: "Bedrock".into(), bedrock: true });
    }
    if let Some(i) = all.iter().find(|i| i.dir == dir) {
        return i.clone();
    }
    // Not from a known launcher: name it from its folder.
    let parent = if dir.file_name().map(|n| n.to_string_lossy().contains("minecraft")).unwrap_or(false) { dir.parent().unwrap_or(dir) } else { dir };
    let pack = pack_name(parent).or_else(|| {
        let n = parent.file_name()?.to_string_lossy().to_string();
        (!n.starts_with('.')).then_some(n)
    });
    Instance { dir: dir.into(), pack: pack.unwrap_or_default(), launcher: String::new(), bedrock: false }
}

#[derive(Clone, Debug, Default)]
pub struct Mods {
    pub sodium: bool,
    pub iris: bool,
    pub optifine: bool,
    pub distant_horizons: bool,
    pub count: usize,
}

/// Mods by file name only.
pub fn mods_of(dir: &Path) -> Mods {
    let mut m = Mods::default();
    let Ok(rd) = std::fs::read_dir(dir.join("mods")) else { return m };
    for e in rd.flatten() {
        let n = e.file_name().to_string_lossy().to_lowercase();
        if !n.ends_with(".jar") {
            continue;
        }
        m.count += 1;
        m.sodium |= n.contains("sodium") || n.contains("embeddium") || n.contains("rubidium");
        m.iris |= n.contains("iris") || n.contains("oculus");
        m.optifine |= n.contains("optifine");
        m.distant_horizons |= n.contains("distanthorizons") || n.contains("distant-horizons") || n.contains("distant_horizons");
    }
    m
}

fn read_kv_file(p: &Path, sep: char) -> Option<BTreeMap<String, String>> {
    let t = std::fs::read_to_string(p).ok()?;
    let mut m = BTreeMap::new();
    for l in t.lines() {
        let l = l.trim();
        if l.starts_with('#') || l.starts_with('[') {
            continue;
        }
        if let Some((k, v)) = l.split_once(sep) {
            m.insert(k.trim().to_string(), v.trim().trim_matches('"').to_string());
        }
    }
    Some(m)
}

fn file_res(p: &Path, format: &str, count: usize) -> FileResult {
    FileResult {
        shown_path: crate::util::scrub(&p.to_string_lossy()),
        found: true,
        format: format.into(),
        read_ok: true,
        settings_count: count,
        modified: std::fs::metadata(p).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64),
        real_path: Some(p.to_path_buf()),
        ..Default::default()
    }
}

fn num(v: Option<&String>) -> Option<f64> {
    v.and_then(|s| s.trim().parse::<f64>().ok())
}

/// Reads one instance into a game result.
pub fn read(inst: &Instance, running: bool) -> GameResult {
    let name = if inst.bedrock {
        "Minecraft (Bedrock)".to_string()
    } else if inst.pack.is_empty() {
        "Minecraft".to_string()
    } else {
        format!("Minecraft · {}", inst.pack)
    };
    let video = format!("{name} › Options › Video Settings");
    let mut g = GameResult {
        id: format!("minecraft:{}", crate::autoread::norm(&inst.pack)),
        name: name.clone(),
        load: "mixed".into(),
        verified: "Read automatically (Minecraft), not hand-checked yet".into(),
        installed: true,
        install_hint: inst.launcher.clone(),
        running,
        processes: if inst.bedrock { vec!["minecraft.windows.exe".into()] } else { vec!["javaw.exe".into(), "java".into()] },
        has_fps_limit: true,
        api: Some(if inst.bedrock { "dx12".into() } else { "opengl".into() }),
        launcher: inst.launcher.clone(),
        motion: "medium".into(),
        auto: Some("Minecraft".into()),
        ..Default::default()
    };
    let opts_path = inst.dir.join("options.txt");
    let Some(o) = read_kv_file(&opts_path, ':') else {
        g.read_gap = Some("Preflight couldn't find this Minecraft's options.txt yet. Start the game once and change a setting.".into());
        return g;
    };
    g.files.push(file_res(&opts_path, "lines", o.len()));
    let mut all: BTreeMap<String, String> = o.clone();
    let mut used = 0usize;
    let mut cur = |setting: &str, value: String, effect: &str, flagged: bool, change_to: Option<String>, menu: &str| {
        g.current.push(CurrentSetting { rule_id: format!("mc_{}", crate::autoread::norm(setting)), setting: setting.into(), effect: effect.into(), menu: menu.into(), value, flagged, change_to, gain: None, cost: None });
    };
    let finding = |effect: &str, label: &str, key: &str, value: &str, to: Option<&str>, gain: Option<&str>, cost: Option<&str>, menu: &str| Finding {
        rule_id: format!("mc_{}", crate::autoread::norm(label)),
        effect: effect.into(),
        label: label.into(),
        menu: menu.into(),
        key: key.into(),
        value: value.into(),
        change_to: to.map(|s| s.to_string()),
        gain: gain.map(|s| s.to_string()),
        cost: cost.map(|s| s.to_string()),
    };

    if inst.bedrock {
        // Bedrock: gfx_max_framerate (0 = unlimited), gfx_vsync, gfx_viewdistance (blocks).
        if let Some(f) = num(o.get("gfx_max_framerate")) {
            used += 1;
            if f <= 0.0 {
                g.findings.push(finding("uncapped_fps", "Max framerate is unlimited", "gfx_max_framerate", "0", None, None, None, &video));
                cur("FPS limit", "Off (unlimited)".into(), "uncapped_fps", true, None, &video);
            } else {
                g.fps_cap = Some(f as u32);
                cur("FPS limit", format!("{f:.0}"), "uncapped_fps", false, None, &video);
            }
        }
        if let Some(v) = num(o.get("gfx_viewdistance")) {
            used += 1;
            let chunks = (v / 16.0).round() as u32;
            let to = (chunks > 16).then(|| "16 chunks".to_string());
            g.tunables.push(TunableState { id: "mc_render".into(), setting: "Render distance".into(), menu: video.clone(), now: format!("{chunks} chunks"), change_to: to, gain: "Large GPU and CPU saving".into(), cost: "You see less far".into() });
        }
    } else {
        // Java: maxFps 260 = Unlimited.
        if let Some(f) = num(o.get("maxFps")) {
            used += 1;
            if f >= 260.0 {
                g.findings.push(finding("uncapped_fps", "Max Framerate is Unlimited", "maxFps", "260", None, None, None, &video));
                cur("FPS limit", "Off (unlimited)".into(), "uncapped_fps", true, None, &video);
            } else {
                g.fps_cap = Some(f as u32);
                cur("FPS limit", format!("{f:.0}"), "uncapped_fps", false, None, &video);
            }
        }
        if let Some(v) = o.get("enableVsync") {
            used += 1;
            let on = v == "true";
            if on {
                g.findings.push(finding("vsync_on", "VSync is on", "enableVsync", v, None, None, None, &video));
            }
            cur("V-Sync", if on { "On".into() } else { "Off".into() }, "vsync_on", false, None, &video);
        }
        if let Some(r) = num(o.get("renderDistance")) {
            used += 1;
            let to = (r > 16.0).then(|| "12 to 16 chunks".to_string());
            g.tunables.push(TunableState { id: "mc_render".into(), setting: "Render distance".into(), menu: video.clone(), now: format!("{r:.0} chunks"), change_to: to, gain: "Large GPU and CPU saving".into(), cost: "You see less far".into() });
        }
        if let Some(sd) = num(o.get("simulationDistance")) {
            used += 1;
            let to = (sd > 12.0).then(|| "8 to 12 chunks".to_string());
            g.tunables.push(TunableState { id: "mc_sim".into(), setting: "Simulation distance".into(), menu: video.clone(), now: format!("{sd:.0} chunks"), change_to: to, gain: "Big CPU saving (farms and mobs far away stop ticking)".into(), cost: "Far-away farms and mobs pause".into() });
        }
        if let Some(gm) = o.get("graphicsMode") {
            used += 1;
            let label = match gm.trim_matches('"') {
                "0" | "fast" => "Fast",
                "1" | "fancy" => "Fancy",
                "2" | "fabulous" => "Fabulous!",
                x => x,
            };
            let fab = label == "Fabulous!";
            g.tunables.push(TunableState { id: "mc_graphics".into(), setting: "Graphics".into(), menu: video.clone(), now: label.into(), change_to: fab.then(|| "Fancy".into()), gain: "Good GPU saving".into(), cost: "Slightly simpler transparency".into() });
        }
        if let Some(b) = num(o.get("biomeBlendRadius")) {
            used += 1;
            if b > 2.0 {
                g.tunables.push(TunableState { id: "mc_blend".into(), setting: "Biome blend".into(), menu: video.clone(), now: format!("{b:.0}"), change_to: Some("2".into()), gain: "Faster chunk loading (CPU)".into(), cost: "Harder colour edges between biomes".into() });
            }
        }
    }

    // Mods: shaders and Distant Horizons are the heavy ones.
    let mods = mods_of(&inst.dir);
    let cfg = inst.dir.join("config");
    let mut shader: Option<String> = None;
    for f in ["iris.properties", "oculus.properties"] {
        if let Some(m) = read_kv_file(&cfg.join(f), '=') {
            g.files.push(file_res(&cfg.join(f), "ini", m.len()));
            used += 1;
            if m.get("enableShaders").map(|v| v == "true").unwrap_or(false) {
                shader = Some(m.get("shaderPack").cloned().unwrap_or_else(|| "a shader pack".into()));
            }
            all.extend(m.into_iter().map(|(k, v)| (format!("{f}/{k}"), v)));
        }
    }
    if let Some(m) = read_kv_file(&inst.dir.join("optionsshaders.txt"), '=') {
        g.files.push(file_res(&inst.dir.join("optionsshaders.txt"), "ini", m.len()));
        used += 1;
        if let Some(p) = m.get("shaderPack").filter(|p| !p.is_empty() && p.as_str() != "OFF" && p.as_str() != "(internal)") {
            shader = Some(p.clone());
        }
    }
    if let Some(sp) = &shader {
        let pack = sp.trim_end_matches(".zip").to_string();
        let mut f = finding("heavy", &format!("Shaders are on ({pack})"), "enableShaders", "true", Some("Off, or a lighter shader pack or preset"), Some("Usually the biggest GPU saving in Minecraft"), Some("No shader lighting and shadows"), &format!("{name} › Options › Video Settings › Shader Packs"));
        f.rule_id = "mc_shaders".into();
        g.findings.push(f);
        cur("Shaders", pack, "heavy", true, Some("Off, or a lighter pack".into()), &format!("{name} › Options › Video Settings › Shader Packs"));
    } else if mods.iris || mods.optifine {
        cur("Shaders", "Off".into(), "heavy", false, None, &format!("{name} › Options › Video Settings › Shader Packs"));
    }
    if mods.distant_horizons {
        let dh = cfg.join("DistantHorizons.toml");
        let radius = read_kv_file(&dh, '=').and_then(|m| {
            let r = m.get("lodChunkRenderDistanceRadius").or_else(|| m.get("lodChunkRenderDistance")).cloned();
            g.files.push(file_res(&dh, "toml", m.len()));
            r
        });
        used += 1;
        let now = radius.clone().map(|r| format!("On · {r} chunks")).unwrap_or_else(|| "On".into());
        let big = radius.as_deref().and_then(|r| r.parse::<u32>().ok()).map(|r| r > 128).unwrap_or(true);
        g.tunables.push(TunableState { id: "mc_dh".into(), setting: "Distant Horizons".into(), menu: format!("{name} › Distant Horizons settings (the DH button in Video Settings)"), now, change_to: big.then(|| "128 chunks or less".into()), gain: "Large GPU saving".into(), cost: "Distant land shows less far".into() });
    }
    if mods.count > 0 {
        let mut notes = vec![format!("{} mods", mods.count)];
        if mods.sodium {
            notes.push("Sodium (a performance mod: good)".into());
        }
        g.platform_note = Some(notes.join(" · "));
    }
    g.display_res = None;
    g.settings_read = all.len();
    g.settings_used = used;
    g
}

/// Minecraft instances to show: the running ones, else the last played.
pub fn scan(sys: &mut Option<sysinfo::System>) -> Vec<GameResult> {
    let running_dirs = running(sys);
    let all = all_instances();
    let mut out = Vec::new();
    for d in &running_dirs {
        let inst = instance_for(d, &all);
        out.push(read(&inst, true));
    }
    if out.is_empty() {
        // Not running: the instance whose options.txt changed last.
        if let Some(i) = all.iter().max_by_key(|i| std::fs::metadata(i.dir.join("options.txt")).and_then(|m| m.modified()).ok()) {
            out.push(read(i, false));
        }
    }
    for g in &out {
        crate::logger::info_changed(&format!("mc:{}", g.id), format!("{}: {} settings, {} used{}", g.name, g.settings_read, g.settings_used, g.platform_note.as_ref().map(|n| format!(" · {n}")).unwrap_or_default()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curseforge_modpack() {
        let root = std::env::temp_dir().join(format!("pf-mc-{}", fastrand::u32(..)));
        let inst = root.join("curseforge/minecraft/Instances/ATM10");
        std::fs::create_dir_all(inst.join("mods")).unwrap();
        std::fs::create_dir_all(inst.join("config")).unwrap();
        std::fs::write(inst.join("minecraftinstance.json"), r#"{"name":"All the Mods 10","gameVersion":"1.21.1"}"#).unwrap();
        std::fs::write(inst.join("options.txt"), "version:3955\nmaxFps:260\nenableVsync:false\nrenderDistance:24\nsimulationDistance:16\ngraphicsMode:2\nbiomeBlendRadius:5\nlastServer:play.example.net\n").unwrap();
        for m in ["sodium-fabric-0.6.jar", "iris-fabric-1.8.jar", "DistantHorizons-2.3.jar", "create-6.0.jar"] {
            std::fs::write(inst.join("mods").join(m), "").unwrap();
        }
        std::fs::write(inst.join("config/iris.properties"), "enableShaders=true\nshaderPack=ComplementaryReimagined_r5.4.zip\n").unwrap();
        std::fs::write(inst.join("config/DistantHorizons.toml"), "[client.advanced.graphics.quality]\n\tlodChunkRenderDistanceRadius = 256\n").unwrap();
        let i = Instance { dir: inst.clone(), pack: pack_name(&inst).unwrap(), launcher: "CurseForge".into(), bedrock: false };
        assert_eq!(i.pack, "All the Mods 10");
        let g = read(&i, true);
        assert_eq!(g.name, "Minecraft · All the Mods 10");
        assert!(g.findings.iter().any(|f| f.effect == "uncapped_fps"));
        assert!(g.findings.iter().any(|f| f.rule_id == "mc_shaders" && f.label.contains("ComplementaryReimagined")));
        let t = |id: &str| g.tunables.iter().find(|t| t.id == id).unwrap().clone();
        assert_eq!(t("mc_render").change_to.as_deref(), Some("12 to 16 chunks"));
        assert_eq!(t("mc_sim").change_to.as_deref(), Some("8 to 12 chunks"));
        assert_eq!(t("mc_graphics").now, "Fabulous!");
        assert_eq!(t("mc_dh").now, "On · 256 chunks");
        assert!(g.platform_note.as_deref().unwrap().contains("Sodium"));
        let mods = mods_of(&inst);
        assert!(mods.sodium && mods.iris && mods.distant_horizons && mods.count == 4);
        let _ = std::fs::remove_dir_all(&root);
    }
}
