//! Hardware scan and live system sampling. Everything here reads from the
//! operating system or official vendor libraries. Nothing touches a game.

use serde::Serialize;
use sysinfo::{Components, ProcessesToUpdate, System};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Vendor {
    Nvidia,
    Amd,
    Intel,
    Other,
}

impl Vendor {
    fn from_id(id: u32) -> Self {
        match id {
            0x10de => Vendor::Nvidia,
            0x1002 | 0x1022 => Vendor::Amd,
            0x8086 => Vendor::Intel,
            _ => Vendor::Other,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Vendor::Nvidia => "NVIDIA",
            Vendor::Amd => "AMD",
            Vendor::Intel => "Intel",
            Vendor::Other => "Other",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Gpu {
    pub name: String,
    pub vendor: Vendor,
    pub vram_mb: Option<u64>,
    pub driver: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct HardwareInfo {
    pub os: String,
    pub os_family: &'static str,
    pub kernel: String,
    pub session: String,
    pub cpu: String,
    pub cpu_physical_cores: Option<usize>,
    pub cpu_threads: usize,
    pub ram_gb: f64,
    pub gpus: Vec<Gpu>,
    /// Hardware encoders this PC most likely has, best first.
    pub encoders: Vec<String>,
    pub notes: Vec<String>,
}

impl HardwareInfo {
    pub fn main_gpu(&self) -> Option<&Gpu> {
        self.gpus
            .iter()
            .find(|g| g.vendor == Vendor::Nvidia)
            .or_else(|| self.gpus.iter().find(|g| g.vendor == Vendor::Amd))
            .or_else(|| self.gpus.first())
    }
    pub fn has_hw_encoder(&self) -> bool {
        !self.encoders.is_empty()
    }
}

pub fn scan() -> HardwareInfo {
    crate::logger::activity("hardware scan");
    let (cpu, threads, ram_gb) = match System::new() {
        Ok(mut sys) => {
            sys.refresh_cpu_all();
            sys.refresh_memory();
            (
                sys.cpus().first().map(|c| c.brand().trim().to_string()).unwrap_or_else(|| "Unknown CPU".into()),
                sys.cpus().len(),
                sys.total_memory() as f64 / 1_073_741_824.0,
            )
        }
        Err(e) => {
            log::warn!("System information unavailable: {e}");
            ("Unknown CPU".to_string(), 0, 0.0)
        }
    };
    let physical = System::physical_core_count().ok();
    let os = System::long_os_version().or_else(|_| System::name()).unwrap_or_else(|_| "Unknown OS".into());
    let kernel = System::kernel_version().unwrap_or_default();
    let os_family = if cfg!(windows) { "Windows" } else { "Linux" };
    let session = session_type();

    let mut notes = Vec::new();
    let mut gpus = detect_gpus(&mut notes);
    gpus.dedup_by(|a, b| a.name == b.name);
    let mut encoders = Vec::new();
    for g in &gpus {
        let e = match (g.vendor, cfg!(windows)) {
            (Vendor::Nvidia, _) => "NVIDIA NVENC",
            (Vendor::Amd, true) => "AMD AMF",
            (Vendor::Amd, false) => "VA-API (AMD)",
            (Vendor::Intel, true) => "Intel Quick Sync",
            (Vendor::Intel, false) => "VA-API (Intel)",
            _ => continue,
        };
        if !encoders.iter().any(|x: &String| x == e) {
            encoders.push(e.to_string());
        }
    }
    let info = HardwareInfo {
        os,
        os_family,
        kernel,
        session,
        cpu,
        cpu_physical_cores: physical,
        cpu_threads: threads,
        ram_gb,
        gpus,
        encoders,
        notes,
    };
    log::info!(
        "Hardware: {} | {} threads | {:.0} GB RAM | GPUs: {} | encoders: {}",
        info.cpu,
        info.cpu_threads,
        info.ram_gb,
        info.gpus.iter().map(|g| g.name.as_str()).collect::<Vec<_>>().join(", "),
        info.encoders.join(", ")
    );
    info
}

fn session_type() -> String {
    if cfg!(windows) {
        return "Windows desktop".into();
    }
    let t = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();
    let d = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    match (t.is_empty(), d.is_empty()) {
        (false, false) => format!("{t} / {d}"),
        (false, true) => t,
        (true, false) => d,
        _ => "unknown".into(),
    }
}

fn detect_gpus(notes: &mut Vec<String>) -> Vec<Gpu> {
    let mut out = Vec::new();
    // NVIDIA's own library gives exact names and driver versions.
    match nvml_wrapper::Nvml::init() {
        Ok(nvml) => {
            let driver = nvml.sys_driver_version().ok();
            if let Ok(n) = nvml.device_count() {
                for i in 0..n {
                    if let Ok(d) = nvml.device_by_index(i) {
                        out.push(Gpu {
                            name: d.name().unwrap_or_else(|_| "NVIDIA GPU".into()),
                            vendor: Vendor::Nvidia,
                            vram_mb: d.memory_info().ok().map(|m| m.total / 1_048_576),
                            driver: driver.clone(),
                        });
                    }
                }
            }
        }
        Err(e) => log::debug!("NVML not available: {e}"),
    }
    for g in platform_gpus(notes) {
        if g.vendor == Vendor::Nvidia && out.iter().any(|x| x.vendor == Vendor::Nvidia) {
            continue;
        }
        out.push(g);
    }
    if out.is_empty() {
        notes.push("No graphics card could be detected.".into());
    }
    out
}

#[cfg(windows)]
fn platform_gpus(_notes: &mut Vec<String>) -> Vec<Gpu> {
    crate::winsys::dxgi_adapters()
        .into_iter()
        .map(|(name, vendor_id, vram)| Gpu {
            name,
            vendor: Vendor::from_id(vendor_id),
            vram_mb: Some(vram / 1_048_576),
            driver: None,
        })
        .collect()
}

#[cfg(not(windows))]
fn platform_gpus(notes: &mut Vec<String>) -> Vec<Gpu> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir("/sys/class/drm") else {
        notes.push("Could not read /sys/class/drm".into());
        return out;
    };
    let mut cards: Vec<_> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let n = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            n.starts_with("card") && !n.contains('-')
        })
        .collect();
    cards.sort();
    for c in cards {
        let dev = c.join("device");
        let read_hex = |f: &str| -> Option<u32> {
            let s = std::fs::read_to_string(dev.join(f)).ok()?;
            u32::from_str_radix(s.trim().trim_start_matches("0x"), 16).ok()
        };
        let Some(vid) = read_hex("vendor") else { continue };
        let did = read_hex("device").unwrap_or(0);
        let driver = std::fs::read_link(dev.join("driver"))
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()));
        let vram_mb = std::fs::read_to_string(dev.join("mem_info_vram_total"))
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .map(|b| b / 1_048_576);
        let vendor = Vendor::from_id(vid);
        let name = lspci_name(vid, did).unwrap_or_else(|| format!("{} GPU ({:04x}:{:04x})", vendor.name(), vid, did));
        out.push(Gpu { name, vendor, vram_mb, driver });
    }
    out
}

#[cfg(not(windows))]
fn lspci_name(vid: u32, did: u32) -> Option<String> {
    let o = std::process::Command::new("lspci")
        .args(["-mm", "-d", &format!("{vid:04x}:{did:04x}")])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&o.stdout);
    let line = s.lines().next()?;
    // Format: slot "class" "vendor" "device" ...
    let parts: Vec<&str> = line.split('"').collect();
    let vendor = parts.get(3)?.trim();
    let device = parts.get(5)?.trim();
    if device.is_empty() {
        return None;
    }
    let v = vendor
        .replace("Advanced Micro Devices, Inc. [AMD/ATI]", "AMD")
        .replace("NVIDIA Corporation", "NVIDIA")
        .replace("Intel Corporation", "Intel");
    Some(format!("{v} {device}"))
}

// ---------------------------------------------------------------------------
// Live sampling during a test

#[derive(Clone, Debug, Default, Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Sample {
    pub t: f32,
    pub cpu: f32,
    pub gpu: Option<f32>,
    pub gpu_temp: Option<f32>,
    pub cpu_temp: Option<f32>,
    pub gpu_throttle: Option<String>,
    /// How busy the video encoder chip is (NVENC etc.), in percent.
    pub enc: Option<f32>,
}

/// Number of NVENC encoder chips, from NVIDIA's encode support matrix.
/// Cards with two or more handle a stream plus a separate recording better.
pub fn nvenc_chips(gpu_name: &str) -> Option<u8> {
    let n = gpu_name.to_lowercase();
    if !n.contains("nvidia") && !n.contains("geforce") && !n.contains("rtx") && !n.contains("gtx") {
        return None;
    }
    let has = |m: &str| n.contains(m);
    if has("5090") {
        Some(3)
    } else if has("5080") || has("5070 ti") || has("4090") || has("4080") || has("4070 ti") {
        Some(2)
    } else {
        Some(1)
    }
}

pub struct Monitor {
    sys: Option<System>,
    comps: Option<Components>,
    nvml: Option<nvml_wrapper::Nvml>,
    #[cfg(not(windows))]
    amd_busy: Option<std::path::PathBuf>,
    #[cfg(not(windows))]
    amd_temp: Option<std::path::PathBuf>,
    #[cfg(windows)]
    pdh: Option<crate::winsys::GpuCounter>,
    #[cfg(windows)]
    pdh_enc: Option<crate::winsys::GpuCounter>,
    pub gpu_source: String,
}

impl Monitor {
    pub fn new() -> Self {
        let mut sys = System::new().ok();
        if let Some(s) = sys.as_mut() {
            s.refresh_cpu_usage();
        }
        let comps = Components::new_with_refreshed_list().ok();
        let nvml = nvml_wrapper::Nvml::init().ok();
        let mut gpu_source = String::new();
        if nvml.is_some() {
            gpu_source = "NVIDIA NVML".into();
        }
        #[cfg(windows)]
        let pdh = {
            let c = crate::winsys::GpuCounter::new();
            if c.is_some() && gpu_source.is_empty() {
                gpu_source = "Windows performance counters".into();
            }
            c
        };
        #[cfg(not(windows))]
        let (amd_busy, amd_temp) = {
            let mut busy = None;
            let mut temp = None;
            if let Ok(rd) = std::fs::read_dir("/sys/class/drm") {
                for e in rd.flatten() {
                    let p = e.path().join("device/gpu_busy_percent");
                    if p.exists() && busy.is_none() {
                        busy = Some(p);
                        if let Ok(h) = std::fs::read_dir(e.path().join("device/hwmon")) {
                            for hh in h.flatten() {
                                let t = hh.path().join("temp1_input");
                                if t.exists() {
                                    temp = Some(t);
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            if busy.is_some() && gpu_source.is_empty() {
                gpu_source = "Linux amdgpu".into();
            }
            (busy, temp)
        };
        if gpu_source.is_empty() {
            gpu_source = "not available on this GPU yet".into();
        }
        Self {
            sys,
            comps,
            nvml,
            #[cfg(not(windows))]
            amd_busy,
            #[cfg(not(windows))]
            amd_temp,
            #[cfg(windows)]
            pdh,
            #[cfg(windows)]
            pdh_enc: crate::winsys::GpuCounter::encoder(),
            gpu_source,
        }
    }

    /// The program using the most graphics memory that isn't a desktop app:
    /// almost always the game. NVIDIA only (memory use isn't reported per
    /// program on Windows, so this returns None there).
    pub fn top_gpu_process(&self, sys: &mut Option<System>) -> Option<String> {
        let n = self.nvml.as_ref()?;
        let d = n.device_by_index(0).ok()?;
        let procs = d.running_graphics_processes().ok()?;
        let sy = sys.as_mut()?;
        refresh_procs(sy);
        let mut best: Option<(u64, String)> = None;
        for p in procs {
            let mem = match p.used_gpu_memory {
                nvml_wrapper::enums::device::UsedGpuMemory::Used(b) => b,
                _ => 0,
            };
            let Some(pr) = sy.process(sysinfo::Pid::from_u32(p.pid)) else { continue };
            let mut name = pr.name().to_string_lossy().to_lowercase();
            if let Some(c0) = pr.cmd().first() {
                let b = base_name(&c0.to_string_lossy());
                if b.ends_with(".exe") {
                    name = b;
                }
            }
            if !name.ends_with(".exe") {
                // A thread name (like "gamethread"): fall back to the program file.
                if let Some(e) = pr.exe().and_then(|e| e.file_name()) {
                    let en = e.to_string_lossy().to_lowercase();
                    if !en.contains("wine") && !en.contains("preloader") {
                        name = en;
                    }
                }
            }
            if NOT_GAMES.iter().any(|x| name.starts_with(x)) || name.contains("wine") || name.contains("preloader") {
                continue;
            }
            if best.as_ref().map(|b| mem > b.0).unwrap_or(true) {
                best = Some((mem, name));
            }
        }
        best.map(|b| b.1)
    }

    pub fn sample(&mut self, t: f32) -> Sample {
        let cpu = match self.sys.as_mut() {
            Some(s) => {
                s.refresh_cpu_usage();
                s.global_cpu_usage()
            }
            None => 0.0,
        };
        if let Some(c) = self.comps.as_mut() {
            c.refresh(false);
        }
        let cpu_temp = self
            .comps
            .iter()
            .flat_map(|c| c.iter())
            .filter(|c| {
                let l = c.label().to_lowercase();
                l.contains("package") || l.contains("tctl") || l.contains("tdie") || l.contains("cpu")
            })
            .filter_map(|c| c.temperature())
            .fold(None, |a: Option<f32>, b| Some(a.map_or(b, |x| x.max(b))));
        let mut s = Sample { t, cpu, cpu_temp, ..Default::default() };
        if let Some(n) = &self.nvml {
            if let Ok(d) = n.device_by_index(0) {
                s.gpu = d.utilization_rates().ok().map(|u| u.gpu as f32);
                s.enc = d.encoder_utilization().ok().map(|u| u.utilization as f32);
                s.gpu_temp = d
                    .temperature(nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu)
                    .ok()
                    .map(|v| v as f32);
                if let Ok(r) = d.current_throttle_reasons() {
                    use nvml_wrapper::bitmasks::device::ThrottleReasons as T;
                    if r.intersects(T::HW_THERMAL_SLOWDOWN | T::SW_THERMAL_SLOWDOWN | T::HW_SLOWDOWN) {
                        s.gpu_throttle = Some("thermal".into());
                    } else if r.intersects(T::SW_POWER_CAP | T::HW_POWER_BRAKE_SLOWDOWN) {
                        s.gpu_throttle = Some("power limit".into());
                    }
                }
            }
        }
        #[cfg(windows)]
        if s.gpu.is_none() {
            if let Some(p) = self.pdh.as_mut() {
                s.gpu = p.sample();
            }
        }
        #[cfg(windows)]
        if s.enc.is_none() {
            if let Some(p) = self.pdh_enc.as_mut() {
                s.enc = p.sample();
            }
        }
        #[cfg(not(windows))]
        if s.gpu.is_none() {
            if let Some(p) = &self.amd_busy {
                s.gpu = std::fs::read_to_string(p).ok().and_then(|v| v.trim().parse::<f32>().ok());
            }
            if let Some(p) = &self.amd_temp {
                s.gpu_temp = std::fs::read_to_string(p).ok().and_then(|v| v.trim().parse::<f32>().ok()).map(|m| m / 1000.0);
            }
        }
        // Developer test builds only: pretend the GPU is this busy (for testing without a GPU).
        #[cfg(debug_assertions)]
        if let Some(v) = std::env::var("PREFLIGHT_FAKE_GPU").ok().and_then(|v| v.parse::<f32>().ok()) {
            s.gpu = Some(v);
        }
        s
    }
}

// ---------------------------------------------------------------------------
// Programs that are known to affect streaming. Reported by name only.

const KNOWN_HOGS: &[(&str, &str)] = &[
    ("steam", "Steam (downloads and updates use bandwidth)"),
    ("steamwebhelper", "Steam"),
    ("epicgameslauncher", "Epic Games Launcher (downloads use bandwidth)"),
    ("battle.net", "Battle.net (downloads use bandwidth)"),
    ("onedrive", "OneDrive (cloud sync uses bandwidth)"),
    ("dropbox", "Dropbox (cloud sync uses bandwidth)"),
    ("googledrivefs", "Google Drive (cloud sync uses bandwidth)"),
    ("qbittorrent", "qBittorrent (uses upload bandwidth)"),
    ("utorrent", "uTorrent (uses upload bandwidth)"),
    ("transmission", "Transmission (uses upload bandwidth)"),
    ("medal", "Medal (another recorder)"),
    ("overwolf", "Overwolf (overlay/recorder)"),
    ("nvidia share", "NVIDIA overlay / instant replay"),
    ("nvcontainer", "NVIDIA overlay / instant replay"),
    ("gamebar", "Xbox Game Bar (recorder)"),
    ("gamebarftserver", "Xbox Game Bar (recorder)"),
    ("tiwork", "Windows Update worker"),
    ("tiworker", "Windows Update worker"),
    ("msmpeng", "Windows Defender scan"),
    ("packagekitd", "Linux software updates"),
    ("dnf", "Linux software updates"),
    ("pacman", "Linux software updates"),
];

#[derive(Clone, Debug, Default, Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Hog {
    pub label: String,
    pub cpu: f32,
}

pub fn new_system() -> Option<System> {
    System::new().ok()
}

/// Last part of a path, splitting on both / and \ (Proton shows Windows paths).
fn base_name(s: &str) -> String {
    s.rsplit(['/', '\\']).next().unwrap_or(s).to_lowercase()
}

/// Refreshes the process list including command lines (sysinfo leaves
/// command lines out by default, and Proton games need them: their short
/// process name is often a thread name like "GameThread").
fn refresh_procs(sys: &mut System) {
    use sysinfo::{ProcessRefreshKind, UpdateKind};
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_cpu().with_memory().with_exe(UpdateKind::OnlyIfNotSet).with_cmd(UpdateKind::OnlyIfNotSet),
    );
}

/// Running programs as (name, CPU %). Under Proton/Wine the short process
/// name can be cut off or be the loader's, so the program's own .exe name
/// from its command line is added too (with 0% CPU, so nothing is counted
/// twice).
pub fn running_process_names(sys: &mut Option<System>) -> Vec<(String, f32)> {
    let Some(sys) = sys.as_mut() else { return Vec::new() };
    refresh_procs(sys);
    let mut v = Vec::new();
    for p in sys.processes().values() {
        let name = p.name().to_string_lossy().to_lowercase();
        if let Some(c0) = p.cmd().first() {
            let b = base_name(&c0.to_string_lossy());
            if b != name && (b.ends_with(".exe") || b.len() > name.len()) {
                v.push((b, 0.0));
            }
        }
        v.push((name, p.cpu_usage()));
    }
    v
}

/// A running program that looks like a game, with where it lives.
#[derive(Clone, Debug, Default)]
pub struct GameProc {
    /// Lower-case program file name, e.g. "moonsweepers.exe".
    pub exe: String,
    /// The program file on disk (Wine paths turned into Linux paths).
    pub path: Option<std::path::PathBuf>,
    /// Steam's Proton folder for this game (STEAM_COMPAT_DATA_PATH), if any.
    pub compat: Option<std::path::PathBuf>,
    /// The Wine prefix (WINEPREFIX), for Heroic, Lutris and Bottles games.
    pub wineprefix: Option<std::path::PathBuf>,
    pub steam_appid: Option<u32>,
}

/// Turns a Wine path ("Z:\home\me\Games\x.exe") into a Linux path. C: paths
/// need the prefix, so they're resolved later.
fn wine_to_unix(s: &str, prefix: Option<&std::path::Path>) -> Option<std::path::PathBuf> {
    let t = s.replace('\\', "/");
    let low = t.to_lowercase();
    if low.starts_with("z:/") {
        return Some(std::path::PathBuf::from(&t[2..]));
    }
    if low.starts_with("c:/") {
        return prefix.map(|p| p.join("drive_c").join(&t[3..]));
    }
    if t.starts_with('/') {
        return Some(std::path::PathBuf::from(t));
    }
    None
}

/// Programs that look like games: their program file sits in a Steam
/// library, a Wine/Proton prefix, or a game launcher's folder. Only a few
/// launch details are read (program path and the Proton/Wine folder
/// variables); nothing inside the game is touched.
pub fn game_processes(sys: &mut Option<System>) -> Vec<GameProc> {
    use sysinfo::{ProcessRefreshKind, UpdateKind};
    let Some(sys) = sys.as_mut() else { return Vec::new() };
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet).with_cmd(UpdateKind::OnlyIfNotSet).with_environ(UpdateKind::OnlyIfNotSet),
    );
    let mut out: Vec<GameProc> = Vec::new();
    for p in sys.processes().values() {
        let mut g = GameProc::default();
        for e in p.environ() {
            let s = e.to_string_lossy();
            if let Some(v) = s.strip_prefix("STEAM_COMPAT_DATA_PATH=") {
                g.compat = Some(std::path::PathBuf::from(v));
            } else if let Some(v) = s.strip_prefix("WINEPREFIX=") {
                g.wineprefix = Some(std::path::PathBuf::from(v));
            } else if let Some(v) = s.strip_prefix("SteamAppId=") {
                g.steam_appid = v.parse().ok().filter(|n| *n > 0);
            }
        }
        let prefix = g.compat.as_ref().map(|c| c.join("pfx")).or_else(|| g.wineprefix.clone());
        let cmd0 = p.cmd().first().map(|c| c.to_string_lossy().to_string()).unwrap_or_default();
        let path = if cmd0.to_lowercase().ends_with(".exe") {
            wine_to_unix(&cmd0, prefix.as_deref()).or_else(|| Some(std::path::PathBuf::from(&cmd0)))
        } else {
            p.exe().map(|e| e.to_path_buf())
        };
        let Some(path) = path else { continue };
        let ps = path.to_string_lossy().replace('\\', "/").to_lowercase();
        let exe = base_name(&path.to_string_lossy());
        if exe.is_empty() || NOT_GAMES.iter().any(|x| exe.starts_with(x)) || exe.contains("wine") || exe.contains("preloader") || exe.contains("crashhandler") || exe.contains("crashreport") || exe.ends_with(".sh") {
            continue;
        }
        let looks_like_game = ps.contains("/steamapps/common/")
            || (prefix.is_some() && exe.ends_with(".exe") && !ps.contains("/windows/system32") && !ps.contains("/windows/syswow64"))
            || ps.contains("/heroic/") || ps.contains("/lutris/") || ps.contains("/bottles/") || ps.contains("/games/")
            || (cfg!(windows) && (ps.contains("/epic games/") || ps.contains("/xboxgames/") || ps.contains("/ea games/") || ps.contains("/ubisoft game launcher/games/") || ps.contains("/gog galaxy/games/")));
        let skip_dirs = ["/steamapps/common/proton", "/steamapps/common/steamlinuxruntime", "/steamapps/common/steamworks shared", "/steamapps/common/steam controller configs"];
        if !looks_like_game || skip_dirs.iter().any(|d| ps.contains(d)) || ps.contains("/windows/") && !ps.contains("/steamapps/") {
            continue;
        }
        g.exe = exe;
        g.path = Some(path);
        if !out.iter().any(|o| o.exe == g.exe) {
            out.push(g);
        }
    }
    out
}

/// Programs that use the GPU but aren't games.
const NOT_GAMES: &[&str] = &[
    "obs", "kwin", "gnome-shell", "xwayland", "xorg", "plasmashell", "steamwebhelper", "steam", "firefox", "chrome", "chromium", "brave", "discord",
    "electron", "stream-preflight", "code", "mutter", "hyprland", "sway", "kded", "spotify", "explorer.exe", "dwm.exe", "msedge",
];

pub fn find_hogs(procs: &[(String, f32)]) -> Vec<Hog> {
    let mut out: Vec<Hog> = Vec::new();
    for (name, cpu) in procs {
        // Steam always runs alongside Steam games; only flag it when it's
        // busy (downloading, updating or compiling shaders).
        if (name.starts_with("steam") || name.starts_with("steamwebhelper")) && *cpu < 25.0 {
            continue;
        }
        let base = name.trim_end_matches(".exe");
        if let Some((_, label)) = KNOWN_HOGS.iter().find(|(k, _)| base == *k || base.starts_with(k)) {
            if let Some(h) = out.iter_mut().find(|h| h.label == *label) {
                h.cpu += cpu;
            } else {
                out.push(Hog { label: label.to_string(), cpu: *cpu });
            }
        }
    }
    out
}

/// Other recorders and overlays matter for the "display capture" and
/// "another recorder is running" checks.
pub fn other_recorders(procs: &[(String, f32)]) -> Vec<String> {
    let mut v = Vec::new();
    for (n, _) in procs {
        let b = n.trim_end_matches(".exe");
        let l = match b {
            "medal" => "Medal",
            "overwolf" => "Overwolf",
            "nvidia share" => "NVIDIA overlay",
            "gamebar" | "gamebarftserver" => "Xbox Game Bar",
            "streamlabs obs" | "streamlabs desktop" => "Streamlabs Desktop",
            "meld studio" | "meldstudio" => "Meld Studio",
            _ => continue,
        };
        if !v.iter().any(|x| x == l) {
            v.push(l.to_string());
        }
    }
    v
}

// ---------------------------------------------------------------------------
// Drives

fn existing_ancestor(p: &std::path::Path) -> Option<std::path::PathBuf> {
    let mut cur = Some(p);
    while let Some(c) = cur {
        if c.exists() {
            return Some(c.to_path_buf());
        }
        cur = c.parent();
    }
    None
}

/// Whether a folder is on a spinning hard drive. Linux only; None if unknown.
pub fn drive_is_hdd(p: &std::path::Path) -> Option<bool> {
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::MetadataExt;
        let dev = std::fs::metadata(existing_ancestor(p)?).ok()?.dev();
        let major = ((dev >> 8) & 0xfff) | ((dev >> 32) & !0xfff);
        let minor = (dev & 0xff) | ((dev >> 12) & !0xff);
        if major == 0 {
            return None; // btrfs and other virtual devices
        }
        let sys = std::fs::canonicalize(format!("/sys/dev/block/{major}:{minor}")).ok()?;
        for d in [sys.clone(), sys.parent()?.to_path_buf()] {
            if let Ok(t) = std::fs::read_to_string(d.join("queue/rotational")) {
                return Some(t.trim() == "1");
            }
        }
        None
    }
    #[cfg(windows)]
    {
        let _ = p;
        None
    }
}

/// Whether two paths are on the same drive.
pub fn same_drive(a: &std::path::Path, b: &std::path::Path) -> bool {
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::MetadataExt;
        match (existing_ancestor(a).and_then(|x| std::fs::metadata(x).ok()), existing_ancestor(b).and_then(|x| std::fs::metadata(x).ok())) {
            (Some(x), Some(y)) => x.dev() == y.dev(),
            _ => false,
        }
    }
    #[cfg(windows)]
    {
        let d = |p: &std::path::Path| p.to_string_lossy().chars().take(2).collect::<String>().to_uppercase();
        d(a) == d(b)
    }
}

#[cfg(test)]
mod name_tests {
    #[test]
    fn proton_command_line_names() {
        assert_eq!(super::base_name("Z:\\mnt\\games\\SteamLibrary\\steamapps\\common\\Halo MCC\\MCC\\Binaries\\Win64\\MCC-Win64-Shipping.exe"), "mcc-win64-shipping.exe");
        assert_eq!(super::base_name("/usr/bin/obs"), "obs");
        assert!(crate::games::process_matches("mcc-win64-shipping.exe", "mcc-win64-shipping.exe"));
    }
}
