// SPDX-License-Identifier: GPL-3.0-or-later
//! Turns everything we measured into a readiness score, a status for each
//! check, and a list of issues. Each issue offers options with trade-offs;
//! the user's priorities decide which one is marked "Suggested".

use crate::config::{Settings, Style};
use crate::games::GameResult;
use crate::hardware::HardwareInfo;
use crate::livetest::TestResult;
use crate::network::NetResult;
use crate::obs::ObsSnapshot;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, PartialOrd, Ord)]
pub enum Status {
    Bad,
    Warn,
    Unknown,
    Ok,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, PartialOrd, Ord)]
pub enum Priority {
    High,
    Medium,
    Low,
}

impl Priority {
    #[allow(dead_code)]
    pub fn label(self) -> &'static str {
        match self {
            Priority::High => "High",
            Priority::Medium => "Medium",
            Priority::Low => "Low",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Check {
    pub name: String,
    pub status: Status,
    pub label: String,
    pub metric: String,
    pub note: String,
    pub measured: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct Opt {
    pub id: String,
    pub name: String,
    pub menu: String,
    pub gain: String,
    pub cost: String,
    pub suggested: bool,
    /// The exact settings to change: "Quick preset: Ultra → High".
    pub changes: Vec<Change>,
}

/// One setting to change, with what it is now when Preflight could read it.
#[derive(Clone, Debug, Serialize)]
pub struct Change {
    pub setting: String,
    pub menu: String,
    pub now: Option<String>,
    pub to: String,
}

fn change(
    setting: impl Into<String>,
    menu: impl Into<String>,
    now: Option<String>,
    to: impl Into<String>,
) -> Change {
    Change {
        setting: setting.into(),
        menu: menu.into(),
        now,
        to: to.into(),
    }
}

impl Opt {
    fn with(mut self, c: Change) -> Self {
        self.changes.push(c);
        self
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Issue {
    pub id: String,
    pub priority: Priority,
    pub area: String,
    pub title: String,
    pub why: String,
    pub who: String,
    pub learn: String,
    pub measured: bool,
    pub options: Vec<Opt>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Analysis {
    pub score: u8,
    pub headline: String,
    pub summary: String,
    pub bottleneck: String,
    pub checks: Vec<Check>,
    pub issues: Vec<Issue>,
    pub current_output: String,
    pub recommended_output: String,
    /// "Your recommended OBS settings": one row per setting.
    pub recommended: Vec<RecRow>,
    /// "Your setup": OBS and game, now vs suggested, in plain words.
    pub setup: SetupView,
    /// Short verdict for the readiness score, e.g. "Good to stream".
    pub verdict: String,
    /// How smooth the stream was in the test (measured). None before a test.
    pub smoothness: Option<u8>,
    /// How well the settings are set up (from settings, not measured).
    pub setup_score: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub enum Tag {
    #[default]
    Keep,
    Change,
    Optional,
    /// The user chose to keep it as it is.
    Kept,
    /// Can't be checked right now (for example, OBS isn't connected).
    Unknown,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct SetupRow {
    pub setting: String,
    pub now: String,
    pub suggested: String,
    pub why: String,
    pub tag: Tag,
    pub menu: String,
    pub issue_id: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct SetupView {
    pub summary: String,
    pub obs: Vec<SetupRow>,
    pub game_name: Option<String>,
    pub game: Vec<SetupRow>,
    pub game_note: Option<String>,
    /// "Read 86 settings from Settings.coc · saved by the game at 10:52".
    pub game_file: Option<String>,
    /// The game's file isn't fully understood: a bug report would help.
    pub report_helps: bool,
    /// "The advice uses N of the M settings read."
    pub game_coverage: Option<String>,
    /// Things fixed on the PC itself (temperatures, background programs, disk...).
    pub pc: Vec<SetupRow>,
}

/// What fixing an issue does for the user, in a few words.
pub fn impact_of(id: &str) -> &'static str {
    match id {
        "gpu_maxed" | "gpu_maybe" | "gpu_near" => "Stops stutter in busy scenes",
        "encoder" => "Frees your CPU for the game",
        "encoder_lag" | "enc_load" => "Stops skipped frames",
        "rate_control" => "Steadier stream for viewers",
        "keyframes" => "Faster start for new viewers",
        "preset" => "Cleaner picture",
        "downscale" => "Sharper text and HUD",
        "bitrate_network" | "net_unsteady" => "Stops buffering for viewers",
        "bitrate_platform" => "The platform accepts your stream",
        "no_game_audio" => "Viewers hear the game",
        "double_desktop_audio" => "Game sound heard once, without echo",
        "audio_balance" | "mic_quiet" => "Viewers hear you clearly",
        "display_capture" => "Keeps notifications and Discord private",
        "rec_format" => "Recordings survive crashes",
        "rec_encoder" | "replay_buffer" | "multitrack" => "Lighter load on the encoder",
        "recording_mismatch" => "The test matches your real stream",
        "disk_space" => "Recording won't stop mid-stream",
        "slow_disk" => "No dropped recording frames",
        "cpu_high" => "Smoother game and stream",
        "heat" => "No slowdowns from heat",
        "hogs" => "More bandwidth and CPU for the stream",
        _ => "Better stream",
    }
}

/// Readiness points an issue costs.
pub fn points_of(p: Priority) -> i32 {
    match p {
        Priority::High => 18,
        Priority::Medium => 10,
        Priority::Low => 4,
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct RecRow {
    pub setting: String,
    pub yours: String,
    pub suggested: String,
    pub menu: String,
    pub ok: bool,
}

/// Where the user makes the change, so fixes can be done in one sitting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Place {
    Obs,
    Game,
    Pc,
}

impl Place {
    pub fn label(self) -> &'static str {
        match self {
            Place::Obs => "In OBS",
            Place::Game => "In your game",
            Place::Pc => "On your PC",
        }
    }
}

pub fn place_of(id: &str) -> Place {
    match id {
        "gpu_maxed" | "gpu_maybe" | "gpu_near" | "bitrate_effects" | "cpu_high" => Place::Game,
        "heat" | "hogs" | "net_unsteady" | "disk_space" | "slow_disk" => Place::Pc,
        "capture_judder" | "ndi_cost" => Place::Game,
        _ => Place::Obs,
    }
}

/// Low-priority issues are optional extras, shown after the important ones.
pub fn is_optional(i: &Issue) -> bool {
    i.priority == Priority::Low
}

/// Preset speed as a number: 1 fastest .. 7 slowest. None if unknown.
pub fn preset_rank(p: &str) -> Option<u8> {
    let p = p.to_lowercase();
    match p.as_str() {
        "p1" | "ultrafast" | "speed" | "tu7" => Some(1),
        "p2" | "superfast" | "hp" | "llhp" | "ll" | "tu6" => Some(2),
        "p3" | "veryfast" | "tu5" => Some(3),
        "p4" | "faster" | "balanced" | "tu4" | "default" | "llhq" => Some(4),
        "p5" | "fast" | "quality" | "tu3" | "hq" => Some(5),
        "p6" | "medium" | "tu2" => Some(6),
        "p7" | "slow" | "slower" | "veryslow" | "placebo" | "tu1" => Some(7),
        _ => None,
    }
}

/// The recommended NVENC preset, worded the same everywhere (Setup table, fleet line):
/// "P6 or P7", "P6 or P7 (P5 for <busy game>)", or "P5" when every game keeps the GPU busy.
pub fn nvenc_preset_words(busy_game: Option<&str>, all_busy: bool) -> String {
    match busy_game {
        _ if all_busy => "P5".into(),
        Some(g) => format!("P6 or P7 (P5 for {g})"),
        None => "P6 or P7".into(),
    }
}

struct Ctx<'a> {
    st: &'a Settings,
    hw: Option<&'a HardwareInfo>,
    games: &'a [GameResult],
    obs: Option<&'a ObsSnapshot>,
    net: Option<&'a NetResult>,
    test: Option<&'a TestResult>,
    /// The user changed the game's heavy settings since the test.
    gpu_changed: bool,
}

impl Ctx<'_> {
    /// The game's FPS: measured in the test, or what the user told us.
    fn game_fps(&self, game: &str) -> Option<f32> {
        self.test
            .and_then(|t| t.game_fps)
            .or_else(|| self.st.game_fps_said.get(game).map(|v| *v as f32))
    }
}

fn opt(
    id: &str,
    name: impl Into<String>,
    menu: impl Into<String>,
    gain: impl Into<String>,
    cost: impl Into<String>,
) -> Opt {
    Opt {
        id: id.into(),
        name: name.into(),
        menu: menu.into(),
        gain: gain.into(),
        cost: cost.into(),
        suggested: false,
        changes: Vec::new(),
    }
}

/// An FPS limit outside the game, for games without one in their menus.
/// Nothing is changed by Preflight: these are steps for the player.
fn outside_cap(
    g: &GameResult,
    cap: u32,
    vendor: Option<crate::hardware::Vendor>,
    cost: String,
) -> Opt {
    use crate::hardware::Vendor;
    let dxvk = matches!(g.api.as_deref(), Some("dx9" | "dx10" | "dx11"));
    // A game read automatically may still have its own limit: say "if".
    let no_limit = if g.auto.is_some() {
        format!("If {} has no FPS limit of its own", g.name)
    } else {
        format!("{} has no FPS limit of its own", g.name)
    };
    if cfg!(not(windows)) {
        // Where launch options or environment variables go, per launcher.
        let (menu, env_style) = match g.launcher.as_str() {
            "Heroic" => (
                format!(
                    "Heroic › {} › Settings › Advanced › Environment Variables",
                    g.name
                ),
                true,
            ),
            "Lutris" => (
                format!(
                    "Lutris › right-click {} › Configure › System options › Environment variables",
                    g.name
                ),
                true,
            ),
            "Bottles" => (
                "Bottles › your bottle › Settings › Environment Variables".to_string(),
                true,
            ),
            _ => (
                format!(
                    "Steam › Library › right-click {} › Properties › General › Launch Options",
                    g.name
                ),
                false,
            ),
        };
        if g.proton && dxvk {
            let line = if env_style {
                format!("DXVK_FRAME_RATE = {cap}")
            } else {
                format!("DXVK_FRAME_RATE={cap} %command%")
            };
            return opt("cap", format!("Cap FPS at {cap} with {}", if env_style { "an environment variable" } else { "a Steam launch option" }), menu.clone(), format!("{no_limit}. This one is built into Proton/DXVK, so nothing is added to the game"), cost)
                .with(change(if env_style { "Environment variable" } else { "Launch Options" }, menu, Some("—".into()), line));
        }
        let line = if env_style {
            format!("MANGOHUD = 1 and MANGOHUD_CONFIG = fps_limit={cap}")
        } else {
            format!("MANGOHUD_CONFIG=fps_limit={cap} mangohud %command%")
        };
        return opt(
            "cap",
            format!("Cap FPS at {cap} with MangoHud"),
            menu.clone(),
            no_limit,
            format!("{cost}. Needs MangoHud installed; skip it in games with anti-cheat"),
        )
        .with(change(
            if env_style {
                "Environment variables"
            } else {
                "Launch Options"
            },
            menu,
            Some("—".into()),
            line,
        ));
    }
    let (menu, setting) = match vendor {
        Some(Vendor::Nvidia) => (
            format!(
                "NVIDIA app › Graphics › Program Settings › {} › Max Frame Rate",
                g.name
            ),
            "Max Frame Rate",
        ),
        Some(Vendor::Amd) => (
            format!(
                "AMD Software › Gaming › {} › Frame Rate Target Control",
                g.name
            ),
            "Frame Rate Target Control",
        ),
        Some(Vendor::Intel) => (
            format!(
                "Intel Graphics Software › Gaming › {} › Frame Rate Limiter",
                g.name
            ),
            "Frame Rate Limiter",
        ),
        _ => (
            "Your graphics driver's control panel › frame rate limit for this game".to_string(),
            "Frame rate limit",
        ),
    };
    opt(
        "cap",
        format!("Cap FPS at {cap} in your graphics driver"),
        menu.clone(),
        no_limit,
        cost,
    )
    .with(change(setting, menu, Some("Off".into()), cap.to_string()))
}

/// The FPS cap to suggest: one the game will actually hit, so the GPU gets
/// headroom. About twice the stream's frame rate (120 for a 60 FPS stream),
/// never above the monitor, and below the game's real FPS when known.
pub fn cap_target(hz: u32, stream_fps: Option<f32>, style: Style, game_fps: Option<f32>) -> u32 {
    const NICE: [u32; 13] = [30, 45, 60, 72, 90, 100, 120, 144, 165, 180, 200, 240, 360];
    let sf = stream_fps.unwrap_or(60.0).clamp(24.0, 120.0);
    let mut want = match style {
        Style::Competitive => 240.0,
        _ => (sf * 2.0).max(60.0),
    };
    want = want.min(hz.max(30) as f32);
    if let Some(g) = game_fps.filter(|g| *g > 10.0) {
        want = want.min(g * 0.9);
    }
    NICE.iter()
        .rev()
        .copied()
        .find(|n| *n as f32 <= want + 0.5)
        .unwrap_or(30)
}

/// Which fix comes first for each style:
/// - Competitive keeps FPS high: lower graphics first (heavy settings, ray
///   tracing, preset), then DLSS/FSR, and an FPS cap only as a last resort.
/// - Balanced: DLSS/FSR first, then an FPS cap, then graphics.
/// - Cinematic keeps the graphics: an FPS cap first, then DLSS/FSR.
pub fn style_pick(st: &Settings, opts: &[Opt]) -> String {
    let has = |id: &str| opts.iter().any(|o| o.id == id);
    let rt = opts
        .iter()
        .find(|o| o.id.starts_with("rt_"))
        .map(|o| o.id.clone());
    let heavy = opts
        .iter()
        .find(|o| o.id.starts_with("h_"))
        .map(|o| o.id.clone());
    let graphics = if has("tune") {
        Some("tune".to_string())
    } else {
        rt.clone()
            .or(heavy.clone())
            .or_else(|| has("preset").then(|| "preset".to_string()))
    };
    let first = |ids: Vec<Option<String>>| {
        ids.into_iter()
            .flatten()
            .find(|i| has(i))
            .unwrap_or_else(|| opts.first().map(|o| o.id.clone()).unwrap_or_default())
    };
    let s = |x: &str| Some(x.to_string());
    let style = match st.style {
        Style::Custom => {
            if st.fps_vs_visuals <= 30 {
                Style::Competitive
            } else if st.fps_vs_visuals >= 70 {
                Style::Cinematic
            } else {
                Style::Balanced
            }
        }
        x => x,
    };
    match style {
        Style::Competitive => first(vec![graphics, s("upscale"), s("cap"), s("cap_out")]),
        Style::Cinematic => first(vec![s("cap"), s("cap_out"), s("upscale"), graphics]),
        _ => first(vec![s("upscale"), s("cap"), s("cap_out"), graphics]),
    }
}

/// Heads-ups that inform without counting against the scores.
pub fn unscored(id: &str) -> bool {
    matches!(id, "gpu_near" | "bitrate_effects")
}

fn suggest(opts: &mut [Opt], id: &str) {
    let any = opts.iter().any(|o| o.id == id);
    for o in opts.iter_mut() {
        o.suggested = if any { o.id == id } else { false };
    }
    if !any && let Some(o) = opts.first_mut() {
        o.suggested = true;
    }
}

/// "about 1 in 125" for a fraction of frames.
fn one_in(frac: f32) -> String {
    if frac <= 0.0 {
        return "none".into();
    }
    let n = (1.0 / frac).round() as u32;
    format!("about 1 in every {n}")
}

/// What a gap between mic and game volume means to viewers.
pub fn mic_gap_meaning(db: f32) -> &'static str {
    if db < 3.0 {
        "That's barely noticeable, so you're fine."
    } else if db < 6.0 {
        "That's noticeable, so viewers may lean in to hear you."
    } else if db < 10.0 {
        "That's clearly quieter, so viewers may have trouble hearing you over gameplay."
    } else {
        "That's much quieter, so your voice will get lost under the game."
    }
}

fn res_label(r: Option<(u32, u32)>) -> String {
    match r {
        Some((_, h)) => format!("{h}p"),
        None => "?".into(),
    }
}

pub fn analyze(
    st: &Settings,
    hw: Option<&HardwareInfo>,
    games: &[GameResult],
    obs: Option<&ObsSnapshot>,
    net: Option<&NetResult>,
    test: Option<&TestResult>,
    gpu_changed: bool,
) -> Analysis {
    let c = Ctx {
        st,
        hw,
        games,
        obs,
        net,
        test,
        gpu_changed,
    };
    let mut a = Analysis::default();
    let mut issues: Vec<Issue> = Vec::new();

    encoder(&c, &mut a, &mut issues);
    stream_quality(&c, &mut a, &mut issues);
    recording(&c, &mut a, &mut issues);
    network(&c, &mut a, &mut issues);
    game_load(&c, &mut a, &mut issues);
    picture_quality(&c, &mut a, &mut issues);
    encode_budget(&c, &mut a, &mut issues);
    multistream(&c, &mut a, &mut issues);
    audio(&c, &mut a, &mut issues);
    audio_settings(&c, &mut issues);
    hardware_and_heat(&c, &mut a, &mut issues);
    scene(&c, &mut a, &mut issues);
    background(&c, &mut issues);
    two_pc(&c, &mut issues);
    // Two PCs: the encoder isn't on the gaming PC, so "leave room for the
    // encoder" advice doesn't apply; on the streaming PC the game isn't here.
    match crate::twopc::Role::from_str(&st.pc_role) {
        crate::twopc::Role::One => {}
        crate::twopc::Role::Gaming => issues.retain(|i| {
            !matches!(
                i.id.as_str(),
                "gpu_maxed" | "gpu_maybe" | "gpu_near" | "encode_budget" | "cpu_encodes"
            )
        }),
        crate::twopc::Role::Streaming => issues.retain(|i| {
            !matches!(
                i.id.as_str(),
                "gpu_maxed" | "gpu_maybe" | "gpu_near" | "bitrate_effects"
            )
        }),
    }

    // Options the user asked never to see again.
    for i in issues.iter_mut() {
        let had_suggested = i.options.iter().any(|o| o.suggested);
        i.options.retain(|o| !st.is_hidden(&i.id, &o.id));
        if had_suggested
            && !i.options.iter().any(|o| o.suggested)
            && let Some(o) = i.options.first_mut()
        {
            o.suggested = true;
        }
    }
    issues.sort_by_key(|i| i.priority);
    let mut score: i32 = 100;
    for i in issues
        .iter()
        .filter(|i| !st.kept_issues.contains(&i.id) && !unscored(&i.id))
    {
        score -= points_of(i.priority);
    }
    for ch in &a.checks {
        if ch.status == Status::Unknown {
            score -= 3;
        }
    }
    a.score = score.clamp(0, 100) as u8;
    let fixes = issues.len();
    let unknown = a
        .checks
        .iter()
        .filter(|c| c.status == Status::Unknown)
        .count();
    a.headline = match (a.score, fixes) {
        (90..=100, 0) => "Ready to go live".into(),
        (_, 0) if unknown > 0 => format!(
            "{unknown} check{} couldn't run yet",
            if unknown == 1 { "" } else { "s" }
        ),
        (_, 0) => "Almost ready".into(),
        (90..=100, _) => "Almost ready".into(),
        (s, n) if s >= 70 => format!(
            "Nearly there. {n} issue{} to look at",
            if n == 1 { "" } else { "s" }
        ),
        (_, n) => format!(
            "Not quite ready. {n} issue{} to look at",
            if n == 1 { "" } else { "s" }
        ),
    };
    a.summary = if test.is_none() {
        "Based on a quick scan. Run a full test while playing to measure real performance.".into()
    } else {
        "Based on your last full test and scan.".into()
    };
    a.recommended = recommended(&c, &a);
    a.setup = setup_view(&c, &a, &issues);
    let open: Vec<&Issue> = issues
        .iter()
        .filter(|i| !st.kept_issues.contains(&i.id))
        .collect();
    let important = open.iter().filter(|i| !is_optional(i)).count();
    let optional = open.len() - important;
    // Two scores: what the test measured, and how the settings are set up.
    let mut smooth: i32 = 100;
    let mut setup_pts: i32 = 100;
    for i in open.iter().filter(|i| !unscored(&i.id)) {
        if i.measured && test.is_some() {
            smooth -= points_of(i.priority);
        } else {
            setup_pts -= points_of(i.priority);
        }
    }
    for ch in &a.checks {
        if ch.status == Status::Unknown {
            setup_pts -= 3;
        }
    }
    a.smoothness = test.map(|_| smooth.clamp(0, 100) as u8);
    a.setup_score = setup_pts.clamp(0, 100) as u8;
    a.verdict = verdict_text(a.smoothness, important, optional);
    a.issues = issues;
    a
}

/// "Your setup" as plain text to paste into Discord. Only settings and
/// numbers: no names, paths, scene names, keys or passwords.
pub fn setup_text(a: &Analysis) -> String {
    let mut t = String::new();
    t.push_str(&format!(
        "Stream Preflight · my setup ({})\n",
        crate::util::now_local()
    ));
    let sm = a
        .smoothness
        .map(|v| v.to_string())
        .unwrap_or_else(|| "not tested".into());
    t.push_str(&format!(
        "Smoothness {sm} · Setup {} · {}\n",
        a.setup_score, a.verdict
    ));
    let section = |t: &mut String, title: &str, rows: &[SetupRow]| {
        if rows.is_empty() {
            return;
        }
        t.push_str(&format!("\n{title}\n"));
        for r in rows {
            let tag = match r.tag {
                Tag::Change => "CHANGE",
                Tag::Optional => "optional",
                Tag::Kept => "kept as is",
                Tag::Unknown => "unknown",
                Tag::Keep => "ok",
            };
            if matches!(r.tag, Tag::Keep | Tag::Kept | Tag::Unknown) {
                t.push_str(&format!("- {}: {} ({tag})\n", r.setting, r.now));
            } else {
                t.push_str(&format!(
                    "- {}: {} -> {} ({tag}) · {}\n",
                    r.setting, r.now, r.suggested, r.why
                ));
            }
        }
    };
    section(&mut t, "In OBS", &a.setup.obs);
    section(
        &mut t,
        &format!(
            "In {}",
            a.setup
                .game_name
                .clone()
                .unwrap_or_else(|| "your game".into())
        ),
        &a.setup.game,
    );
    section(&mut t, "On my PC", &a.setup.pc);
    t
}

/// One sentence: how the stream ran, then what's left to change.
pub fn verdict_text(smooth: Option<u8>, important: usize, optional: usize) -> String {
    let ran = match smooth {
        None => "Not tested yet".to_string(),
        Some(90..=100) => "Your stream ran smoothly".into(),
        Some(70..=89) => "Your stream mostly ran smoothly".into(),
        Some(45..=69) => "Your stream stuttered at times".into(),
        Some(_) => "Your stream stuttered a lot".into(),
    };
    let left = match (important, optional) {
        (0, 0) => "nothing to change".to_string(),
        (0, o) => format!("{o} optional improvement{}", if o == 1 { "" } else { "s" }),
        (i, _) => format!("{i} change{} will help", if i == 1 { "" } else { "s" }),
    };
    format!("{ran}. {}{}.", left[..1].to_uppercase(), &left[1..])
}

// ---------------------------------------------------------------------------
// "Your setup"

fn tag_for(issue: Option<&Issue>, ok: bool, st: &Settings) -> Tag {
    match issue {
        Some(i) if st.kept_issues.contains(&i.id) => Tag::Kept,
        Some(i) => {
            if is_optional(i) {
                Tag::Optional
            } else {
                Tag::Change
            }
        }
        None => {
            if ok {
                Tag::Keep
            } else {
                Tag::Change
            }
        }
    }
}

/// Short setting-style name for an issue that has no row of its own.
fn row_name(id: &str) -> &'static str {
    match id {
        "recording_mismatch" => "Test recording",
        "enc_load" => "Encoder load",
        "encoder_lag" => "Encoder keeping up",
        "replay_buffer" => "Replay buffer",
        "obs_old" => "OBS version",
        "heat" => "Temperatures",
        "hogs" => "Background programs",
        "net_unsteady" => "Internet steadiness",
        "disk_space" => "Recording space",
        "slow_disk" => "Recording drive",
        "cpu_high" => "CPU load",
        "net_unreachable" => "Internet test",
        "encode_budget" => "Encodes",
        "cpu_encodes" => "CPU encodes",
        "multistream" => "Several platforms",
        "enc_quality" => "NVENC quality options",
        "rec_native" => "Recording size",
        "resolution_bitrate" => "Output size",
        "audio_bitrate" => "Audio bitrate",
        "sample_rate" => "Sample rate",
        "bitrate_effects" => "In-game effects",
        _ => "Other",
    }
}

/// The game the advice is about: the tested game (by name, or by program
/// name for a game without a profile), else the one running. After a test
/// where no game was recognised, settings files aren't used to guess one:
/// the advice would be for the wrong game.
fn target_game<'a>(c: &Ctx<'a>, any_detected: bool) -> Option<&'a GameResult> {
    let detected: Vec<&GameResult> = c.games.iter().filter(|g| g.relevant()).collect();
    if let Some(t) = c.test {
        if let Some(n) = &t.game_running
            && let Some(g) = detected.iter().find(|g| &g.name == n)
        {
            return Some(g);
        }
        if let Some(guess) = &t.game_guess
            && let Some(g) = detected.iter().find(|g| {
                g.processes
                    .iter()
                    .any(|p| p == guess || crate::games::process_matches(guess, p))
            })
        {
            return Some(g);
        }
        if t.game_running.is_none() && !detected.iter().any(|g| g.running) {
            return None;
        }
    }
    detected
        .iter()
        .find(|g| g.running)
        .copied()
        .or_else(|| {
            detected
                .iter()
                .find(|g| !g.findings.is_empty() && g.auto.is_none())
                .copied()
        })
        .or_else(|| {
            if any_detected {
                detected.iter().find(|g| g.auto.is_none()).copied()
            } else {
                None
            }
        })
}

fn setup_view(c: &Ctx, a: &Analysis, issues: &[Issue]) -> SetupView {
    let mut v = SetupView::default();
    let st = c.st;
    let find = |id: &str| issues.iter().find(|i| i.id == id);
    let first = |ids: &[&str]| ids.iter().find_map(|id| find(id));
    let plat = st.platform.name();
    // ---- OBS
    if let Some(o) = c.obs {
        for r in &a.recommended {
            let (issue, why_ok, why_change): (Option<&Issue>, String, String) = match r
                .setting
                .as_str()
            {
                "Encoder" => (
                    first(&["encoder"]),
                    if o.encoder_kind.contains("CPU") {
                        "No hardware encoder found, so the CPU is the only choice".into()
                    } else {
                        "Uses your graphics card, so the game keeps your CPU".into()
                    },
                    "Encoding on the CPU competes with your game".into(),
                ),
                "Bitrate" => {
                    let head = c.net.filter(|n| n.ok).map(|n| {
                        let need = (o.bitrate_kbps.unwrap_or(6000) as f32 + 160.0) / 1000.0;
                        format!("; your upload has about {:.0}x headroom", n.low_mbps / need)
                    });
                    (
                        first(&["bitrate_network", "bitrate_platform"]),
                        format!("Fits {plat}{}", head.unwrap_or_default()),
                        "Too high for your connection or platform".into(),
                    )
                }
                "Rate control" => (
                    first(&["rate_control"]),
                    format!("CBR is what {plat} expects"),
                    "Bitrate spikes can cause dropped frames".into(),
                ),
                "Keyframe interval" => (
                    first(&["keyframes"]),
                    "Every 2 s is the standard".into(),
                    format!("{plat} wants a keyframe every 2 s"),
                ),
                "Preset" => (
                    first(&["preset"]),
                    "Good quality without slowing the encoder".into(),
                    "Too fast to look good, or too slow to keep up".into(),
                ),
                "Output (resolution · FPS)" => (
                    first(&["bitrate_network", "resolution_bitrate"]),
                    "Sharp for your bitrate and your game's motion".into(),
                    if find("resolution_bitrate").is_some() {
                        "A smaller picture stays sharp at your bitrate".into()
                    } else {
                        "A smaller picture keeps the stream smooth on your connection".into()
                    },
                ),
                "Downscale filter" => (
                    first(&["downscale"]),
                    "Keeps text and edges sharp".into(),
                    "Bilinear makes text and HUD blurry".into(),
                ),
                "Audio" => (
                    first(&["audio_bitrate", "sample_rate"]),
                    "Clear voice and game sound".into(),
                    "Muffled audio or extra resampling".into(),
                ),
                "Recording encoder" => {
                    let chips = crate::hardware::nvenc_chips(
                        &c.hw
                            .and_then(|h| h.main_gpu())
                            .map(|g| g.name.clone())
                            .unwrap_or_default(),
                    );
                    let ok = if o.rec_encoder_raw.is_empty() {
                        "Shares the stream encoder: no extra load".to_string()
                    } else if let Some(n) = chips.filter(|n| *n >= 2) {
                        format!(
                            "Your card has {n} encoder chips, so recording separately doesn't slow the stream"
                        )
                    } else {
                        "Light enough for your card".to_string()
                    };
                    (
                        first(&["rec_encoder", "enc_load"]),
                        ok,
                        "Adds work to the encoder".into(),
                    )
                }
                "Recording format" => (
                    first(&["rec_format"]),
                    "Survives crashes".into(),
                    "Plain MP4/MOV is lost if OBS crashes".into(),
                ),
                "Auto-record when streaming" => (
                    None,
                    if o.auto_record == Some(true) {
                        "Every stream is recorded too; Preflight watches both".into()
                    } else {
                        "Start recording yourself when you want a copy".into()
                    },
                    String::new(),
                ),
                "Enhanced Broadcasting" => (
                    first(&["multitrack"]),
                    "Your encoder has room for it".into(),
                    "Several stream versions keep your encoder busy".into(),
                ),
                _ => (None, String::new(), String::new()),
            };
            // A setting that isn't ideal but isn't causing a problem here is optional, never "Change".
            let tag = if r.ok && issue.is_none() {
                Tag::Keep
            } else if issue.is_none() {
                Tag::Optional
            } else {
                tag_for(issue, r.ok, st)
            };
            let why_change = if issue.is_none() && r.setting == "Downscale filter" {
                "Only matters when OBS shrinks your picture; Bicubic is safer if you do".to_string()
            } else {
                why_change
            };
            v.obs.push(SetupRow {
                setting: r.setting.clone(),
                now: r.yours.clone(),
                suggested: if tag == Tag::Keep || tag == Tag::Kept {
                    "Keep".into()
                } else {
                    r.suggested.clone()
                },
                why: if tag == Tag::Keep { why_ok } else { why_change },
                tag,
                menu: r.menu.clone(),
                issue_id: issue.map(|i| i.id.clone()),
            });
        }
        // Audio and capture
        let ga = find("no_game_audio");
        let doubled = o.doubled_desktop_audio() >= 2;
        if !o.connected && o.desktop_audio().is_empty() {
            // Saved OBS files don't always list global audio, so "None" could be wrong.
            v.obs.push(SetupRow {
                setting: "Game audio".into(),
                now: "Unknown".into(),
                suggested: "Connect OBS to check".into(),
                why: "OBS isn't connected, so its audio sources can't be checked".into(),
                tag: Tag::Unknown,
                menu: "OBS › Settings › Audio › Desktop Audio".into(),
                issue_id: None,
            });
        } else {
            v.obs.push(SetupRow {
                setting: "Game audio".into(),
                now: if o.desktop_audio().is_empty() {
                    "None".into()
                } else if doubled {
                    "Captured twice".into()
                } else {
                    "Captured".into()
                },
                suggested: if ga.is_some() {
                    "Add Desktop Audio".into()
                } else {
                    "Keep".into()
                },
                why: if ga.is_some() {
                    "Viewers won't hear the game".into()
                } else {
                    "Viewers hear the game".into()
                },
                tag: if ga.is_some() {
                    tag_for(ga, false, st)
                } else {
                    Tag::Keep
                },
                menu: "OBS › Settings › Audio › Desktop Audio".into(),
                issue_id: ga.map(|i| i.id.clone()),
            });
        }
        if let Some(t) = c.test
            && let Some(m) = t.mic_db
        {
            let mi = first(&["audio_balance", "mic_quiet"]);
            let now = match t.desktop_db {
                Some(d) => format!("{:.0} dB under the game", (d - m).max(0.0)),
                None => format!("Peaks at {m:.0} dB"),
            };
            // Without game sound in the test, the two can't be compared.
            let compared = t.desktop_db.is_some();
            v.obs.push(SetupRow {
                setting: "Mic level".into(),
                now: if mi.is_some() {
                    now
                } else if compared {
                    "Above the game".into()
                } else {
                    "Couldn't compare".into()
                },
                suggested: if mi.is_some() {
                    "Raise the mic".into()
                } else if compared {
                    "Keep".into()
                } else {
                    "Test with game sound".into()
                },
                why: if mi.is_some() {
                    "Viewers may not hear you over the game".into()
                } else if compared {
                    "Viewers hear you clearly".into()
                } else {
                    format!("No game sound was heard during the test; your mic peaked at {m:.0} dB")
                },
                tag: if mi.is_some() {
                    tag_for(mi, false, st)
                } else {
                    Tag::Keep
                },
                menu: "OBS › Audio Mixer › your mic".into(),
                issue_id: mi.map(|i| i.id.clone()),
            });
        }
        let cap = find("display_capture");
        if o.uses_display_capture() || o.uses_game_capture() || o.uses_portal_capture() {
            let now = if o.uses_game_capture() {
                if o.sources.iter().any(|s| s.kind.contains("vkcapture")) {
                    "Game capture (vkcapture)".to_string()
                } else {
                    "Game capture".to_string()
                }
            } else if o.uses_display_capture() {
                "Whole screen".to_string()
            } else {
                "PipeWire (screen or window)".to_string()
            };
            let portal_only = cap.is_none() && !o.uses_game_capture() && o.uses_portal_capture();
            let has_vk = o.plugins.iter().any(|p| p.contains("vkcapture"));
            v.obs.push(SetupRow {
                setting: "Game capture".into(),
                now,
                suggested: if cap.is_some() { "Capture just the game window".into() } else if portal_only && has_vk { "Keep, or try Game Capture".into() } else { "Keep".into() },
                why: if cap.is_some() {
                    "A whole-screen capture can show Discord and notifications".into()
                } else if portal_only && has_vk {
                    "Fine if you picked the game's window. You also have the vkcapture plugin: its Game Capture source shows only the game and is usually lighter".into()
                } else if portal_only {
                    "Fine if you picked the game's window, not a screen".into()
                } else {
                    "Only the game is shown".into()
                },
                tag: if cap.is_some() { tag_for(cap, false, st) } else { Tag::Keep },
                menu: "OBS › Sources › your capture › Properties".into(),
                issue_id: cap.map(|i| i.id.clone()),
            });
        }
    }

    // ---- Game
    let target = target_game(c, false);
    let gpu_issue = first(&["gpu_maxed", "gpu_maybe", "gpu_near"]);
    let need_room = gpu_issue.map(|i| !is_optional(i)).unwrap_or(false);
    if let Some(g) = target {
        v.game_name = Some(g.name.clone());
        // Did Preflight connect to the settings file? Say it first.
        let read: Vec<&crate::games::FileResult> = g.files.iter().filter(|f| f.read_ok).collect();
        v.game_file = Some(match read.first() {
            Some(f) => {
                let short = |f: &crate::games::FileResult| {
                    if f.inline_text.is_some() || f.format == "unity prefs" {
                        "Unity's saved settings".to_string()
                    } else {
                        f.shown_path
                            .rsplit(['/', '\\'])
                            .next()
                            .unwrap_or(&f.shown_path)
                            .to_string()
                    }
                };
                let newest = read.iter().filter_map(|f| f.modified).max();
                let saved = newest
                    .or(f.modified)
                    .map(|t| format!(" · saved by the game {}", crate::util::fmt_local(t)))
                    .unwrap_or_default();
                let names: Vec<String> = read.iter().map(|f| short(f)).collect();
                let total: usize = read.iter().map(|f| f.settings_count).sum();
                if names.len() == 1 {
                    format!("Read {total} settings from {}{saved}", names[0])
                } else {
                    format!(
                        "Read {total} settings from {} files ({}){saved}",
                        names.len(),
                        names.join(", ")
                    )
                }
            }
            None => match g.files.iter().find(|f| f.found) {
                Some(f) => format!(
                    "Found {} but couldn't read it: {}",
                    f.shown_path.rsplit(['/', '\\']).next().unwrap_or(""),
                    f.error.clone().unwrap_or_else(|| "unknown error".into())
                ),
                None => format!(
                    "No settings file yet (looked in {} place{}). Launch the game once and change a setting.",
                    g.files.iter().map(|f| f.searched.len()).sum::<usize>(),
                    if g.files.iter().map(|f| f.searched.len()).sum::<usize>() == 1 {
                        ""
                    } else {
                        "s"
                    }
                ),
            },
        });
        // How much of what was read the advice actually uses.
        if g.settings_read > 0 {
            let other = g.settings_read.saturating_sub(g.settings_used);
            let uses = if other == 0 {
                format!("The advice uses all {} settings read.", g.settings_read)
            } else {
                format!(
                    "The advice uses {} of the {} settings read; Preflight couldn't check the other {other} (most are controls, audio and other non-graphics settings).",
                    g.settings_used, g.settings_read
                )
            };
            v.game_coverage = Some(if g.auto.is_some() {
                format!(
                    "Read automatically ({}), not hand-checked. {uses}",
                    g.auto.clone().unwrap_or_default()
                )
            } else {
                uses
            });
        }
        if let Some(gap) = &g.read_gap {
            v.game_coverage = Some(gap.clone());
        }
        v.report_helps = g.auto.is_some()
            || !g.unreadable.is_empty()
            || g.verified.starts_with("Unverified")
            || g.verified.starts_with("Not yet")
            || g.files.iter().any(|f| f.found && !f.read_ok);
        let kept = gpu_issue
            .map(|i| st.kept_issues.contains(&i.id))
            .unwrap_or(false);
        let mut shown: std::collections::HashSet<String> = std::collections::HashSet::new();
        // The fixes first: exactly what the Fix step suggests, from the same list.
        if let Some(i) = gpu_issue {
            let mut opts: Vec<&Opt> = i.options.iter().collect();
            opts.sort_by_key(|o| !o.suggested);
            for o in opts {
                for ch in &o.changes {
                    if !shown.insert(ch.setting.to_lowercase()) {
                        continue;
                    }
                    let tag = if kept {
                        Tag::Kept
                    } else if o.suggested && need_room {
                        Tag::Change
                    } else {
                        Tag::Optional
                    };
                    v.game.push(SetupRow {
                        setting: ch.setting.clone(),
                        now: ch.now.clone().unwrap_or_else(|| "Couldn't read".into()),
                        suggested: ch.to.clone(),
                        why: if o.suggested {
                            o.gain.clone()
                        } else {
                            "Only if the GPU is still maxed after the change above".into()
                        },
                        tag,
                        menu: ch.menu.clone(),
                        issue_id: Some(i.id.clone()),
                    });
                }
            }
        }
        // Then everything else Preflight read, as it is.
        let mut cur = g.current.clone();
        cur.sort_by_key(|s| match s.effect.as_str() {
            "uncapped_fps" => 1,
            "upscaler_off" => 2,
            "info" => 3,
            "heavy" => 4,
            "rt_on" => 5,
            _ => 6,
        });
        for s in &cur {
            if !shown.insert(s.setting.to_lowercase()) {
                continue;
            }
            let why = match (s.effect.as_str(), s.flagged) {
                ("uncapped_fps", false) => "Leaves GPU room for OBS".to_string(),
                ("uncapped_fps", true) => "Your GPU has room, so this is fine for now".to_string(),
                ("upscaler_off", false) => "Saves GPU at almost the same sharpness".to_string(),
                ("rt_on", true) | ("heavy", true) => "Your GPU handles it".to_string(),
                ("rt_on", false) | ("heavy", false) => "Off, so it costs nothing".to_string(),
                ("info", _) => {
                    let n = s.setting.to_lowercase();
                    let off = s.value.eq_ignore_ascii_case("off");
                    if n.contains("reflex") || n.contains("anti-lag") || n.contains("latency") {
                        if off {
                            "Off. Turning it on lowers input lag; it doesn't affect the stream"
                                .into()
                        } else {
                            "Lowers input lag; it doesn't affect the stream".into()
                        }
                    } else if n.contains("cmaa")
                        || n.contains("fxaa")
                        || n.contains("anti-aliasing")
                    {
                        "Light anti-aliasing that costs almost nothing".into()
                    } else if n.contains("frame gen") {
                        "Raises the FPS you see; doesn't free the GPU for OBS".into()
                    } else if off {
                        "Off: full sharpness. Turn it on only if you need more FPS".into()
                    } else {
                        "Raises the FPS you see and frees some GPU".into()
                    }
                }
                ("vsync_on", true) => "Caps FPS at your refresh rate".to_string(),
                ("bitrate", true) => {
                    // Fine noise and smear eat stream bitrate.
                    v.game.push(SetupRow { setting: s.setting.clone(), now: s.value.clone(), suggested: s.change_to.clone().unwrap_or_else(|| "Off".into()), why: "Adds fine noise or smear that costs stream bitrate. Off gives viewers a cleaner picture".into(), tag: Tag::Optional, menu: s.menu.clone(), issue_id: Some("bitrate_effects".into()) });
                    continue;
                }
                ("bitrate", false) => "Off, so viewers get a cleaner picture".to_string(),
                _ => "Fine as it is".to_string(),
            };
            v.game.push(SetupRow {
                setting: s.setting.clone(),
                now: s.value.clone(),
                suggested: "Keep".into(),
                why,
                tag: Tag::Keep,
                menu: s.menu.clone(),
                issue_id: None,
            });
        }
        if let Some(p) = &g.preset
            && shown.insert(p.setting.to_lowercase())
        {
            let why = if p.custom {
                "You've set individual settings; the heavy ones are listed here".to_string()
            } else {
                "Fine as it is".to_string()
            };
            v.game.push(SetupRow {
                setting: p.setting.clone(),
                now: p.value.clone(),
                suggested: "Keep".into(),
                why,
                tag: Tag::Keep,
                menu: p.menu.clone(),
                issue_id: None,
            });
        }
        for t in &g.tunables {
            if shown.insert(t.setting.to_lowercase()) {
                let why = match (t.change_to.is_some(), gpu_issue.is_some()) {
                    (true, true) => format!(
                        "Heavy. Lower it to {} too if the GPU is still maxed",
                        t.change_to.clone().unwrap_or_default()
                    ),
                    (true, false) => "Heavy, but your GPU has room right now".to_string(),
                    _ => "Fine as it is".to_string(),
                };
                v.game.push(SetupRow {
                    setting: t.setting.clone(),
                    now: t.now.clone(),
                    suggested: "Keep".into(),
                    why,
                    tag: Tag::Keep,
                    menu: t.menu.clone(),
                    issue_id: None,
                });
            }
        }
        // The game's own display mode next to the monitor's (inform only).
        if g.display_res.is_some() || g.display_hz.is_some() {
            let now = match (&g.display_res, g.display_hz) {
                (Some(r), Some(h)) => format!("{r} · {h:.0} Hz"),
                (Some(r), None) => r.clone(),
                (None, Some(h)) => format!("{h:.0} Hz"),
                _ => String::new(),
            };
            let mon = st.monitor_hz;
            let why = match g.display_hz {
                Some(h) if (h - mon as f32).abs() > 2.0 => format!(
                    "Your monitor runs at {mon} Hz; the game is set to {h:.0} Hz. Fine if that's on purpose"
                ),
                _ => "Matches your monitor".to_string(),
            };
            v.game.push(SetupRow {
                setting: "Game display mode".into(),
                now,
                suggested: "Keep".into(),
                why,
                tag: Tag::Keep,
                menu: format!("{} › Settings › Display", g.name),
                issue_id: None,
            });
        }
        // Settings a game update may have renamed: say so, never hide it.
        for u in &g.unreadable {
            if shown.insert(u.to_lowercase()) {
                v.game.push(SetupRow {
                    setting: u.clone(),
                    now: "Couldn't read".into(),
                    suggested: "Check it in the game".into(),
                    why: "A game update may have renamed it. Preflight's profile needs an update"
                        .into(),
                    tag: Tag::Optional,
                    menu: format!("{} › Settings", g.name),
                    issue_id: None,
                });
            }
        }
        if g.verified.starts_with("Unverified") && !v.game.is_empty() {
            v.game_note = Some(format!(
                "Preflight's {} profile is new and not checked on a real PC yet.",
                g.name
            ));
        }
        if v.game.is_empty() {
            v.game_note = Some(if g.files.iter().any(|f| f.read_ok) {
                "Preflight read this game's settings but none of the ones it checks were found."
                    .into()
            } else {
                "Not launched on this PC yet, so there are no settings to read.".into()
            });
        }
    } else if let Some(i) = gpu_issue {
        let guess = c.test.and_then(|t| t.game_guess.clone());
        v.game_name = Some(
            guess
                .map(|g| format!("Your game ({g})"))
                .unwrap_or_else(|| "Your game".into()),
        );
        v.game_note = Some(
            "Preflight doesn't know this game's settings yet, so it can't show what's set now."
                .into(),
        );
        for o in &i.options {
            v.game.push(SetupRow {
                setting: o.name.clone(),
                now: "Not readable".into(),
                suggested: if o.suggested {
                    "Do this first".into()
                } else {
                    "If still maxed".into()
                },
                why: o.gain.clone(),
                tag: if st.kept_issues.contains(&i.id) {
                    Tag::Kept
                } else if o.suggested && !is_optional(i) {
                    Tag::Change
                } else {
                    Tag::Optional
                },
                menu: o.menu.clone(),
                issue_id: Some(i.id.clone()),
            });
        }
    }

    // ---- Every other issue gets a row too, so no fix is hidden.
    let covered: std::collections::HashSet<String> = v
        .obs
        .iter()
        .chain(v.game.iter())
        .filter_map(|r| r.issue_id.clone())
        .collect();
    for i in issues.iter().filter(|i| !covered.contains(&i.id)) {
        let sug = i.options.iter().find(|o| o.suggested).or(i.options.first());
        let row = SetupRow {
            setting: row_name(&i.id).into(),
            now: i.title.trim_end_matches('.').to_string(),
            suggested: sug
                .map(|o| o.name.clone())
                .unwrap_or_else(|| "See the fix".into()),
            why: impact_of(&i.id).into(),
            tag: tag_for(Some(i), false, st),
            menu: sug.map(|o| o.menu.clone()).unwrap_or_default(),
            issue_id: Some(i.id.clone()),
        };
        match place_of(&i.id) {
            Place::Pc => v.pc.push(row),
            Place::Game => v.game.push(row),
            Place::Obs => v.obs.push(row),
        }
    }

    // ---- Summary
    let changes: Vec<&SetupRow> = v
        .game
        .iter()
        .chain(v.obs.iter())
        .chain(v.pc.iter())
        .filter(|r| r.tag == Tag::Change)
        .collect();
    // Counts come from the fix list itself, so every screen says the same number.
    let open_issues: Vec<&Issue> = issues
        .iter()
        .filter(|i| !st.kept_issues.contains(&i.id))
        .collect();
    let optional = open_issues.iter().filter(|i| is_optional(i)).count();
    let important = open_issues.len() - optional;
    v.summary = if important == 0 {
        if optional > 0 {
            format!(
                "Everything important is set well. {optional} optional improvement{} below.",
                if optional == 1 { "" } else { "s" }
            )
        } else {
            "Everything is set well for a smooth stream and recording.".into()
        }
    } else {
        let names: Vec<String> = changes
            .iter()
            .take(3)
            .map(|r| {
                let s = r.suggested.to_lowercase();
                if r.suggested.starts_with("Cap at") {
                    format!(
                        "cap {} at {}",
                        v.game_name.clone().unwrap_or_default(),
                        &r.suggested[7..]
                    )
                } else {
                    format!("{} to {}", r.setting.to_lowercase(), s)
                }
            })
            .collect();
        let more = important.saturating_sub(names.len());
        format!(
            "To stream and record without stutter, change {} thing{}: {}{}.{}",
            important,
            if important == 1 { "" } else { "s" },
            names.join("; "),
            if more > 0 {
                format!("; and {more} more below")
            } else {
                String::new()
            },
            if optional > 0 {
                format!(" {optional} optional.")
            } else {
                " Everything else is fine.".into()
            }
        )
    };
    v
}

// ---------------------------------------------------------------------------

fn encoder(c: &Ctx, a: &mut Analysis, issues: &mut Vec<Issue>) {
    let Some(o) = c.obs else {
        a.checks.push(Check {
            name: "Encoder".into(),
            status: Status::Unknown,
            label: "Not connected".into(),
            metric: "—".into(),
            note: "Connect OBS in Settings › OBS connection to check your encoder.".into(),
            measured: false,
        });
        return;
    };
    let hw_enc = c.hw.and_then(|h| h.encoders.first().cloned());
    let cpu_enc = o.encoder_kind.contains("CPU");
    let mut status = Status::Ok;
    let mut note = format!("{} · {}", o.encoder_kind, o.codec);
    if cpu_enc {
        if let Some(he) = &hw_enc {
            status = Status::Bad;
            note = format!("OBS encodes on your CPU, but your graphics card has {he}.");
            let mut opts = vec![
                opt(
                    "hw",
                    format!("Switch to {he}"),
                    "OBS › Settings › Output › Video Encoder",
                    "Frees your CPU for the game and OBS",
                    "Very little on recent cards",
                ),
                opt(
                    "faster",
                    "Keep x264, use a faster preset",
                    "OBS › Settings › Output › CPU Usage Preset (Advanced mode)",
                    "Lighter CPU load, keeps x264",
                    "Blockier image in fast motion",
                ),
            ];
            let strong_cpu = c.hw.map(|h| h.cpu_threads >= 16).unwrap_or(false);
            let keep_x264 = c.st.style == Style::Cinematic
                && strong_cpu
                && c.test.map(|t| t.cpu_avg < 50.0).unwrap_or(false);
            suggest(&mut opts, if keep_x264 { "faster" } else { "hw" });
            issues.push(Issue {
                id: "encoder".into(),
                priority: Priority::High,
                area: "OBS".into(),
                title: format!("OBS is encoding your stream on the CPU, using {}.", o.encoder_kind.replace(" (CPU)", "")),
                why: format!("Your graphics card has {he}, which does this almost for free. Switching means the game and the stream stop competing for your CPU."),
                who: "Anyone with an NVIDIA GTX 16-series or RTX card, or a recent AMD or Intel GPU.".into(),
                learn: "x264 can look slightly better at the same bitrate, but it uses a lot of CPU. In games that also need the CPU, both the game and the stream suffer. Modern GPU encoders are close to x264 quality at almost no cost to the game.".into(),
                measured: false,
                options: opts,
            });
        } else {
            status = Status::Warn;
            note = "OBS encodes on your CPU and no hardware encoder was found.".into();
        }
    }
    a.checks.push(Check {
        name: "Encoder".into(),
        status,
        label: match status {
            Status::Ok => "Good".into(),
            Status::Bad => "Wrong choice".into(),
            _ => "Check".into(),
        },
        metric: o.encoder_kind.clone(),
        note,
        measured: o.connected,
    });

    // A deliberate separate GPU recording encoder is fine (the "Recording
    // encoder" row says so). The Test step offers the stream + record test
    // for it, so it isn't a fix to make.
    let deliberate_sep = !o.rec_encoder_kind.is_empty() && !o.rec_encoder_kind.contains("CPU");
    if o.recording_uses_stream_settings == Some(false) && !deliberate_sep {
        let advanced = o.output_mode.eq_ignore_ascii_case("advanced");
        let mut opts = vec![opt(
            "match",
            if advanced {
                "Record with the stream encoder"
            } else {
                "Set recording quality to \"Same as stream\""
            },
            if advanced {
                "OBS › Settings › Output › Recording › Video Encoder › (Use stream encoder)"
            } else {
                "OBS › Settings › Output › Recording Quality › Same as stream"
            },
            "Test results reflect your real stream",
            "Your normal recordings use stream quality while it's set",
        )];
        suggest(&mut opts, "match");
        let deliberate = !o.rec_encoder_kind.is_empty() && !o.rec_encoder_kind.contains("CPU");
        issues.push(Issue {
            id: "recording_mismatch".into(),
            priority: if deliberate { Priority::Low } else { Priority::Medium },
            area: "OBS".into(),
            title: "Your recording doesn't use your stream's settings.".into(),
            why: format!("{} The test measures a recording, so it only matches your real stream when the recording uses the same settings. If you record separately on purpose, the stream + record test on the Test step measures both together.", o.recording_note),
            who: "Only matters for the full test.".into(),
            learn: "In Advanced output mode, OBS can record with a different encoder, bitrate or resolution than it streams with. Switch recording to the stream encoder before a test, and switch back afterwards if you like.".into(),
            measured: false,
            options: opts,
        });
    }
}

// ---------------------------------------------------------------------------
// Stream quality settings: rate control, keyframes, preset, downscale filter.

fn stream_quality(c: &Ctx, a: &mut Analysis, issues: &mut Vec<Issue>) {
    let Some(o) = c.obs else { return };
    if o.encoder_raw.is_empty() {
        return;
    }
    if o.multitrack == Some(true) {
        a.checks.push(Check {
            name: "Stream quality settings".into(),
            status: Status::Ok,
            label: "Automatic".into(),
            metric: "Set by Enhanced Broadcasting".into(),
            note: format!("With Enhanced Broadcasting on, {} picks the bitrate, preset and keyframes for you.", c.st.platform.name()),
            measured: false,
        });
        return;
    }
    let advanced = o.output_mode.eq_ignore_ascii_case("advanced");
    let plat = c.st.platform.name();
    let enc_menu = "OBS › Settings › Output › Streaming";
    let before = issues.len();
    let mut parts: Vec<String> = Vec::new();

    // Rate control
    let rc = o.rate_control.to_uppercase();
    if !rc.is_empty() {
        parts.push(rc.clone());
    }
    let no_cap = matches!(
        rc.as_str(),
        "CQP" | "CRF" | "ICQ" | "LOSSLESS" | "CQVBR" | "QVBR"
    );
    if advanced && (no_cap || rc == "VBR") {
        let mut opts = vec![opt(
            "cbr",
            "Switch to CBR",
            format!("{enc_menu} › Rate Control › CBR"),
            "Steady bitrate that platforms expect",
            "Very simple scenes use the full bitrate anyway",
        )];
        suggest(&mut opts, "cbr");
        issues.push(Issue {
            id: "rate_control".into(),
            priority: if no_cap { Priority::High } else { Priority::Medium },
            area: "OBS".into(),
            title: format!("Your stream uses {rc} rate control instead of CBR."),
            why: if no_cap {
                format!("{rc} keeps quality constant with no bitrate limit, so busy scenes can jump far above what your internet or {plat} allows. That causes dropped frames and buffering for viewers.")
            } else {
                format!("VBR lets the bitrate rise in busy scenes. Spikes above your normal bitrate can cause dropped frames, and {plat} recommends CBR.")
            },
            who: "Everyone streaming live. CQP and VBR are meant for recordings.".into(),
            learn: "CBR (constant bitrate) sends the same amount of data every second. Live platforms are built around that, and it makes your upload predictable. Quality-based modes are great for recordings, where file size doesn't matter.".into(),
            measured: false,
            options: opts,
        });
    }

    // Keyframe interval
    if let Some(k) = o.keyint_sec {
        parts.push(if k == 0 {
            "auto keyframes".into()
        } else {
            format!("{k} s keyframes")
        });
        let ignoring = o.ignore_recommended == Some(true);
        if advanced && k != 2 && k != 1 {
            let mut opts = vec![opt(
                "two",
                "Set it to 2 seconds",
                format!("{enc_menu} › Keyframe Interval › 2 s"),
                format!("Matches what {plat} wants"),
                "Nothing noticeable",
            )];
            suggest(&mut opts, "two");
            let (title, why) = if k == 0 {
                (
                    "Your keyframe interval is on auto, not 2 seconds.".to_string(),
                    if ignoring {
                        format!(
                            "Auto usually means a keyframe every 4 seconds or more, and you've told OBS to ignore {plat}'s recommendations. {plat} wants one every 2 seconds, or viewers can get longer buffering and a delay when they join."
                        )
                    } else {
                        format!(
                            "OBS usually fixes this to 2 seconds for {plat} on its own, unless you use a custom server. Setting 2 seconds yourself removes the guesswork."
                        )
                    },
                )
            } else {
                (
                    format!("Your keyframe interval is {k} seconds; {plat} wants 2."),
                    format!(
                        "Longer gaps between keyframes mean longer buffering and a slower start when viewers join{}.",
                        if plat == "YouTube" {
                            ". YouTube accepts up to 4 seconds, but 2 is recommended"
                        } else {
                            ", and the stream can be rejected or show warnings"
                        }
                    ),
                )
            };
            issues.push(Issue {
                id: "keyframes".into(),
                priority: if k == 0 && !ignoring { Priority::Low } else { Priority::Medium },
                area: "OBS".into(),
                title,
                why,
                who: "Everyone streaming to Twitch, YouTube or Kick.".into(),
                learn: "A keyframe is a full picture; the frames in between only store changes. Viewers can only start watching, and the platform can only switch quality, at a keyframe. Every 2 seconds is the standard for live streaming.".into(),
                measured: false,
                options: opts,
            });
        }
    }

    // Encoder preset
    if !o.preset.is_empty() {
        parts.push(preset_label(&o.preset));
        let skipping = c
            .test
            .map(|t| t.output_total > 0 && t.output_skipped as f32 / t.output_total as f32 > 0.005)
            .unwrap_or(false);
        let cpu_busy = c.test.map(|t| t.cpu_avg >= 70.0).unwrap_or(false);
        let p = o.preset.to_lowercase();
        let kind = o.encoder_kind.as_str();
        let menu = if advanced {
            format!("{enc_menu} › Preset")
        } else {
            "OBS › Settings › Output › Encoder Preset".to_string()
        };
        let x264_menu = if advanced {
            format!("{enc_menu} › CPU Usage Preset")
        } else {
            "OBS › Settings › Output › Encoder Preset".to_string()
        };
        let mut found: Option<(Priority, String, String, Vec<Opt>, &str)> = None;
        if kind.contains("x264") {
            match p.as_str() {
                "ultrafast" | "superfast" => {
                    found = Some((
                        Priority::Medium,
                        format!("x264 is set to the {p} preset, the lowest quality."),
                        "It saves CPU, but fast motion turns blocky at the same bitrate.".into(),
                        vec![opt(
                            "veryfast",
                            "Use veryfast",
                            format!("{x264_menu} › veryfast"),
                            "Noticeably cleaner picture",
                            "A little more CPU",
                        )],
                        "veryfast",
                    ));
                }
                "slow" | "slower" | "veryslow" | "placebo" => {
                    found = Some((Priority::High, format!("x264 is set to the {p} preset, which is too heavy for live streaming."), "On most CPUs it can't keep up in real time while you play, so OBS skips frames.".into(), vec![opt("veryfast", "Use veryfast or faster", format!("{x264_menu} › veryfast"), "Encoder keeps up", "Slightly softer detail")], "veryfast"));
                }
                "fast" | "medium" if skipping || cpu_busy => {
                    found = Some((Priority::Medium, format!("x264 is set to {p}, and your CPU struggled during the test."), "Heavier presets look a bit better but need a lot more CPU. With the game also using it, the encoder fell behind.".into(), vec![opt("veryfast", "Use veryfast", format!("{x264_menu} › veryfast"), "Much lighter on the CPU", "Slightly softer detail")], "veryfast"));
                }
                _ => {}
            }
        } else if kind.contains("NVENC") {
            match p.as_str() {
                "p1" | "p2" | "hp" | "llhp" | "ll" => {
                    found = Some((Priority::Low, format!("NVENC is set to a fast preset ({}), which looks worse than it needs to.", preset_label(&p)), "P4 or P5 looks noticeably cleaner and still costs your game almost nothing.".into(), vec![opt("p5", "Use P5: Slow (Good Quality)", format!("{menu} › P5"), "Cleaner picture at the same bitrate", "Barely any extra GPU")], "p5"));
                }
                "p6" | "p7" if skipping => {
                    found = Some((Priority::Medium, format!("NVENC is set to {}, and OBS skipped frames in your test.", preset_label(&p)), "The slowest presets add work for the encoder. With a busy GPU, they can make it fall behind.".into(), vec![opt("p5", "Use P5: Slow (Good Quality)", format!("{menu} › P5"), "Encoder keeps up", "A tiny bit of detail")], "p5"));
                }
                _ => {}
            }
        } else if (kind.contains("AMD") || kind.contains("Intel"))
            && (p == "speed" || p == "tu7" || p == "tu6")
        {
            found = Some((
                Priority::Low,
                format!(
                    "{kind} is set to its fastest preset ({}).",
                    preset_label(&p)
                ),
                "Balanced or Quality looks cleaner at the same bitrate with little extra load."
                    .into(),
                vec![opt(
                    "balanced",
                    "Use Balanced or Quality",
                    menu.clone(),
                    "Cleaner picture",
                    "Slightly more GPU",
                )],
                "balanced",
            ));
        }
        if let Some((pri, title, why, mut opts, sug)) = found {
            suggest(&mut opts, sug);
            issues.push(Issue {
                id: "preset".into(),
                priority: pri,
                area: "OBS".into(),
                title,
                why,
                who: "Anyone who wants the best picture for their bitrate.".into(),
                learn: "The preset trades encoding work for quality. Slower presets squeeze more detail out of the same bitrate but need more processing. For live streaming, the middle presets are the sweet spot.".into(),
                measured: skipping || cpu_busy,
                options: opts,
            });
        }
    }

    // Downscale filter (only matters when OBS shrinks the picture)
    if let (Some(base), Some(out)) = (o.base_res, o.output_res)
        && base != out
        && out.1 < base.1
    {
        let f = o.scale_type.to_lowercase();
        parts.push(format!("{} downscale", filter_label(&f)));
        if f == "bilinear" {
            let big = base.1 as f32 / out.1 as f32 >= 1.9;
            let mut opts = vec![
                opt(
                    "lanczos",
                    "Lanczos (sharpest)",
                    "OBS › Settings › Video › Downscale Filter › Lanczos",
                    "Sharpest picture",
                    "A little more GPU work",
                ),
                opt(
                    "bicubic",
                    "Bicubic",
                    "OBS › Settings › Video › Downscale Filter › Bicubic",
                    "Sharper text and edges",
                    "Very little",
                ),
                opt(
                    "area",
                    "Area",
                    "OBS › Settings › Video › Downscale Filter › Area",
                    "Clean and smooth for big size drops",
                    "Slightly softer than Lanczos",
                ),
            ];
            suggest(
                &mut opts,
                if big {
                    "area"
                } else if c.st.q_game_stream() == 2 {
                    "lanczos"
                } else {
                    "bicubic"
                },
            );
            issues.push(Issue {
                    id: "downscale".into(),
                    priority: Priority::Low,
                    area: "OBS".into(),
                    title: format!("OBS shrinks {}p to {}p with the blurriest filter (Bilinear).", base.1, out.1),
                    why: "Text, HUDs and fine detail look softer than they need to. A better filter costs almost nothing.".into(),
                    who: "Anyone streaming at a lower resolution than their screen.".into(),
                    learn: "Your canvas (base resolution) is bigger than your stream (output resolution), so OBS shrinks every frame. Bilinear is the fastest and blurriest way to do it. Bicubic and Lanczos keep edges sharper; Area works well when the size drops by half or more.".into(),
                    measured: false,
                    options: opts,
                });
        }
    }

    let found_n = issues.len() - before;
    let worst = issues[before..].iter().map(|i| i.priority).min();
    a.checks.push(Check {
        name: "Stream quality settings".into(),
        status: match worst {
            None => Status::Ok,
            Some(Priority::High) => Status::Bad,
            _ => Status::Warn,
        },
        label: if found_n == 0 {
            "Good".into()
        } else {
            format!("{found_n} to change")
        },
        metric: if parts.is_empty() {
            "—".into()
        } else {
            parts.join(" · ")
        },
        note: if found_n == 0 {
            format!("Rate control, keyframes, preset and scaling are set the way {plat} expects.")
        } else {
            "See the issues below.".into()
        },
        measured: false,
    });
}

fn preset_label(p: &str) -> String {
    match p.to_lowercase().as_str() {
        "p1" => "P1: Fastest".into(),
        "p2" => "P2: Faster".into(),
        "p3" => "P3: Fast".into(),
        "p4" => "P4: Medium".into(),
        "p5" => "P5: Slow".into(),
        "p6" => "P6: Slower".into(),
        "p7" => "P7: Slowest".into(),
        "hq" => "Quality".into(),
        "llhq" => "Low-latency quality".into(),
        "hp" | "llhp" | "ll" => "Performance".into(),
        other => {
            let mut c = other.chars();
            match c.next() {
                Some(f) => format!("{}{}", f.to_uppercase(), c.as_str()),
                None => String::new(),
            }
        }
    }
}

fn filter_label(f: &str) -> &'static str {
    match f {
        "bilinear" => "Bilinear",
        "area" => "Area",
        "lanczos" => "Lanczos",
        _ => "Bicubic",
    }
}

// ---------------------------------------------------------------------------
// Picture quality: resolution for the bitrate, encoder quality options,
// separate full-resolution recording.

/// Bitrate an H.264 stream needs to stay clean at this size and frame rate.
/// 1080p60 with fast motion needs about 6000 kbps (Twitch's standard), and
/// the need grows a little slower than the pixel count.
pub fn needed_kbps(w: u32, h: u32, fps: f32, motion: &str, codec: &str) -> u32 {
    let px = (w as f64 * h as f64) / (1920.0 * 1080.0);
    let fps_f = (fps.clamp(24.0, 120.0) as f64 / 60.0).powf(0.7);
    let motion_f = match motion {
        "high" => 1.0,
        "low" => 0.7,
        _ => 0.85,
    };
    let codec_f = match codec.to_lowercase().as_str() {
        c if c.contains("av1") => 0.6,
        c if c.contains("hevc") || c.contains("265") => 0.7,
        _ => 1.0,
    };
    (6000.0 * px.powf(0.8) * fps_f * motion_f * codec_f) as u32
}

/// The biggest standard stream size (same shape as the canvas) that this
/// bitrate keeps clean.
pub fn best_height(
    base: (u32, u32),
    bitrate: u32,
    fps: f32,
    motion: &str,
    codec: &str,
    max_h: u32,
) -> (u32, u32) {
    let aspect = base.0 as f64 / base.1.max(1) as f64;
    let size = |h: u32| (((h as f64 * aspect) / 2.0).round() as u32 * 2, h);
    for h in [2160u32, 1440, 1080, 936, 720] {
        if h > base.1 || h > max_h {
            continue;
        }
        let (w, hh) = size(h);
        if needed_kbps(w, hh, fps, motion, codec) as f32 <= bitrate as f32 * 1.05 {
            return (w, hh);
        }
    }
    size(720.min(base.1))
}

fn picture_quality(c: &Ctx, a: &mut Analysis, issues: &mut Vec<Issue>) {
    let Some(o) = c.obs.filter(|o| o.connected || !o.encoder_raw.is_empty()) else {
        return;
    };
    let plat = c.st.platform;
    let pname = plat.name();
    let fps = o.fps.unwrap_or(60.0);
    let fps_n = fps.round() as u32;
    let target = target_game(c, true);
    // With several tested games, the stream setup suits the fastest one, so
    // OBS doesn't need changing per game.
    let mrank = |m: &str| match m {
        "high" => 3,
        "medium" => 2,
        "low" => 1,
        _ => 0,
    };
    let own = target
        .map(|g| g.motion.clone())
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| "medium".into());
    let motion = if mrank(&c.st.fleet_motion) > mrank(&own) {
        c.st.fleet_motion.clone()
    } else {
        own
    };
    let gname = target
        .map(|g| g.name.clone())
        .unwrap_or_else(|| "your game".into());
    // Does the GPU have room for heavier encoder settings?
    let skipped = c
        .test
        .map(|t| t.output_total > 0 && t.output_skipped as f32 / t.output_total as f32 > 0.001)
        .unwrap_or(false);
    let maxed_frac = c
        .test
        .map(|t| {
            let v: Vec<f32> = t.samples.iter().filter_map(|s| s.gpu).collect();
            if v.is_empty() {
                0.0
            } else {
                v.iter().filter(|g| **g >= 95.0).count() as f32 / v.len() as f32
            }
        })
        .unwrap_or(0.0);
    let headroom = !c.st.fleet_busy
        && c.test
            .map(|t| t.gpu_avg.map(|g| g < 85.0).unwrap_or(false) && maxed_frac < 0.10 && !skipped)
            .unwrap_or(false);
    let safe_upload = c.net.filter(|n| n.ok).map(|n| n.safe_video_kbps());
    let upload_text = |need: u32| match safe_upload {
        Some(s) if s >= need => format!("Your upload can carry it (safe up to about {s} kbps)"),
        Some(s) => format!(
            "Your upload is only safe up to about {s} kbps, so this needs a faster connection"
        ),
        None => {
            "Preflight hasn't measured your upload yet, so check it first (Quick scan)".to_string()
        }
    };
    let upload_ok = |need: u32| safe_upload.map(|s| s >= need).unwrap_or(false);

    // ---- Resolution for the bitrate
    if let (Some(b), Some(base), Some(out)) = (o.bitrate_kbps, o.base_res, o.output_res) {
        let multitrack = o.multitrack == Some(true);
        let max_h = if plat == crate::config::Platform::Kick {
            1080
        } else {
            2160
        };
        let need_now = needed_kbps(out.0, out.1, fps, &motion, &o.codec);
        let best = best_height(base, b, fps, &motion, &o.codec, max_h);
        let per_px = |r: (u32, u32)| b as f64 / (r.0 as f64 * r.1 as f64);
        let note: String;
        if multitrack {
            note =
                format!("Enhanced Broadcasting picks the stream sizes and bitrates for {pname}.");
        } else if out.1 > best.1 {
            let more = ((per_px(best) / per_px(out) - 1.0) * 100.0).round() as i64;
            let kick_cap = plat == crate::config::Platform::Kick && out.1 > 1080;
            let mut opts = Vec::new();
            let mut down = opt(
                "res_down",
                format!("Stream at {}p{fps_n}, keep your {}p canvas", best.1, base.1),
                "OBS › Settings › Video › Output (Scaled) Resolution",
                format!("About {more}% more data per pixel, so fast motion stays sharp"),
                "A little less fine detail in still scenes",
            )
            .with(change(
                "Output (Scaled) Resolution",
                "OBS › Settings › Video",
                Some(format!("{}x{}", out.0, out.1)),
                format!("{}x{}", best.0, best.1),
            ));
            if o.scale_type.to_lowercase() != "lanczos" {
                down = down.with(change(
                    "Downscale Filter",
                    "OBS › Settings › Video",
                    Some(filter_label(&o.scale_type.to_lowercase()).to_string()),
                    "Lanczos (sharpest)",
                ));
            }
            opts.push(down);
            let need_up = ((need_now as f32 / 500.0).ceil() * 500.0) as u32;
            let yt = plat == crate::config::Platform::YouTube;
            if yt {
                let to = need_up.clamp(12000, 30000);
                opts.push(
                    opt(
                        "bitrate_up",
                        format!("Raise your bitrate to {to} kbps"),
                        "OBS › Settings › Output › Video Bitrate",
                        "Keeps your resolution. YouTube recommends about 24 Mbps for 1440p60",
                        upload_text(to + 160),
                    )
                    .with(change(
                        "Video Bitrate",
                        "OBS › Settings › Output",
                        Some(format!("{b} kbps")),
                        format!("{to} kbps"),
                    )),
                );
            }
            if plat == crate::config::Platform::Twitch && out.1 >= 1440 {
                opts.push(
                    opt(
                        "eb",
                        "Turn on Enhanced Broadcasting",
                        "OBS › Settings › Stream › Enhanced Broadcasting",
                        "Twitch gets several versions, up to 1440p (about 9 Mbps in total)",
                        format!("For Affiliates and Partners. {}", upload_text(9000)),
                    )
                    .with(change(
                        "Enhanced Broadcasting",
                        "OBS › Settings › Stream",
                        Some("Off".into()),
                        "On",
                    )),
                );
            }
            let pick = if yt && upload_ok(need_up.max(12000) + 160) {
                "bitrate_up"
            } else {
                "res_down"
            };
            suggest(&mut opts, pick);
            if !issues.iter().any(|i| i.id == "bitrate_network") {
                let s = opts
                    .iter()
                    .find(|o| o.suggested)
                    .map(|o| o.id.clone())
                    .unwrap_or_default();
                a.recommended_output = if s == "bitrate_up" {
                    format!("{}p{fps_n} · {} kbps", out.1, need_up.clamp(12000, 30000))
                } else {
                    format!("{}p{fps_n} · {b} kbps", best.1)
                };
            }
            let ratio = need_now as f32 / b as f32;
            issues.push(Issue {
                id: "resolution_bitrate".into(),
                priority: if ratio >= 1.4 || kick_cap { Priority::Medium } else { Priority::Low },
                area: "OBS".into(),
                title: if kick_cap { format!("Kick streams top out at 1080p, but yours is set to {}p.", out.1) } else { format!("At {b} kbps, {}p{fps_n} turns blocky in fast motion.", out.1) },
                why: format!(
                    "{}p{fps_n} with {} motion like {gname} needs about {need_now} kbps to stay clean. {}p{fps_n} gets about {more}% more data per pixel from the same {b} kbps. Keep your canvas (base) at {}p: OBS shrinks it cleanly, and your own screen doesn't change.",
                    out.1,
                    if motion == "high" { "fast" } else if motion == "low" { "slow" } else { "moderate" },
                    best.1,
                    base.1
                ),
                who: format!("Anyone streaming to {pname} at {b} kbps."),
                learn: "Bitrate is shared by every pixel in every frame. A bigger picture spreads it thinner, and fast motion needs the most data, so busy moments break into blocks first. A slightly smaller, sharp stream looks better than a big, blocky one. Lowering the frame rate isn't the fix: viewers notice 30 FPS in games more than a little less detail.".into(),
                measured: false,
                options: opts,
            });
            note = format!(
                "At {b} kbps, {}p{fps_n} is the sharpest size for {} motion.",
                best.1, motion
            );
        } else if plat == crate::config::Platform::YouTube && out.1 >= 1440 {
            note = "1440p on YouTube gets a better encode on YouTube's side, so it looks sharper even to viewers watching at 1080p.".into();
        } else if out.1 < best.1 && out.1 < base.1 {
            note = format!(
                "You could stream at {}p{fps_n} at this bitrate if you want more detail.",
                best.1
            );
        } else {
            note = format!("{}p{fps_n} at {b} kbps suits {} motion.", out.1, motion);
        }
        a.checks.push(Check {
            name: "Picture for viewers".into(),
            status: if issues
                .iter()
                .any(|i| i.id == "resolution_bitrate" && i.priority == Priority::Medium)
            {
                Status::Warn
            } else {
                Status::Ok
            },
            label: if issues.iter().any(|i| i.id == "resolution_bitrate") {
                "Too big for the bitrate".into()
            } else {
                "Sharp".into()
            },
            metric: format!("{}p{fps_n} · {b} kbps", out.1),
            note,
            measured: false,
        });
    }

    // ---- In-game effects that cost stream bitrate (heads-up, not scored)
    if let Some(g) = target {
        let fx: Vec<&crate::games::CurrentSetting> = g
            .current
            .iter()
            .filter(|s| s.effect == "bitrate" && s.flagged)
            .collect();
        if !fx.is_empty() && o.multitrack != Some(true) {
            let mut o2 = opt(
                "fx_off",
                format!(
                    "Turn off {}",
                    fx.iter()
                        .map(|f| f.setting.to_lowercase())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                fx[0].menu.clone(),
                "A cleaner stream at the same bitrate",
                "Only a look you may like (your game still runs the same)",
            );
            for f in &fx {
                o2 = o2.with(change(
                    f.setting.clone(),
                    f.menu.clone(),
                    Some(f.value.clone()),
                    "Off",
                ));
            }
            let mut opts = vec![o2];
            suggest(&mut opts, "fx_off");
            issues.push(Issue {
                id: "bitrate_effects".into(),
                priority: Priority::Low,
                area: g.name.clone(),
                title: format!("{} {} on in {}: {} the stream's bitrate.", fx.iter().map(|f| f.setting.clone()).collect::<Vec<_>>().join(", "), if fx.len() == 1 { "is" } else { "are" }, g.name, if fx.len() == 1 { "it eats into" } else { "they eat into" }),
                why: "Film grain, motion blur, depth of field and chromatic aberration add fine noise or smear to every frame. The encoder spends bits on that instead of the game, so the stream looks blockier. This heads-up doesn't count against your scores.".into(),
                who: "Anyone streaming at a fixed bitrate (Twitch, Kick, YouTube).".into(),
                learn: "Encoders save bits by reusing what didn't change between frames. Grain changes every pixel every frame, and blur smears motion, so there's less to reuse.".into(),
                measured: false,
                options: opts,
            });
        }
    }

    // ---- NVENC quality options (scaled to how busy the GPU is)
    if o.encoder_kind.contains("NVENC") && o.multitrack != Some(true) {
        let menu = "OBS › Settings › Output › Streaming";
        let mut ch: Vec<Change> = Vec::new();
        let preset_issue = issues.iter().any(|i| i.id == "preset");
        let (want_preset, want_mp, want_mp_label) = if headroom {
            ("p7", "fullres", "Two Passes (Full Resolution)")
        } else {
            ("p5", "qres", "Two Passes (Quarter Resolution)")
        };
        // P6 and P7 look almost the same: with headroom, only presets below P6
        // are worth changing (to P7); otherwise below P5.
        let floor = if headroom { 6 } else { 5 };
        if !preset_issue && preset_rank(&o.preset).map(|r| r < floor).unwrap_or(false) {
            ch.push(change(
                "Preset",
                menu,
                Some(preset_label(&o.preset)),
                preset_label(want_preset),
            ));
        }
        let mp_rank = |m: &str| match m {
            "fullres" => 2,
            "qres" => 1,
            _ => 0,
        };
        if mp_rank(&o.enc_multipass) < mp_rank(want_mp) {
            let now = match o.enc_multipass.as_str() {
                "qres" => "Two Passes (Quarter Resolution)",
                "fullres" => "Two Passes (Full Resolution)",
                _ => "Single Pass",
            };
            ch.push(change(
                "Multipass Mode",
                menu,
                Some(now.into()),
                want_mp_label,
            ));
        }
        if !o.enc_tune.is_empty() && o.enc_tune != "hq" {
            let now = match o.enc_tune.as_str() {
                "ll" => "Low Latency",
                "ull" => "Ultra Low Latency",
                x => x,
            };
            ch.push(change("Tuning", menu, Some(now.into()), "High Quality"));
        }
        if o.enc_bframes == Some(0) {
            ch.push(change("Max B-frames", menu, Some("0".into()), "2"));
        }
        if matches!(o.enc_profile.as_str(), "baseline" | "main") {
            ch.push(change(
                "Profile",
                menu,
                Some(super_cap(&o.enc_profile)),
                "High",
            ));
        }
        if !ch.is_empty() {
            let mut opts = vec![Opt {
                changes: ch,
                ..opt(
                    "nvenc_hq",
                    "Use NVENC's high-quality options",
                    menu,
                    "A cleaner picture at the same bitrate",
                    if headroom {
                        "A little more encoder work; your GPU had room in the test"
                    } else {
                        "A little more encoder work"
                    },
                )
            }];
            suggest(&mut opts, "nvenc_hq");
            issues.push(Issue {
                id: "enc_quality".into(),
                priority: Priority::Low,
                area: "OBS".into(),
                title: "NVENC could give a cleaner picture at the same bitrate.".into(),
                why: if headroom {
                    "Your GPU had room to spare in the test, so the slowest preset and full-resolution two-pass are free quality.".into()
                } else if c.test.is_some() {
                    "Your GPU was busy in the test, so these are the lighter high-quality options. Preflight checks for encoding lag after you change them (run a Quick check).".into()
                } else {
                    "These are safe on most cards. Run a test and Preflight can tell whether your GPU has room for the slowest options.".into()
                },
                who: "NVIDIA users who want the most out of their bitrate.".into(),
                learn: "NVENC runs on its own chip, but heavier options still add work. Two-pass looks at each frame twice to spend bits where they matter; High Quality tuning and B-frames squeeze more detail from the same bitrate. Low-latency tuning only matters for game streaming services, not Twitch or YouTube.".into(),
                measured: c.test.is_some(),
                options: opts,
            });
        }
    }

    // ---- Recording at full resolution while the stream is downscaled
    if let (Some(base), Some(out)) = (o.base_res, o.output_res) {
        let has_hw = c.hw.map(|h| !h.encoders.is_empty()).unwrap_or(false);
        if out.1 < base.1 && o.rec_encoder_raw.is_empty() && has_hw && o.multitrack != Some(true) {
            let chips = crate::hardware::nvenc_chips(
                &c.hw
                    .and_then(|h| h.main_gpu())
                    .map(|g| g.name.clone())
                    .unwrap_or_default(),
            )
            .unwrap_or(1);
            let rec_menu = if o.output_mode.eq_ignore_ascii_case("advanced") {
                "OBS › Settings › Output › Recording"
            } else {
                "OBS › Settings › Output › Recording Quality"
            };
            let mut opts = vec![
                opt(
                    "rec_native",
                    format!("Record separately at {}p", base.1),
                    rec_menu,
                    format!(
                        "Your recordings keep the full {}p detail for YouTube uploads and clips",
                        base.1
                    ),
                    if chips >= 2 {
                        format!(
                            "Your card has {chips} encoder chips, so this doesn't slow the stream"
                        )
                    } else {
                        "Adds encoder work: use P4 or faster for the recording".to_string()
                    },
                )
                .with(change(
                    "Recording encoder",
                    rec_menu,
                    Some("Same as stream".into()),
                    "Separate GPU encoder, CQP 18–20",
                ))
                .with(change(
                    "Rescale output",
                    rec_menu,
                    None,
                    format!("Off ({}p)", base.1),
                )),
            ];
            suggest(&mut opts, "rec_native");
            issues.push(Issue {
                id: "rec_native".into(),
                priority: Priority::Low,
                area: "OBS".into(),
                title: format!("Your recordings are shrunk to {}p like the stream.", out.1),
                why: format!("The stream is downscaled to fit your bitrate, but recordings don't have that limit. Recording separately keeps the full {}p.", base.1),
                who: "Anyone who uploads recordings or makes clips.".into(),
                learn: "OBS can record with its own encoder settings while it streams. For the full test, Preflight's stream + record test measures both together.".into(),
                measured: false,
                options: opts,
            });
        }
    }
}

/// Every encode the setup runs (stream, separate recording, plugin outputs,
/// Source Record, extra platforms), against the card's encoder chips.
pub fn encode_list(o: &ObsSnapshot, extra_platforms: usize) -> Vec<(String, bool)> {
    // (what, uses the CPU)
    let mut v: Vec<(String, bool)> = vec![(
        "Stream".into(),
        o.encoder_kind.contains("CPU") || o.encoder_kind.contains("x264"),
    )];
    if o.recording_uses_stream_settings == Some(false) && !o.rec_encoder_kind.is_empty() {
        v.push((
            "Recording (its own encoder)".into(),
            o.rec_encoder_kind.contains("CPU") || o.rec_encoder_kind.contains("x264"),
        ));
    }
    for out in o
        .outputs
        .iter()
        .filter(|x| x.plugin && x.label != "Source Record" && x.kind != "virtualcam_output")
    {
        v.push((out.label.clone(), false));
    }
    for (i, sr) in o
        .source_records
        .iter()
        .filter(|s| s.enabled && s.mode != "Off")
        .enumerate()
    {
        v.push((
            format!("Source Record {} ({})", i + 1, sr.mode.to_lowercase()),
            sr.encoder.contains("CPU") || sr.encoder.contains("x264"),
        ));
    }
    // Extra platforms streamed through a plugin, when no plugin output shows them.
    let shown = o
        .outputs
        .iter()
        .filter(|x| x.plugin && x.label.starts_with("Extra stream"))
        .count();
    for i in shown..extra_platforms {
        v.push((format!("Extra stream {}", i + 1), false));
    }
    v
}

fn encode_budget(c: &Ctx, a: &mut Analysis, issues: &mut Vec<Issue>) {
    let Some(o) = c.obs.filter(|o| o.connected) else {
        return;
    };
    let extra = if c.st.multistream_how == "plugin" {
        c.st.platforms.len().saturating_sub(1)
    } else {
        0
    };
    let list = encode_list(o, extra);
    // Without a readable GPU the chip count is a guess: count, don't warn.
    let Some(gpu_name) = c.hw.and_then(|h| h.main_gpu()).map(|g| g.name.clone()) else {
        return;
    };
    let nvenc = o.encoder_kind.contains("NVENC");
    let chips = if nvenc {
        crate::hardware::nvenc_chips(&gpu_name).unwrap_or(1) as usize
    } else {
        1
    };
    let gpu_encodes = list.iter().filter(|(_, cpu)| !cpu).count();
    let cpu_encodes: Vec<&String> = list
        .iter()
        .filter(|(_, cpu)| *cpu)
        .map(|(n, _)| n)
        .collect();
    if list.len() > 1 || !o.plugins.is_empty() {
        a.checks.push(Check {
            name: "Encodes".into(),
            status: if gpu_encodes > chips * 2 {
                Status::Warn
            } else {
                Status::Ok
            },
            label: if gpu_encodes > chips * 2 {
                "More encodes than the chips handle well".into()
            } else {
                "Fits".into()
            },
            metric: format!(
                "{} encode{} · {chips} encoder chip{}",
                list.len(),
                if list.len() == 1 { "" } else { "s" },
                if chips == 1 { "" } else { "s" }
            ),
            note: format!(
                "{}{}",
                list.iter()
                    .map(|(n, _)| n.clone())
                    .collect::<Vec<_>>()
                    .join(", "),
                if o.plugins.is_empty() {
                    String::new()
                } else {
                    format!(". Plugins: {}", o.plugins.join(", "))
                }
            ),
            measured: false,
        });
    }
    // One chip comfortably runs two encodes. More than that per chip, or an
    // encoder that fell behind in the test with more encodes than chips, is a problem.
    let measured_lag = c
        .test
        .map(|t| {
            (t.output_total > 0 && t.output_skipped as f32 / t.output_total as f32 > 0.001)
                || t.enc_avg.map(|e| e >= 85.0).unwrap_or(false)
        })
        .unwrap_or(false);
    let over = gpu_encodes > chips * 2 || (gpu_encodes > chips && measured_lag);
    if over && nvenc {
        let mut opts = vec![
            opt(
                "share",
                "Let extra outputs share the stream's encoder",
                "Your plugin's settings (e.g. \"use stream encoder\")",
                "One encode instead of two for that output",
                "That output gets the stream's quality",
            ),
            opt(
                "fewer",
                "Turn off outputs you don't use",
                "OBS › your plugin, or the Source Record filter",
                "Frees an encoder chip",
                "Fewer separate files",
            ),
            opt(
                "lighter",
                "Use a faster preset for recordings (P4 or lower)",
                "OBS › Settings › Output › Recording",
                "Each recording uses less of the encoder",
                "Slightly bigger files at the same quality",
            ),
        ];
        suggest(
            &mut opts,
            if list.iter().any(|(n, _)| n.starts_with("Source Record")) {
                "fewer"
            } else {
                "share"
            },
        );
        issues.push(Issue {
            id: "encode_budget".into(),
            priority: if measured_lag { Priority::High } else { Priority::Medium },
            area: "OBS".into(),
            title: format!("Your setup runs {gpu_encodes} GPU encodes, but your card has {chips} encoder chip{}.", if chips == 1 { "" } else { "s" }),
            why: format!("{}. When there are more encodes than chips, they take turns, and the stream can skip frames.{}", list.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>().join(", "), if measured_lag { " Your test already showed skipped frames." } else { " Run the stream + record test to see whether it keeps up." }),
            who: "Anyone streaming and recording with plugins or to several platforms.".into(),
            learn: "Each encoder chip runs about two encodes comfortably; beyond that they take turns. Cards like the RTX 5090 have three chips, most have one or two.".into(),
            measured: measured_lag,
            options: opts,
        });
    }
    if !cpu_encodes.is_empty() {
        let mut opts = vec![opt(
            "gpu",
            "Switch them to the GPU encoder",
            "OBS › the output's or plugin's encoder setting",
            "Frees your CPU for the game",
            "Nothing noticeable",
        )];
        suggest(&mut opts, "gpu");
        issues.push(Issue {
            id: "cpu_encodes".into(),
            priority: Priority::Low,
            area: "OBS".into(),
            title: format!("{} {} on your CPU (x264).", cpu_encodes.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "), if cpu_encodes.len() == 1 { "encodes" } else { "encode" }),
            why: "CPU encoding competes with the game. Your GPU encoder does the same job almost for free.".into(),
            who: "Anyone with a GPU encoder.".into(),
            learn: "x264 can look great but needs a lot of CPU. NVENC, AMF and Quick Sync run on separate chips.".into(),
            measured: false,
            options: opts,
        });
    }
}

/// What each platform should get when it has its own encode.
pub fn platform_target(name: &str, base_h: u32, fps: u32, codecs: &[String]) -> (String, u32) {
    match name {
        "YouTube" if base_h >= 1440 => {
            if codecs.iter().any(|c| c == "AV1" || c == "HEVC") {
                (
                    format!(
                        "1440p{fps} · about 14000 kbps · {} (or H.264 at 24000)",
                        if codecs.iter().any(|c| c == "AV1") {
                            "AV1"
                        } else {
                            "HEVC"
                        }
                    ),
                    14000,
                )
            } else {
                (format!("1440p{fps} · 24000 kbps · H.264"), 24000)
            }
        }
        "YouTube" => (format!("1080p{fps} · 12000 kbps · H.264"), 12000),
        "Kick" => (format!("1080p{fps} · 8000 kbps · H.264"), 8000),
        _ => (format!("1080p{fps} · 6000 kbps · H.264"), 6000),
    }
}

/// Streaming to several platforms at once: one shared encode or one each,
/// the total upload, and what each platform should get.
fn multistream(c: &Ctx, a: &mut Analysis, issues: &mut Vec<Issue>) {
    let st = c.st;
    if st.platforms.len() < 2 {
        return;
    }
    let Some(o) = c.obs.filter(|o| o.connected || !o.encoder_raw.is_empty()) else {
        return;
    };
    let plugin_seen = o
        .outputs
        .iter()
        .any(|x| x.plugin && x.label.starts_with("Extra stream"));
    let how = if plugin_seen {
        "plugin"
    } else {
        st.multistream_how.as_str()
    };
    let fps = o.fps.map(|f| f.round() as u32).unwrap_or(60);
    let base_h = o.base_res.map(|b| b.1).unwrap_or(1080);
    let gpu = c.hw.and_then(|h| h.main_gpu()).cloned();
    let codecs = gpu
        .as_ref()
        .map(|g| crate::encoders::codecs_for(g.vendor, &g.name))
        .unwrap_or_else(|| vec!["H.264".into()]);
    let chips = if o.encoder_kind.contains("NVENC") {
        gpu.as_ref()
            .and_then(|g| crate::hardware::nvenc_chips(&g.name))
            .unwrap_or(1) as usize
    } else {
        1
    };
    let audio = o.audio_kbps.unwrap_or(160);
    let main_b = o.bitrate_kbps.unwrap_or(6000);
    let others: Vec<&String> = st
        .platforms
        .iter()
        .filter(|p| p.as_str() != st.platform.name())
        .collect();
    // Encodes besides the extra streams.
    let base_encodes = encode_list(o, 0).len();
    let separate_fits = base_encodes + others.len() <= chips;
    let separate_upload: u32 = main_b
        + audio
        + others
            .iter()
            .map(|p| platform_target(p, base_h, fps, &codecs).1 + audio)
            .sum::<u32>();
    let shared_upload: u32 = (main_b + audio) * (1 + others.len() as u32);
    let safe = c.net.filter(|n| n.ok).map(|n| n.safe_video_kbps());
    let up_ok = |k: u32| safe.map(|s| s >= k).unwrap_or(true);
    let names = st.platforms.join(", ");

    let mut opts = Vec::new();
    let mut sep = opt(
        "separate",
        "Give each platform its own encode",
        "Your multi-stream plugin › each platform's encoder settings",
        "Each platform gets its best quality",
        format!(
            "{} encodes on your {chips} encoder chip{} and about {} Mbps of upload",
            base_encodes + others.len(),
            if chips == 1 { "" } else { "s" },
            (separate_upload as f32 / 1000.0).round()
        ),
    )
    .with(change(
        format!("{} (main stream)", st.platform.name()),
        "OBS › Settings › Output",
        None,
        platform_target(st.platform.name(), base_h, fps, &codecs).0,
    ));
    for p in &others {
        sep = sep.with(change(
            p.to_string(),
            "Your multi-stream plugin › this platform",
            None,
            platform_target(p, base_h, fps, &codecs).0,
        ));
    }
    opts.push(sep);
    opts.push(opt(
        "shared",
        "Share one encode for every platform",
        "Your multi-stream plugin › \"use the stream encoder\"",
        "One encode, so it fits any card",
        format!(
            "Every platform gets {}'s limits ({} kbps)",
            st.platform.name(),
            main_b
        ),
    ));
    opts.push(opt(
        "service",
        "Use a restream service",
        "Restream or similar: OBS streams once to the service",
        "Only one upload, whatever your connection",
        "Every platform gets the same stream; the service may need an account",
    ));
    let pick = if how == "service" {
        "service"
    } else if separate_fits && up_ok(separate_upload) {
        "separate"
    } else if up_ok(shared_upload) {
        "shared"
    } else {
        "service"
    };
    suggest(&mut opts, pick);

    let upload_bad = how == "plugin"
        && !up_ok(if pick == "separate" {
            separate_upload
        } else {
            shared_upload
        });
    a.checks.push(Check {
        name: "Several platforms".into(),
        status: if upload_bad { Status::Warn } else { Status::Ok },
        label: match pick {
            "separate" => "One encode each".into(),
            "shared" => "One shared encode".into(),
            _ => "Through a restream service".into(),
        },
        metric: names.clone(),
        note: match (how, safe) {
            ("service", _) => "A restream service uploads once, so your connection only carries one stream.".into(),
            (_, Some(s)) => format!("Upload needed: about {:.0} Mbps for separate encodes, {:.0} Mbps shared. Yours is safe up to about {:.0} Mbps.", separate_upload as f32 / 1000.0, shared_upload as f32 / 1000.0, s as f32 / 1000.0),
            _ => "Run the network test to check your upload for several streams.".into(),
        },
        measured: safe.is_some(),
    });
    issues.push(Issue {
        id: "multistream".into(),
        priority: if upload_bad || (pick == "separate" && how == "plugin" && !separate_fits) { Priority::Medium } else { Priority::Low },
        area: "OBS".into(),
        title: match pick {
            "separate" => format!("You stream to {names}: your PC can give each its own encode."),
            "shared" => format!("You stream to {names}: share one encode between them."),
            _ => format!("You stream to {names}: a restream service suits your setup best."),
        },
        why: match pick {
            "separate" => format!("With one shared encode every platform gets {}'s limits. Your {chips} encoder chip{} and your upload can carry one encode per platform, so YouTube can get a sharper stream.", st.platform.name(), if chips == 1 { "" } else { "s" }),
            "shared" => "Separate encodes would need more encoder chips or upload than you have. One shared encode keeps every stream smooth.".into(),
            _ => "Your upload can't carry a stream per platform. A restream service takes one stream and sends it to each platform for you.".into(),
        },
        who: "Anyone streaming to more than one platform at once.".into(),
        learn: "A multi-stream plugin sends each platform its own upload (and can give each its own encode). A restream service takes one upload and copies it. Test each platform in its private mode with the stream + record test.".into(),
        measured: false,
        options: opts,
    });
}

/// Stream audio: bitrate and sample rate (settings only; audio is never captured).
fn audio_settings(c: &Ctx, issues: &mut Vec<Issue>) {
    let Some(o) = c.obs else { return };
    let plat = c.st.platform;
    let pname = plat.name();
    if let Some(k) = o.audio_kbps {
        let advanced = o.output_mode.eq_ignore_ascii_case("advanced");
        let menu = if advanced {
            "OBS › Settings › Output › Audio › Track 1 › Audio Bitrate"
        } else {
            "OBS › Settings › Output › Audio Bitrate"
        };
        let too_low = k < 128;
        let too_high = plat == crate::config::Platform::Twitch && k > 160;
        if too_low || too_high {
            let mut opts = vec![
                opt(
                    "a160",
                    "Set it to 160 kbps",
                    menu,
                    if too_low {
                        "Clear voice and game sound"
                    } else {
                        "Matches what Twitch expects"
                    },
                    "Nothing noticeable",
                )
                .with(change(
                    "Audio Bitrate",
                    menu,
                    Some(format!("{k} kbps")),
                    "160 kbps",
                )),
            ];
            suggest(&mut opts, "a160");
            issues.push(Issue {
                id: "audio_bitrate".into(),
                priority: Priority::Low,
                area: "OBS".into(),
                title: if too_low { format!("Your stream audio is only {k} kbps.") } else { format!("Your stream audio is {k} kbps; {pname} expects up to 160.") },
                why: if too_low { "Below 128 kbps, voices and music start to sound swirly or muffled, especially with game sound underneath.".into() } else { "Higher audio bitrates don't sound better on Twitch and can cause the stream to be re-encoded.".into() },
                who: "Everyone streaming.".into(),
                learn: "160 kbps AAC is the standard for streaming: clear voice and game audio at a small cost in bitrate.".into(),
                measured: false,
                options: opts,
            });
        }
    }
    if let Some(sr) = o.sample_rate.filter(|r| *r != 48000) {
        let mut opts = vec![
            opt(
                "48k",
                "Use 48 kHz",
                "OBS › Settings › Audio › Sample Rate",
                "Matches most mics, headsets and game audio, so nothing is resampled",
                "Restart OBS once",
            )
            .with(change(
                "Sample Rate",
                "OBS › Settings › Audio",
                Some(format!("{:.1} kHz", sr as f32 / 1000.0)),
                "48 kHz",
            )),
        ];
        suggest(&mut opts, "48k");
        issues.push(Issue {
            id: "sample_rate".into(),
            priority: Priority::Low,
            area: "OBS".into(),
            title: format!("OBS runs audio at {:.1} kHz instead of 48 kHz.", sr as f32 / 1000.0),
            why: "Most mics, headsets and games work at 48 kHz. A different rate means OBS converts everything, which can add crackles or drift on some setups.".into(),
            who: "Everyone streaming.".into(),
            learn: "Sample rate is how many times per second sound is measured. 48 kHz is the standard for video.".into(),
            measured: false,
            options: opts,
        });
    }
}

// ---------------------------------------------------------------------------
// Recording while streaming, extra encoder jobs, disk.

fn recording(c: &Ctx, a: &mut Analysis, issues: &mut Vec<Issue>) {
    let Some(o) = c.obs else { return };
    let gpu_name =
        c.hw.and_then(|h| h.main_gpu())
            .map(|g| g.name.clone())
            .unwrap_or_default();
    let chips = crate::hardware::nvenc_chips(&gpu_name);
    let enc_busy = c
        .test
        .and_then(|t| t.enc_avg)
        .map(|e| e >= 85.0)
        .unwrap_or(false);
    let enc_lag = c
        .test
        .map(|t| t.output_total > 0 && t.output_skipped as f32 / t.output_total as f32 > 0.005)
        .unwrap_or(false);
    let separate = !o.rec_encoder_raw.is_empty();
    let advanced = o.output_mode.eq_ignore_ascii_case("advanced");
    let rec_menu = if advanced {
        "OBS › Settings › Output › Recording"
    } else {
        "OBS › Settings › Output › Recording Quality"
    };

    // A separate recording encoder: advice for each side.
    if separate {
        let rk = &o.rec_encoder_kind;
        let share = opt(
            "share",
            if advanced {
                "Record with the stream encoder"
            } else {
                "Set recording quality to \"Same as stream\""
            },
            if advanced {
                format!("{rec_menu} › Video Encoder › (Use stream encoder)")
            } else {
                format!("{rec_menu} › Same as stream")
            },
            "No extra encoding at all",
            "The recording is capped at your stream bitrate",
        );
        if rk.contains("CPU") && !o.encoder_kind.contains("CPU") {
            let mut opts = vec![
                share.clone(),
                opt(
                    "gpu",
                    format!("Record with {}", o.encoder_kind),
                    format!("{rec_menu} › Video Encoder"),
                    "Your CPU stays free for the game",
                    "Slightly bigger files at the same quality",
                ),
            ];
            suggest(&mut opts, "gpu");
            issues.push(Issue {
                id: "rec_encoder".into(),
                priority: if c.test.map(|t| t.cpu_avg >= 70.0).unwrap_or(false) { Priority::High } else { Priority::Medium },
                area: "OBS".into(),
                title: "Your recording uses x264 on the CPU, alongside your game.".into(),
                why: "Recording with x264 while you play makes the game, OBS and the recording fight over your CPU. Your graphics card's encoder can record almost for free.".into(),
                who: "Anyone who records and streams at the same time.".into(),
                learn: "x264 is a software encoder. A second x264 job on top of a game is one of the most common causes of stutter in both the game and the stream.".into(),
                measured: false,
                options: opts,
            });
        } else {
            let sr = preset_rank(&o.preset);
            let rr = preset_rank(&o.rec_preset);
            let one_chip = chips == Some(1) || (chips.is_none() && !o.encoder_kind.contains("CPU"));
            let rec_bigger = match (o.rec_res, o.output_res) {
                (Some(r), Some(s)) => r.1 > s.1,
                _ => false,
            };
            let heavy = match (sr, rr) {
                (Some(s), Some(r)) => (one_chip && r >= s.max(5)) || r >= 6,
                _ => false,
            };
            if heavy || enc_busy || enc_lag || (one_chip && rec_bigger) {
                let faster = if rk.contains("NVENC") {
                    "P4"
                } else if rk.contains("AMD") || rk.contains("Intel") {
                    "Balanced"
                } else {
                    "a faster preset"
                };
                let mut opts = vec![
                    share.clone(),
                    opt(
                        "faster",
                        format!("Keep it separate, use {faster} for recording"),
                        format!("{rec_menu} › Preset"),
                        "The recording barely looks different at its high bitrate",
                        "Slightly bigger files",
                    ),
                ];
                if rec_bigger {
                    opts.push(opt(
                        "same_res",
                        "Record at the stream's resolution",
                        format!("{rec_menu} › Rescale Output › off"),
                        "OBS scales once instead of twice",
                        "Recording is no sharper than the stream",
                    ));
                }
                suggest(
                    &mut opts,
                    if one_chip && (enc_busy || enc_lag) {
                        "share"
                    } else {
                        "faster"
                    },
                );
                let chip_text = match chips {
                    Some(1) => {
                        "Your card has one encoder chip, so the stream and recording share it."
                            .to_string()
                    }
                    Some(n) => format!(
                        "Your card has {n} encoder chips, which helps, but the slowest presets still add up."
                    ),
                    None => "Both encoding jobs share your graphics card's encoder.".to_string(),
                };
                let measured = c
                    .test
                    .and_then(|t| t.enc_avg)
                    .map(|e| format!(" During the test it was {e:.0}% busy."))
                    .unwrap_or_default();
                issues.push(Issue {
                    id: "rec_encoder".into(),
                    priority: if enc_busy || enc_lag { Priority::Medium } else { Priority::Low },
                    area: "OBS".into(),
                    title: format!("Your recording has its own encoder, at {}.", crate::analysis::preset_name(&o.rec_preset)),
                    why: format!("{chip_text}{measured} The stream is limited by its bitrate, so a slow preset helps it most. The recording has plenty of bitrate, so a faster preset looks the same there."),
                    who: "Anyone who records and streams at the same time.".into(),
                    learn: "Keep the slower preset (like P5) on the stream, where every bit counts, and use a faster one (like P4) for the recording. If the encoder still struggles, let the recording share the stream encoder.".into(),
                    measured: enc_busy || enc_lag,
                    options: opts,
                });
            }
        }
    }

    // Replay buffer running with its own encoder all stream long.
    if o.replay_active && separate {
        let mut opts = vec![opt(
            "off",
            "Turn the replay buffer off when you don't need it",
            "OBS › Controls › Stop Replay Buffer",
            "One less encoding job",
            "No instant replays",
        )];
        suggest(&mut opts, "off");
        issues.push(Issue {
            id: "replay_buffer".into(),
            priority: if enc_busy || enc_lag { Priority::Medium } else { Priority::Low },
            area: "OBS".into(),
            title: "Your replay buffer is running with the recording's own encoder.".into(),
            why: "That's a second encoding job for as long as it runs, even when you aren't recording.".into(),
            who: "Anyone using the replay buffer for clips.".into(),
            learn: "The replay buffer uses the recording settings. When the recording has its own encoder, the buffer keeps that encoder busy the whole time.".into(),
            measured: false,
            options: opts,
        });
    }

    // Enhanced Broadcasting: several versions of the stream at once.
    if o.multitrack == Some(true) {
        let mut opts = vec![
            opt(
                "keep",
                "Keep it on",
                "OBS › Settings › Stream › Enhanced Broadcasting",
                "Viewers get quality options (1080p, 720p, ...)",
                "Your GPU encodes several streams at once",
            ),
            opt(
                "off",
                "Turn it off",
                "OBS › Settings › Stream › Enhanced Broadcasting › off",
                "Much lighter on your encoder",
                "Viewers only get the one version you send",
            ),
        ];
        suggest(&mut opts, if enc_busy || enc_lag { "off" } else { "keep" });
        issues.push(Issue {
            id: "multitrack".into(),
            priority: if enc_busy || enc_lag { Priority::Medium } else { Priority::Low },
            area: "OBS".into(),
            title: "Enhanced Broadcasting is on, so your GPU encodes several versions of the stream.".into(),
            why: if enc_busy || enc_lag {
                "Your encoder struggled during the test. Enhanced Broadcasting can send up to 5 versions at once, which is a lot of extra work.".into()
            } else {
                format!("It's fine when your encoder has room. {} also picks your bitrate and preset while it's on.", c.st.platform.name())
            },
            who: "NVIDIA users on Twitch or YouTube.".into(),
            learn: "Enhanced Broadcasting encodes the extra quality levels on your PC instead of on the platform's servers. Viewers get quality options even if you aren't a partner, but your encoder does more work.".into(),
            measured: enc_busy || enc_lag,
            options: opts,
        });
    }

    // Measured: how busy the encoder chip was.
    if let Some(t) = c.test
        && let Some(e) = t.enc_avg.filter(|e| *e >= 85.0)
    {
        let mut opts = vec![
            opt(
                "faster",
                "Use a faster preset",
                "OBS › Settings › Output › Preset",
                "Less work per frame",
                "Slightly softer picture",
            ),
            opt(
                "lower",
                "Lower the output resolution or FPS",
                "OBS › Settings › Video",
                "Much less work",
                "Viewers get a smaller picture",
            ),
        ];
        if separate {
            opts.insert(
                0,
                opt(
                    "share",
                    "Let the recording share the stream encoder",
                    rec_menu,
                    "Removes a whole encoding job",
                    "Recording capped at stream bitrate",
                ),
            );
        }
        if o.multitrack == Some(true) {
            opts.insert(
                0,
                opt(
                    "eb_off",
                    "Turn off Enhanced Broadcasting",
                    "OBS › Settings › Stream",
                    "Removes several encoding jobs",
                    "No quality options for viewers",
                ),
            );
        }
        let first = opts[0].id.clone();
        suggest(&mut opts, &first);
        issues.push(Issue {
                id: "enc_load".into(),
                priority: if enc_lag { Priority::High } else { Priority::Medium },
                area: "OBS".into(),
                title: format!("Your encoder chip was {e:.0}% busy during the test, peaking at {:.0}%.", t.enc_max.unwrap_or(e)),
                why: "Above about 85%, it can fall behind in busy moments, and OBS skips frames on the stream and the recording.".into(),
                who: "Anyone streaming, recording or multistreaming at once.".into(),
                learn: "The video encoder is a separate part of your graphics card. Every stream version, separate recording and replay buffer adds to its work.".into(),
                measured: true,
                options: opts,
            });
    }

    // Recording format
    let f = o.rec_format.to_lowercase();
    if f == "mp4" || f == "mov" {
        let new_obs = o
            .obs_version
            .split('.')
            .next()
            .and_then(|m| m.parse::<u32>().ok())
            .map(|m| m >= 31)
            .unwrap_or(false)
            || o.obs_version.starts_with("30.2");
        let mut opts = vec![opt(
            "mkv",
            "MKV",
            format!("{rec_menu} › Recording Format › MKV"),
            "Survives crashes",
            "Remux to MP4 before editing (File › Remux Recordings)",
        )];
        if new_obs {
            opts.insert(
                0,
                opt(
                    "hybrid",
                    "Hybrid MP4",
                    format!("{rec_menu} › Recording Format › Hybrid MP4"),
                    "Survives crashes and opens everywhere",
                    "Needs OBS 30.2 or newer (you have it)",
                ),
            );
        }
        let first = opts[0].id.clone();
        suggest(&mut opts, &first);
        issues.push(Issue {
            id: "rec_format".into(),
            priority: Priority::Medium,
            area: "OBS".into(),
            title: format!("Your recordings are saved as plain {}.", f.to_uppercase()),
            why: "If OBS or your PC crashes, power cuts out, or the drive fills up, the whole recording is lost.".into(),
            who: "Anyone who keeps recordings.".into(),
            learn: "Plain MP4 only writes the index that makes the file playable at the very end. MKV and Hybrid MP4 stay playable up to the moment something goes wrong.".into(),
            measured: false,
            options: opts,
        });
    }

    // Disk space for recordings
    if let Some(free) = o.disk_free_mb {
        let (kbps, measured) = match c
            .test
            .and_then(|t| t.recording_size.map(|s| (s, t.seconds)))
        {
            Some((size, secs)) if secs > 30.0 => (size as f32 * 8.0 / 1000.0 / secs, true),
            _ if c.st.rec_kbps_seen.is_some() => (c.st.rec_kbps_seen.unwrap_or(6000.0), true),
            _ => {
                let est = if !separate {
                    o.bitrate_kbps.map(|b| b as f32)
                } else if let Some(b) = o.rec_bitrate_kbps.filter(|_| {
                    !o.rec_rate_control.to_uppercase().contains("CQ")
                        && !o.rec_rate_control.eq_ignore_ascii_case("CRF")
                }) {
                    Some(b as f32)
                } else {
                    let (w, h) = o.rec_res.or(o.output_res).unwrap_or((1920, 1080));
                    let fps = o.fps.unwrap_or(60.0);
                    Some(w as f32 * h as f32 * fps * 0.1 / 1000.0)
                };
                (est.unwrap_or(6000.0), false)
            }
        };
        let hours = free as f32 * 8.0 / (kbps / 1000.0).max(0.1) / 3600.0;
        a.checks.push(Check {
            name: "Recording space".into(),
            status: if hours < 1.5 {
                Status::Bad
            } else if hours < 3.0 {
                Status::Warn
            } else {
                Status::Ok
            },
            label: if hours < 3.0 {
                "Low".into()
            } else {
                "Plenty".into()
            },
            metric: format!("About {} of recording", fmt_hours(hours)),
            note: format!(
                "{} free, recording at about {:.0} Mbps{}.",
                crate::util::fmt_bytes((free * 1_048_576.0) as u64),
                kbps / 1000.0,
                if measured {
                    " (measured from your recordings)"
                } else {
                    " (estimated)"
                }
            ),
            measured,
        });
        if hours < 3.0 {
            let mut opts = vec![
                opt(
                    "clear",
                    "Free up space or move old recordings",
                    "Your file manager",
                    "Room for long streams",
                    "Time to sort old files",
                ),
                opt(
                    "folder",
                    "Record to a bigger drive",
                    format!("{rec_menu} › Recording Path"),
                    "Lots more room",
                    "Pick a fast drive",
                ),
            ];
            suggest(&mut opts, "clear");
            issues.push(Issue {
                id: "disk_space".into(),
                priority: if hours < 1.5 { Priority::Medium } else { Priority::Low },
                area: "PC".into(),
                title: format!("Your recording drive only has room for about {}.", fmt_hours(hours)),
                why: "If you record while streaming and the drive fills up, the recording stops (and plain MP4 files are lost).".into(),
                who: "Anyone who records their streams.".into(),
                learn: "OBS reports free space on the drive you record to. Preflight divides it by your recording bitrate.".into(),
                measured,
                options: opts,
            });
        }
    }

    // Recording to a hard drive (Linux can tell)
    if let Some(dir) = rec_folder(o, c.test)
        && crate::hardware::drive_is_hdd(&dir) == Some(true)
    {
        let same = c
            .games
            .iter()
            .filter(|g| g.running || !g.findings.is_empty())
            .flat_map(|g| g.files.iter())
            .filter_map(|f| f.real_path.clone())
            .any(|p| crate::hardware::same_drive(&p, &dir));
        let mut opts = vec![opt(
            "ssd",
            "Record to an SSD",
            format!("{rec_menu} › Recording Path"),
            "No dropped recording frames",
            "Needs free SSD space",
        )];
        suggest(&mut opts, "ssd");
        issues.push(Issue {
                id: "slow_disk".into(),
                priority: Priority::Low,
                area: "PC".into(),
                title: "Your recordings go to a hard drive, not an SSD.".into(),
                why: if same {
                    "It's also the drive your game loads from, so loading screens and the recording compete for it.".into()
                } else {
                    "Hard drives can fall behind with high-bitrate recordings, and OBS then drops recording frames.".into()
                },
                who: "Anyone recording at high quality.".into(),
                learn: "SSDs have no moving parts and handle recording and game loading at the same time easily.".into(),
                measured: false,
                options: opts,
            });
    }
}

fn rec_folder(o: &ObsSnapshot, t: Option<&TestResult>) -> Option<std::path::PathBuf> {
    if let Some(p) = t.and_then(|t| t.recording_path.as_ref()) {
        return std::path::Path::new(p).parent().map(|p| p.to_path_buf());
    }
    if !o.rec_dir.is_empty() {
        return Some(std::path::PathBuf::from(&o.rec_dir));
    }
    None
}

fn fmt_hours(h: f32) -> String {
    if h >= 10.0 {
        format!("{:.0} hours", h)
    } else if h >= 1.0 {
        format!("{:.1} hours", h)
    } else {
        format!("{:.0} minutes", h * 60.0)
    }
}

pub fn preset_name(p: &str) -> String {
    match p.to_lowercase().as_str() {
        "p1" | "p2" | "p3" | "p4" | "p5" | "p6" | "p7" => p.to_uppercase(),
        "" => "its default preset".into(),
        other => {
            let mut c = other.chars();
            c.next()
                .map(|f| format!("{}{}", f.to_uppercase(), c.as_str()))
                .unwrap_or_default()
        }
    }
}

// ---------------------------------------------------------------------------
// "Your recommended OBS settings"

fn recommended(c: &Ctx, a: &Analysis) -> Vec<RecRow> {
    let Some(o) = c.obs else { return Vec::new() };
    let mut v = Vec::new();
    let advanced = o.output_mode.eq_ignore_ascii_case("advanced");
    let out = "OBS › Settings › Output";
    let hw = c.hw.and_then(|h| h.encoders.first().cloned());
    let mut row = |setting: &str, yours: String, suggested: String, menu: String, ok: bool| {
        v.push(RecRow {
            setting: setting.into(),
            yours,
            suggested,
            menu,
            ok,
        });
    };
    // Encoder
    let enc_ok = !o.encoder_kind.contains("CPU") || hw.is_none();
    row(
        "Encoder",
        o.encoder_kind.clone(),
        hw.clone().unwrap_or_else(|| o.encoder_kind.clone()),
        format!("{out} › Video Encoder"),
        enc_ok,
    );
    if o.multitrack == Some(true) {
        row(
            "Bitrate, preset, keyframes",
            "Set by Enhanced Broadcasting".into(),
            "Automatic".into(),
            "OBS › Settings › Stream".into(),
            true,
        );
    } else {
        // Bitrate
        let limits = crate::network::limits(c.st.platform);
        let cap = c
            .net
            .filter(|n| n.ok)
            .map(|n| n.safe_video_kbps().min(limits.max_kbps_1080p60))
            .unwrap_or(limits.max_kbps_1080p60);
        let cur = o.bitrate_kbps;
        row(
            "Bitrate",
            cur.map(|b| format!("{b} kbps"))
                .unwrap_or_else(|| "unknown".into()),
            format!("up to {cap} kbps"),
            format!("{out} › Video Bitrate"),
            cur.map(|b| b <= cap).unwrap_or(false),
        );
        let rc = o.rate_control.to_uppercase();
        row(
            "Rate control",
            if rc.is_empty() {
                "CBR".into()
            } else {
                rc.clone()
            },
            "CBR".into(),
            format!("{out} › Rate Control"),
            rc.is_empty() || rc == "CBR",
        );
        let k = o.keyint_sec.unwrap_or(2);
        row(
            "Keyframe interval",
            if k == 0 {
                "Auto".into()
            } else {
                format!("{k} s")
            },
            "2 s".into(),
            format!("{out} › Keyframe Interval"),
            k == 2 || k == 1,
        );
        let skipped = c
            .test
            .map(|t| t.output_total > 0 && t.output_skipped as f32 / t.output_total as f32 > 0.005)
            .unwrap_or(false);
        let headroom = c
            .test
            .map(|t| t.gpu_avg.map(|g| g < 85.0).unwrap_or(false) && !skipped)
            .unwrap_or(false);
        let (sug, okp) = if o.encoder_kind.contains("NVENC") {
            (
                if headroom {
                    nvenc_preset_words(None, false)
                } else {
                    let g = c
                        .test
                        .and_then(|t| t.game_running.clone().or_else(|| t.game_guess.clone()));
                    nvenc_preset_words(g.as_deref(), g.is_none())
                },
                preset_rank(&o.preset)
                    .map(|r| (if headroom { r >= 6 } else { r >= 4 }) && !(r >= 6 && skipped))
                    .unwrap_or(true),
            )
        } else if o.encoder_kind.contains("x264") {
            (
                "veryfast".to_string(),
                preset_rank(&o.preset)
                    .map(|r| (3..=5).contains(&r))
                    .unwrap_or(true),
            )
        } else {
            (
                "Quality".to_string(),
                preset_rank(&o.preset).map(|r| r >= 4).unwrap_or(true),
            )
        };
        row(
            "Preset",
            preset_name(&o.preset),
            sug,
            format!("{out} › Preset"),
            okp,
        );
    }
    row(
        "Output (resolution · FPS)",
        a.current_output
            .split(" · ")
            .next()
            .unwrap_or("")
            .to_string(),
        a.recommended_output
            .split(" · ")
            .next()
            .unwrap_or("")
            .to_string(),
        "OBS › Settings › Video".into(),
        a.recommended_output == a.current_output || a.recommended_output == "—",
    );
    if let (Some(b), Some(ot)) = (o.base_res, o.output_res)
        && b != ot
    {
        let f = o.scale_type.to_lowercase();
        row(
            "Downscale filter",
            super_cap(&f),
            "Lanczos (sharpest)".into(),
            "OBS › Settings › Video › Downscale Filter".into(),
            f != "bilinear",
        );
    }
    if let Some(k) = o.audio_kbps {
        let sr = o.sample_rate.unwrap_or(48000);
        let twitch_high = c.st.platform == crate::config::Platform::Twitch && k > 160;
        row(
            "Audio",
            format!("{k} kbps · {:.0} kHz", sr as f32 / 1000.0),
            "160 kbps · 48 kHz".into(),
            "OBS › Settings › Output › Audio  ·  Settings › Audio".into(),
            k >= 128 && !twitch_high && sr == 48000,
        );
    }
    // Recording
    let separate = !o.rec_encoder_raw.is_empty();
    let chips = crate::hardware::nvenc_chips(
        &c.hw
            .and_then(|h| h.main_gpu())
            .map(|g| g.name.clone())
            .unwrap_or_default(),
    );
    let multi = chips.map(|n| n >= 2).unwrap_or(false);
    let rec_sug = if multi {
        "Separate is fine on your card"
    } else {
        "Same as stream, or separate at P4 or faster"
    };
    let rec_yours = if separate {
        format!(
            "Separate: {} {}",
            o.rec_encoder_kind,
            preset_name(&o.rec_preset)
        )
    } else {
        "Same as stream".into()
    };
    let rec_ok = !separate
        || (!o.rec_encoder_kind.contains("CPU")
            && preset_rank(&o.rec_preset)
                .map(|r| r <= 5 || chips.map(|n| n >= 2).unwrap_or(false))
                .unwrap_or(true));
    row(
        "Recording encoder",
        rec_yours,
        rec_sug.into(),
        if advanced {
            format!("{out} › Recording › Video Encoder")
        } else {
            format!("{out} › Recording Quality")
        },
        rec_ok,
    );
    let f = o.rec_format.to_lowercase();
    if !f.is_empty() {
        row(
            "Recording format",
            format_label(&f),
            "Hybrid MP4, Hybrid MOV or MKV".into(),
            format!("{out} › Recording Format"),
            f != "mp4" && f != "mov",
        );
    }
    if let Some(a) = o.auto_record {
        row(
            "Auto-record when streaming",
            if a { "On".into() } else { "Off".into() },
            "Your choice".into(),
            "OBS › Settings › Advanced › Recording".into(),
            true,
        );
    }
    if o.multitrack == Some(true) {
        let busy = c
            .test
            .and_then(|t| t.enc_avg)
            .map(|e| e >= 85.0)
            .unwrap_or(false);
        row(
            "Enhanced Broadcasting",
            "On".into(),
            if busy {
                "Off (your encoder is busy)".into()
            } else {
                "Your choice".into()
            },
            "OBS › Settings › Stream".into(),
            !busy,
        );
    }
    v
}

fn format_label(f: &str) -> String {
    match f {
        "hybrid_mp4" => "Hybrid MP4".into(),
        "hybrid_mov" => "Hybrid MOV".into(),
        "fragmented_mp4" => "Fragmented MP4".into(),
        "fragmented_mov" => "Fragmented MOV".into(),
        other => other.to_uppercase(),
    }
}

fn super_cap(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| format!("{}{}", f.to_uppercase(), c.as_str()))
        .unwrap_or_default()
}

fn network(c: &Ctx, a: &mut Analysis, issues: &mut Vec<Issue>) {
    let limits = crate::network::limits(c.st.platform);
    let cur_bitrate = c.obs.and_then(|o| o.bitrate_kbps);
    let out_res = c.obs.and_then(|o| o.output_res);
    let fps = c.obs.and_then(|o| o.fps).unwrap_or(60.0);
    if c.obs.is_none() {
        a.current_output = "Connect OBS to see your stream settings".into();
        a.recommended_output = "—".into();
    }
    a.current_output = if c.obs.is_none() {
        a.current_output.clone()
    } else {
        format!(
            "{}{} · {}",
            res_label(out_res),
            fps.round() as u32,
            cur_bitrate
                .map(|b| format!("{b} kbps"))
                .unwrap_or_else(|| "bitrate unknown".into())
        )
    };
    if c.obs.is_some() {
        a.recommended_output = a.current_output.clone();
    }

    let Some(n) = c.net.filter(|n| n.ok) else {
        a.checks.push(Check {
            name: "Network".into(),
            status: Status::Unknown,
            label: "Not tested".into(),
            metric: "—".into(),
            note: c
                .net
                .and_then(|n| n.error.clone())
                .unwrap_or_else(|| "Run a quick scan to test your upload speed.".into()),
            measured: false,
        });
        return;
    };
    let safe = n.safe_video_kbps();
    let cap = safe.min(limits.max_kbps_1080p60);
    let unsteady = n.unsteady_for(cur_bitrate.unwrap_or(limits.max_kbps_1080p60));
    let mut status = if unsteady { Status::Warn } else { Status::Ok };
    let mut label = if unsteady {
        "Unsteady".to_string()
    } else {
        "Good".to_string()
    };
    let note;
    if let Some(b) = cur_bitrate {
        if b > safe {
            status = Status::Bad;
            label = "Too tight".into();
            note = format!(
                "Upload dips to {:.1} Mbps. That's too tight for {b} kbps.",
                n.low_mbps
            );
            let mut opts = Vec::new();
            let at_1080 = out_res.map(|(_, h)| h >= 1080).unwrap_or(true);
            let keep_res_ok = cap >= 6000 || !at_1080;
            opts.push(opt(
                "keep_res",
                format!("{} kbps at {}", cap, res_label(out_res)),
                "OBS › Settings › Output › Bitrate",
                if keep_res_ok {
                    "Keeps your resolution; no dropped frames"
                } else {
                    "Keeps your resolution"
                },
                if keep_res_ok {
                    "Slightly less detail in fast motion"
                } else {
                    "Blockier image in fast motion"
                },
            ));
            if at_1080 && !c.st.never.lower_below_1080p {
                opts.push(opt(
                    "936",
                    format!("{} kbps at 936p", cap.min(6000)),
                    "OBS › Settings › Output › Bitrate  ·  Video › Output (Scaled) Resolution › 1664x936",
                    "Sharp image at a lower bitrate",
                    "Slightly lower resolution",
                ));
                if cap < 4500 {
                    opts.push(opt(
                        "720",
                        format!("{} kbps at 720p", cap.min(4500)),
                        "OBS › Settings › Output › Bitrate  ·  Video › Output (Scaled) Resolution › 1280x720",
                        "Safest on a slow or unsteady connection",
                        "Noticeably softer image",
                    ));
                }
            }
            let pick = if cap < 4500 && opts.iter().any(|o| o.id == "720") {
                "720"
            } else if c.st.smooth_vs_stream >= 60 || keep_res_ok || c.st.never.lower_below_1080p {
                "keep_res"
            } else {
                "936"
            };
            suggest(&mut opts, pick);
            if let Some(s) = opts.iter().find(|o| o.suggested) {
                a.recommended_output = format!(
                    "{}{} · {} kbps",
                    if s.id == "keep_res" {
                        res_label(out_res)
                    } else {
                        format!("{}p", s.id)
                    },
                    fps.round() as u32,
                    if s.id == "720" {
                        cap.min(4500)
                    } else if s.id == "936" {
                        cap.min(6000)
                    } else {
                        cap
                    }
                );
            }
            issues.push(Issue {
                id: "bitrate_network".into(),
                priority: if b as f32 > safe as f32 * 1.3 { Priority::High } else { Priority::Medium },
                area: "OBS".into(),
                title: format!("Your upload dips to {:.1} Mbps, too low for {b} kbps.", n.low_mbps),
                why: format!(
                    "It averaged {:.1} Mbps, but at its slowest moments the stream wouldn't fit, so OBS would drop frames and viewers would buffer.",
                    n.avg_mbps
                ),
                who: "Everyone on Wi-Fi or shared home internet. A wired connection may let you keep a higher bitrate.".into(),
                learn: "Bitrate is how much video data you send each second. If your connection briefly can't keep up, frames are dropped. A good rule is to stream at no more than about 75% of your lowest measured upload.".into(),
                measured: true,
                options: opts,
            });
        } else if b > limits.max_kbps_1080p60 {
            status = Status::Warn;
            label = "Above limit".into();
            note = format!(
                "{} kbps is above {}'s usual limit.",
                b,
                c.st.platform.name()
            );
            let mut opts = vec![opt(
                "platform_max",
                format!("Lower to {} kbps", limits.max_kbps_1080p60),
                "OBS › Settings › Output › Bitrate",
                "Stream is accepted reliably",
                "Nothing, if the platform would cap it anyway",
            )];
            suggest(&mut opts, "platform_max");
            issues.push(Issue {
                id: "bitrate_platform".into(),
                priority: Priority::Medium,
                area: "OBS".into(),
                title: format!("Your bitrate is {} kbps, above {}'s usual limit of {} kbps.", b, c.st.platform.name(), limits.max_kbps_1080p60),
                why: limits.note.into(),
                who: format!("Anyone streaming to {}.", c.st.platform.name()),
                learn: "Platforms set a maximum bitrate. Going over can cause buffering for viewers or a rejected stream.".into(),
                measured: false,
                options: opts,
            });
        } else {
            let need = (b as f32 + 160.0) / 1000.0;
            note = format!(
                "Your upload is {:.0} Mbps, dipping to {:.0} at worst. That's about {:.0} times what your {} kbps stream needs.",
                n.avg_mbps,
                n.low_mbps,
                n.low_mbps / need,
                b
            );
        }
    } else {
        note = format!(
            "Upload {:.1} Mbps (low {:.1}). Safe bitrate: about {} kbps.",
            n.avg_mbps, n.low_mbps, cap
        );
    }
    let note = if n.stopped_early {
        format!("{note} Fast connection: the test stopped early to save data.")
    } else {
        note
    };
    if unsteady && status != Status::Bad {
        issues.push(Issue {
            id: "net_unsteady".into(),
            priority: Priority::Low,
            area: "Network".into(),
            title: format!("Your upload speed swings a lot, dropping to {:.0}% of its average.", n.steadiness * 100.0),
            why: {
                let need = (cur_bitrate.unwrap_or(limits.max_kbps_1080p60) as f32 + 160.0) / 1000.0;
                format!("At its slowest it's {:.1} Mbps, only about {:.1} times what your stream needs, so busy moments could drop frames.", n.low_mbps, n.low_mbps / need)
            },
            who: "Most common on Wi-Fi.".into(),
            learn: "Unsteady upload is usually Wi-Fi interference, other devices or programs using the internet, or a busy network in the evening.".into(),
            measured: true,
            options: {
                let mut o = vec![
                    opt("wired", "Use a wired (Ethernet) connection", "Plug your PC into the router", "Much steadier upload", "Needs a cable"),
                    opt("pause", "Pause downloads and cloud sync while live", "Steam, Epic, OneDrive, Dropbox and similar", "Frees upload bandwidth", "Updates wait until later"),
                ];
                suggest(&mut o, "wired");
                o
            },
        });
    }
    // A saved result from an earlier scan: say how old it is.
    let age = crate::util::unix_now() - n.tested_at;
    let note = if n.tested_at > 0 && age >= 15 * 60 {
        let h = age / 3600;
        let when = if h >= 1 {
            format!("{h} hour{} ago", if h == 1 { "" } else { "s" })
        } else {
            format!("{} minutes ago", age / 60)
        };
        format!("{note} (Tested {when}; tests again after 6 hours.)")
    } else {
        note
    };
    a.checks.push(Check {
        name: "Network".into(),
        status,
        label,
        metric: format!("{:.1} Mbps up", n.avg_mbps),
        note,
        measured: true,
    });
}

fn game_load(c: &Ctx, a: &mut Analysis, issues: &mut Vec<Issue>) {
    let target = target_game(c, true);
    let unknown_game =
        target.is_none() && c.test.map(|t| t.game_running.is_none()).unwrap_or(false);

    let gpu_avg = c.test.and_then(|t| t.gpu_avg);
    let render_drop = c
        .test
        .map(|t| t.render_total > 0 && t.render_skipped as f32 / t.render_total as f32 > 0.01)
        .unwrap_or(false);
    // How long the GPU was maxed out matters more than the average: a game
    // can sit at 98% for the busy half of the test and still average 84%.
    let maxed_frac = c
        .test
        .map(|t| {
            let v: Vec<f32> = t.samples.iter().filter_map(|s| s.gpu).collect();
            if v.is_empty() {
                0.0
            } else {
                v.iter().filter(|g| **g >= 95.0).count() as f32 / v.len() as f32
            }
        })
        .unwrap_or(0.0);
    let maxed_secs = c.test.map(|t| t.seconds * maxed_frac).unwrap_or(0.0);
    let gpu_maxed =
        gpu_avg.map(|g| g >= 92.0).unwrap_or(false) || maxed_frac >= 0.25 || render_drop;
    let maxed_text = if maxed_frac >= 0.25 && gpu_avg.map(|g| g < 92.0).unwrap_or(true) {
        format!(
            "Your GPU was maxed out (95%+) for {:.0}% of the test, about {:.0} seconds.",
            maxed_frac * 100.0,
            maxed_secs
        )
    } else {
        format!(
            "Your GPU was maxed out, at {:.0}% on average.",
            gpu_avg.unwrap_or(99.0)
        )
    };
    let skipped_frac = c
        .test
        .map(|t| {
            if t.output_total > 0 {
                t.output_skipped as f32 / t.output_total as f32
            } else {
                0.0
            }
        })
        .unwrap_or(0.0);
    let render_frac = c
        .test
        .map(|t| {
            if t.render_total > 0 {
                t.render_skipped as f32 / t.render_total as f32
            } else {
                0.0
            }
        })
        .unwrap_or(0.0);
    // OBS kept up in the test even though the GPU was busy.
    let obs_kept_up = skipped_frac < 0.001 && render_frac < 0.001;
    // On very high refresh monitors, capping at the refresh rate rarely lowers
    // the load, so suggest half of it.
    let hz = c.st.monitor_hz.max(30);
    let stream_fps = c.obs.and_then(|o| o.fps);
    let fps_game = target
        .map(|g| g.name.clone())
        .or_else(|| c.test.and_then(|t| t.game_guess.clone()))
        .unwrap_or_default();
    let game_fps = c.game_fps(&fps_game);
    let cap_to = cap_target(hz, stream_fps, c.st.style, game_fps);
    let guess = c.test.and_then(|t| t.game_guess.clone());

    let game_label = guess
        .as_deref()
        .map(|g| format!("The game ({g})"))
        .unwrap_or_else(|| "Your game".into());
    let (status, label, metric, note) = match (target, gpu_avg) {
        (None, Some(_)) if unknown_game && guess.is_none() => (
            Status::Unknown,
            "No game running".into(),
            "—".into(),
            "No game was running during the test, so the game's load wasn't measured. Start your game, then test again.".into(),
        ),
        (None, Some(ga)) if unknown_game => {
            if gpu_maxed {
                (if obs_kept_up { Status::Warn } else { Status::Bad }, "GPU maxed".into(), maxed_text.clone(), format!("{game_label} wasn't one Preflight knows, so the advice is general."))
            } else {
                (Status::Ok, "Good".into(), format!("Your GPU averaged {ga:.0}%."), "There's room left for the stream encoder.".into())
            }
        }
        (None, _) => (Status::Unknown, "No game found".to_string(), "—".to_string(), "None of the supported games were found on this PC.".to_string()),
        (Some(g), Some(_)) if c.gpu_changed => (Status::Ok, "Changed".into(), "You changed the heavy settings.".into(), format!("Run another test in {} to measure the difference.", g.name)),
        (Some(g), Some(_)) if gpu_maxed => (if obs_kept_up { Status::Warn } else { Status::Bad }, "Very busy".into(), maxed_text.clone(), if obs_kept_up { format!("OBS kept up this time, but {} leaves little room in busy scenes.", g.name) } else { format!("{} leaves no room for the stream encoder.", g.name) }),
        (Some(g), Some(ga)) => (Status::Ok, "Good".into(), format!("Your GPU averaged {ga:.0}%."), format!("{} leaves room for the stream encoder.", g.name)),
        (Some(g), None) => {
            let heavy = g.findings.iter().filter(|f| f.effect != "info").count();
            if heavy > 0 {
                (Status::Warn, "Could be heavy".into(), format!("{heavy} heavy setting{}", if heavy == 1 { "" } else { "s" }), format!("{}: some settings are known to cost a lot. Run a full test to measure.", g.name))
            } else if g.files.iter().any(|f| f.read_ok) {
                (Status::Ok, "Looks fine".into(), g.name.clone(), "No heavy settings found. Run a full test to measure.".into())
            } else {
                (Status::Unknown, "Not read".into(), g.name.clone(), "Game found, but its settings file wasn't read. Run a full test to measure.".into())
            }
        }
    };
    // Say plainly when a setting Preflight expects couldn't be read.
    let note = match target.filter(|g| !g.unreadable.is_empty()) {
        Some(g) => format!(
            "{note} Couldn't read: {}. A game update may have renamed {}.",
            g.unreadable.join(", "),
            if g.unreadable.len() == 1 {
                "it"
            } else {
                "them"
            }
        ),
        None => note,
    };
    let status =
        if status == Status::Ok && target.map(|g| !g.unreadable.is_empty()).unwrap_or(false) {
            Status::Warn
        } else {
            status
        };
    a.checks.push(Check {
        name: "Game settings".into(),
        status,
        label,
        metric,
        note,
        measured: gpu_avg.is_some(),
    });

    let st = c.st;
    if target.is_none() && unknown_game && gpu_maxed {
        let menu = "Your game › Settings › Video (Frame rate limit / Max FPS), if it has one";
        let cap_cost = if cap_to < hz {
            format!("Fewer frames than your {hz} Hz monitor can show")
        } else {
            "Nothing visible at your refresh rate".to_string()
        };
        let vendor = c.hw.and_then(|h| h.gpus.first()).map(|g| g.vendor);
        let stand_in = GameResult {
            name: guess.clone().unwrap_or_else(|| "your game".into()),
            auto: Some("unknown".into()),
            proton: cfg!(not(windows)),
            ..Default::default()
        };
        let mut out = outside_cap(&stand_in, cap_to, vendor, cap_cost.clone());
        out.id = "cap_out".into();
        let mut opts = vec![
            opt(
                "cap",
                format!("Cap your FPS at {cap_to}"),
                menu,
                "Big drop in GPU load",
                cap_cost.clone(),
            )
            .with(change("FPS limit", menu, None, cap_to.to_string())),
            out,
            opt(
                "upscale",
                "Turn on DLSS or FSR (Quality)",
                "Your game › Settings › Graphics",
                "Big FPS gain at almost the same sharpness",
                "Slightly softer image",
            ),
            opt(
                "preset",
                "Lower your graphics preset",
                "Your game › Settings › Graphics",
                "Reliable GPU headroom",
                "A small drop in visual quality",
            ),
        ];
        let pick = style_pick(st, &opts);
        suggest(&mut opts, &pick);
        issues.push(Issue {
            id: "gpu_maxed".into(),
            priority: if obs_kept_up { Priority::Medium } else { Priority::High },
            area: game_label.clone(),
            title: maxed_text.clone(),
            why: if obs_kept_up {
                "OBS kept up during the test, so this isn't hurting your stream yet. In busier scenes the encoder can fall behind; leaving 10-20% headroom avoids that.".into()
            } else {
                format!("That left no room for OBS, so it skipped {:.1}% of frames. Viewers would see stutters.", skipped_frac.max(render_frac) * 100.0)
            },
            who: "Anyone encoding with the GPU.".into(),
            learn: "Graphics cards have a separate chip for video encoding, but it still shares the GPU with the game. If the game keeps the GPU at 100%, OBS's frames can arrive late. Preflight didn't recognise the game, so it can't point to specific settings.".into(),
            measured: true,
            options: opts,
        });
    }
    let Some(g) = target else { return };
    let mut opts: Vec<Opt> = Vec::new();
    let fps_menu = g
        .findings
        .iter()
        .find(|f| f.effect == "uncapped_fps")
        .map(|f| f.menu.clone())
        .or_else(|| {
            g.current
                .iter()
                .find(|c| c.effect == "uncapped_fps")
                .map(|c| c.menu.clone())
        })
        .unwrap_or_else(|| format!("{} › Settings › Video (Max FPS / Frame rate limit)", g.name));
    let cap_cost = if cap_to < hz {
        format!("Fewer frames than your {hz} Hz monitor can show, but steadier")
    } else {
        format!("Nothing visible on a {hz} Hz monitor")
    };
    let uncapped = g.findings.iter().any(|f| f.effect == "uncapped_fps");
    if !g.has_fps_limit {
        let vendor = c.hw.and_then(|h| h.gpus.first()).map(|g| g.vendor);
        if g.auto.is_some() {
            // Read automatically and no FPS setting found: the game may still
            // have one. Suggest it first, with a limiter outside the game as
            // the fallback.
            let menu = format!(
                "{} › Settings › Video or Display (Frame rate limit / Max FPS), if it has one",
                g.name
            );
            opts.push(
                opt(
                    "cap",
                    format!("Cap your FPS at {cap_to}"),
                    menu.clone(),
                    "Big drop in GPU load, smoother frame pacing",
                    cap_cost.clone(),
                )
                .with(change("FPS limit", menu, None, cap_to.to_string())),
            );
            let mut out = outside_cap(g, cap_to, vendor, cap_cost.clone());
            out.id = "cap_out".into();
            opts.push(out);
        } else {
            // The game has no FPS limit of its own: point to one outside it.
            opts.push(outside_cap(g, cap_to, vendor, cap_cost.clone()));
        }
    } else if uncapped {
        opts.push(
            opt(
                "cap",
                format!("Cap your FPS at {cap_to}"),
                fps_menu.clone(),
                "Big drop in GPU load, smoother frame pacing",
                cap_cost.clone(),
            )
            .with(change(
                "FPS limit",
                fps_menu.clone(),
                Some("Off".into()),
                cap_to.to_string(),
            )),
        );
    } else if let Some(cur) = g.fps_cap {
        // A cap the game never reaches does nothing: the GPU still runs flat out.
        if gpu_maxed && cur > cap_to {
            opts.push(opt("cap", format!("Lower your FPS limit to {cap_to}"), fps_menu.clone(), format!("At {cur} the limit never kicks in. At {cap_to} it holds and frees GPU for the stream"), cap_cost.clone())
                .with(change("FPS limit", fps_menu.clone(), Some(cur.to_string()), cap_to.to_string())));
        }
    } else if g.unreadable.iter().any(|u| u.contains("FPS"))
        || g.current.iter().all(|c| c.effect != "uncapped_fps")
    {
        opts.push(
            opt(
                "cap",
                format!("Cap your FPS at {cap_to}"),
                fps_menu.clone(),
                "Big drop in GPU load, smoother frame pacing",
                cap_cost.clone(),
            )
            .with(change(
                "FPS limit",
                fps_menu.clone(),
                None,
                cap_to.to_string(),
            )),
        );
    }
    for f in &g.findings {
        match f.effect.as_str() {
            "rt_on" if !st.never.disable_ray_tracing => {
                let name = f.label.replace(" is on", "");
                let to = f.change_to.clone().unwrap_or_else(|| "Off".into());
                opts.push(
                    opt(
                        &format!("rt_{}", f.rule_id),
                        format!(
                            "Turn {} {}",
                            name.to_lowercase(),
                            if to.starts_with("Off") {
                                "off".to_string()
                            } else {
                                format!("to {to}")
                            }
                        ),
                        f.menu.clone(),
                        f.gain
                            .clone()
                            .unwrap_or_else(|| "Large GPU headroom".into()),
                        f.cost
                            .clone()
                            .unwrap_or_else(|| "Less realistic lighting".into()),
                    )
                    .with(change(name, f.menu.clone(), Some("On".into()), to)),
                )
            }
            "upscaler_off" if !st.never.upscaling => opts.push(
                opt(
                    "upscale",
                    "Turn on DLSS or FSR (Quality)",
                    f.menu.clone(),
                    "Big FPS gain at almost the same sharpness",
                    "Slightly softer image",
                )
                .with(change(
                    "Upscaler",
                    f.menu.clone(),
                    Some("Off".into()),
                    "DLSS or FSR: Quality",
                )),
            ),
            "heavy" => opts.push(
                opt(
                    &format!("h_{}", f.rule_id),
                    format!(
                        "{} to {}",
                        f.label,
                        f.change_to.clone().unwrap_or_else(|| "lower".into())
                    ),
                    f.menu.clone(),
                    f.gain.clone().unwrap_or_else(|| "More GPU headroom".into()),
                    f.cost
                        .clone()
                        .unwrap_or_else(|| "Slightly lower visual quality".into()),
                )
                .with(change(
                    f.label.clone(),
                    f.menu.clone(),
                    Some(crate::games::pretty_value(&f.value)),
                    f.change_to.clone().unwrap_or_else(|| "Lower".into()),
                )),
            ),
            _ => {}
        }
    }
    // The preset, or (when it's Custom) the heaviest individual settings.
    let heavy: Vec<&crate::games::TunableState> = g
        .tunables
        .iter()
        .filter(|t| t.change_to.is_some())
        .collect();
    match &g.preset {
        Some(p) if !p.custom && p.lower.is_some() => {
            let lower = p.lower.clone().unwrap_or_default();
            opts.push(
                opt(
                    "preset",
                    format!("Set {} to {lower}", p.setting),
                    p.menu.clone(),
                    "Reliable GPU headroom",
                    "A small drop in detail",
                )
                .with(change(
                    p.setting.clone(),
                    p.menu.clone(),
                    Some(p.value.clone()),
                    lower,
                )),
            );
        }
        _ if !heavy.is_empty() => {
            let top: Vec<&&crate::games::TunableState> = heavy.iter().take(3).collect();
            let name = if top.len() == 1 {
                format!(
                    "Set {} to {}",
                    top[0].setting,
                    top[0].change_to.clone().unwrap_or_default()
                )
            } else {
                format!("Lower {} heavy graphics settings", top.len())
            };
            let mut o = opt(
                "tune",
                name,
                top[0].menu.clone(),
                top[0].gain.clone(),
                top.iter()
                    .map(|t| t.cost.to_lowercase())
                    .collect::<Vec<_>>()
                    .join("; "),
            );
            for t in &top {
                o = o.with(change(
                    t.setting.clone(),
                    t.menu.clone(),
                    Some(t.now.clone()),
                    t.change_to.clone().unwrap_or_default(),
                ));
            }
            opts.push(o);
        }
        _ => {}
    }
    let cpu_game = g.load.starts_with("cpu");
    if opts.is_empty() {
        // Preflight couldn't read anything specific: say so instead of guessing.
        let menu = g
            .preset
            .as_ref()
            .map(|p| p.menu.clone())
            .unwrap_or_else(|| format!("{} › Settings › Graphics", g.name));
        opts.push(
            opt(
                "preset",
                "Lower your graphics preset",
                menu.clone(),
                "Reliable GPU headroom",
                "A small drop in detail",
            )
            .with(change(
                "Graphics preset",
                menu,
                g.preset.as_ref().map(|p| p.value.clone()),
                "One level lower",
            )),
        );
    }
    if gpu_maxed {
        let pick = style_pick(st, &opts);
        suggest(&mut opts, &pick);
        issues.push(Issue {
            id: "gpu_maxed".into(),
            priority: if obs_kept_up { Priority::Medium } else { Priority::High },
            area: g.name.clone(),
            title: maxed_text.clone(),
            why: {
                let changed = if c.gpu_changed { " You've changed settings since the test, so run another test to see the difference." } else { "" };
                if skipped_frac > 0.001 {
                    format!("That leaves no room for the stream encoder, so OBS skipped {:.1}% of frames, {}. Viewers would see stutters in busy scenes.{changed}", skipped_frac * 100.0, one_in(skipped_frac))
                } else {
                    format!("OBS kept up during the test, so this isn't hurting your stream yet. In busier scenes the encoder can fall behind; leaving 10-20% headroom avoids that.{changed}")
                }
            },
            who: "Anyone encoding with the GPU (NVENC, AMF, Quick Sync, VA-API). Matters most in busy scenes.".into(),
            learn: "Graphics cards have a separate chip for video encoding, but it still shares memory and scheduling with the game. If the game keeps the GPU at 100%, encoded frames arrive late and OBS skips them. Leaving 10–20% headroom usually fixes this.".into(),
            measured: true,
            options: opts,
        });
    } else if c.test.is_some() && maxed_frac >= 0.10 && !cpu_game {
        // Close to the limit: worth knowing, not scored.
        let pick = style_pick(st, &opts);
        suggest(&mut opts, &pick);
        issues.push(Issue {
            id: "gpu_near".into(),
            priority: Priority::Low,
            area: g.name.clone(),
            title: format!("Your GPU hit 95%+ for {:.0}% of the test, about {:.0} seconds.", maxed_frac * 100.0, maxed_secs),
            why: "Not enough to hurt the stream in this test, but busier scenes could push it over. This heads-up doesn't count against your scores.".into(),
            who: "Anyone encoding with the GPU.".into(),
            learn: "Short spikes to 100% are normal. If they get longer, OBS's encoder can fall behind and skip frames. One of these changes keeps some headroom.".into(),
            measured: true,
            options: opts.clone(),
        });
    } else if c.test.is_none()
        && g.findings
            .iter()
            .any(|f| f.effect == "uncapped_fps" || f.effect == "heavy" || f.effect == "rt_on")
        && !cpu_game
    {
        let pick = style_pick(st, &opts);
        suggest(&mut opts, &pick);
        issues.push(Issue {
            id: "gpu_maybe".into(),
            priority: Priority::Low,
            area: g.name.clone(),
            title: format!("{} has settings that often max out the GPU.", g.name),
            why: format!(
                "Found in your settings file: {}. This is an estimate, so run a test while playing to measure it.",
                g.findings.iter().filter(|f| f.effect != "info").map(|f| f.label.to_lowercase()).collect::<Vec<_>>().join(", ")
            ),
            who: "Anyone encoding with the GPU.".into(),
            learn: "An uncapped frame rate or very heavy effects keep the GPU at 100%, which can delay the stream encoder.".into(),
            measured: false,
            options: opts,
        });
    }

    // CPU-heavy games (city builders) and CPU pressure.
    if let Some(t) = c.test
        && t.cpu_avg >= 85.0
    {
        let x264 = c
            .obs
            .map(|o| o.encoder_kind.contains("CPU"))
            .unwrap_or(false);
        let mut o = Vec::new();
        if x264 {
            o.push(opt(
                "hw",
                "Switch OBS to your GPU encoder",
                "OBS › Settings › Output › Video Encoder",
                "Biggest CPU saving",
                "Very little",
            ));
        }
        o.push(opt(
            "bg",
            "Close background programs",
            "Browser tabs, launchers, cloud sync",
            "Frees CPU time",
            "Those programs stop until you reopen them",
        ));
        if cpu_game {
            o.push(opt(
                "sim",
                "Lower simulation-heavy settings or speed",
                format!("{} › Options", g.name),
                "Less CPU work per frame",
                "Slower simulation or less detail",
            ));
        } else {
            o.push(opt(
                "cap",
                format!("Cap FPS at {}", st.monitor_hz),
                format!("{} › Settings › Video", g.name),
                "Fewer frames to prepare means less CPU work",
                "Nothing visible at your refresh rate",
            ));
        }
        suggest(&mut o, if x264 { "hw" } else { "bg" });
        issues.push(Issue {
                id: "cpu_high".into(),
                priority: Priority::High,
                area: "System".into(),
                title: format!("Your CPU was nearly maxed out, at {:.0}% on average.", t.cpu_avg),
                why: format!("It peaked at {:.0}%. OBS needs some CPU too, so when it's full the stream and the game can both stutter.", t.cpu_max),
                who: if cpu_game { format!("{} leans heavily on the CPU, especially in big cities.", g.name) } else { "Anyone running heavy games with other programs open.".into() },
                learn: "When the CPU is full, OBS can't prepare frames on time and the game may stutter too. GPU encoding removes most of OBS's CPU work.".into(),
                measured: true,
                options: o,
            });
    }
}

fn audio(c: &Ctx, a: &mut Analysis, issues: &mut Vec<Issue>) {
    let Some(o) = c.obs.filter(|o| o.connected || !o.audio.is_empty()) else {
        a.checks.push(Check {
            name: "Audio".into(),
            status: Status::Unknown,
            label: "Not connected".into(),
            metric: "—".into(),
            note: "Connect OBS to check your mic and game audio.".into(),
            measured: false,
        });
        return;
    };
    let mics = o.mics();
    if mics.is_empty() && o.connected {
        a.checks.push(Check {
            name: "Audio".into(),
            status: Status::Warn,
            label: "No mic".into(),
            metric: "—".into(),
            note: "OBS has no microphone input.".into(),
            measured: true,
        });
        return;
    }
    let (mic, desk) = (
        c.test.and_then(|t| t.mic_db),
        c.test.and_then(|t| t.desktop_db),
    );
    if o.connected && o.desktop_audio().is_empty() {
        let linux = !cfg!(windows);
        let mut opts = vec![opt(
            "desktop",
            "Turn on Desktop Audio",
            "OBS › Settings › Audio › Global Audio Devices › Desktop Audio › Default",
            "Viewers hear the game (and everything else your PC plays)",
            "Discord and music are included too",
        )];
        if linux {
            opts.push(opt(
                "app",
                "Capture just the game (Application Audio Capture)",
                "Install the obs-pipewire-audio-capture plugin, then Sources › + › Application Audio Capture (PipeWire) › pick the game",
                "Only the game's sound, no Discord or music",
                "A plugin to install",
            ));
        } else {
            opts.push(opt(
                "app",
                "Capture just the game (Application Audio Capture)",
                "OBS › Sources › + › Application Audio Capture › pick the game",
                "Only the game's sound, no Discord or music",
                "Pick the game again if its window changes",
            ));
        }
        suggest(&mut opts, "desktop");
        issues.push(Issue {
            id: "no_game_audio".into(),
            priority: Priority::Medium,
            area: "Audio".into(),
            title: "OBS has no game audio source, so viewers won't hear your game.".into(),
            why: if linux {
                "Screen Capture (PipeWire) only captures the picture. Game sound needs its own audio source in OBS.".into()
            } else {
                "Your scene has no desktop or game audio source. Game sound needs its own audio source in OBS.".into()
            },
            who: "Anyone who wants viewers to hear the game.".into(),
            learn: "OBS captures picture and sound separately. Desktop Audio sends everything your PC plays; an application capture sends one program.".into(),
            measured: false,
            options: opts,
        });
    }
    let doubled = o.doubled_desktop_audio();
    if o.connected && doubled >= 2 {
        let mut opts = vec![
            opt(
                "mute",
                "Mute or remove one of them",
                "OBS › Audio Mixer › the extra Desktop Audio › mute (or Settings › Audio › Global Audio Devices › Desktop Audio 2 › Disabled)",
                "The game is heard once, at the level you set",
                "None",
            ),
            opt(
                "keep",
                "Keep both",
                "Nothing to change",
                "Fine if one is only in a scene you don't use while gaming",
                "If both are live, game sound is louder and can echo",
            ),
        ];
        suggest(&mut opts, "mute");
        issues.push(Issue {
            id: "double_desktop_audio".into(),
            priority: Priority::Low,
            area: "Audio".into(),
            title: format!("{doubled} Desktop Audio sources capture the same sound, so viewers may hear the game twice."),
            why: "Both are unmuted and set to the same output device. Game sound gets louder and can sound doubled or echoey.".into(),
            who: "Anyone with more than one Desktop Audio source.".into(),
            learn: "OBS mixes every unmuted source. Two captures of the same speakers add the same sound together.".into(),
            measured: false,
            options: opts,
        });
    }
    if let (Some(m), None) = (mic, desk)
        && m < -40.0
    {
        let mut opts = vec![
            opt(
                "gain",
                "Raise the mic's level",
                "OBS › Audio Mixer › your mic (or your system's input volume)",
                "Your voice comes through clearly",
                "Background noise gets louder too",
            ),
            opt(
                "filter",
                "Add a Gain filter",
                "OBS › Audio Mixer › your mic › ⋮ › Filters › + › Gain",
                "Boosts only the stream, not other apps",
                "One more setting to manage",
            ),
        ];
        suggest(&mut opts, "gain");
        issues.push(Issue {
                id: "mic_quiet".into(),
                priority: Priority::Low,
                area: "Audio".into(),
                title: format!("Your mic peaked at only about {m:.0} dB during the test."),
                why: "Speech usually peaks around -20 to -10 dB. If you were talking, viewers will struggle to hear you. If you were quiet during the test, you can ignore this.".into(),
                who: "Anyone who talks on stream.".into(),
                learn: "OBS meters show loudness in dB below the maximum (0 dB). A voice around -20 to -10 dB is easy to hear without distorting.".into(),
                measured: true,
                options: opts,
            });
    }
    match (mic, desk) {
        (Some(m), Some(d)) if m < d - 3.0 => {
            let diff = d - m;
            a.checks.push(Check {
                name: "Audio".into(),
                status: Status::Warn,
                label: "Mic quiet".into(),
                metric: format!(
                    "Your mic is quieter than the game, by about {:.0} dB.",
                    diff
                ),
                note: mic_gap_meaning(diff).into(),
                measured: true,
            });
            let mut opts = vec![
                opt(
                    "mic_up",
                    format!("Raise your mic about {:.0} dB", diff.round()),
                    "OBS › Audio Mixer › your mic",
                    "Voice clearly on top",
                    "Background noise gets louder too",
                ),
                opt(
                    "desk_down",
                    format!("Lower game audio about {:.0} dB", diff.round()),
                    "OBS › Audio Mixer › Desktop Audio",
                    "Voice on top, no extra noise",
                    "Game sounds quieter for viewers",
                ),
            ];
            suggest(&mut opts, "mic_up");
            issues.push(Issue {
                id: "audio_balance".into(),
                priority: Priority::Low,
                area: "Audio".into(),
                title: format!("Your mic is quieter than the game, by about {diff:.0} dB."),
                why: mic_gap_meaning(diff).into(),
                who: "Anyone who talks to chat while playing.".into(),
                learn: "Viewers usually want your voice on top. Raising the mic is better if your mic is clean; if it picks up background noise, lowering the game avoids boosting that noise.".into(),
                measured: true,
                options: opts,
            });
        }
        (Some(_), Some(_)) => a.checks.push(Check {
            name: "Audio".into(),
            status: Status::Ok,
            label: "Balanced".into(),
            metric: "Voice on top".into(),
            note: "Your voice sits above game audio.".into(),
            measured: true,
        }),
        _ => {
            let muted = mics.iter().any(|m| m.muted == Some(true));
            a.checks.push(Check {
                name: "Audio".into(),
                status: if muted { Status::Warn } else { Status::Ok },
                label: if muted {
                    "Mic muted".into()
                } else {
                    "Found".into()
                },
                metric: format!(
                    "{} mic{}",
                    mics.len(),
                    if mics.len() == 1 { "" } else { "s" }
                ),
                note: if muted {
                    "Your mic is muted in OBS.".into()
                } else {
                    "Run a full test to compare mic and game volume.".into()
                },
                measured: false,
            });
        }
    }
}

fn hardware_and_heat(c: &Ctx, a: &mut Analysis, issues: &mut Vec<Issue>) {
    if let Some(h) = c.hw {
        let gpu = h
            .main_gpu()
            .map(|g| g.name.clone())
            .unwrap_or_else(|| "No GPU found".into());
        let good = h.ram_gb >= 15.0 && h.cpu_threads >= 8 && h.has_hw_encoder();
        a.checks.push(Check {
            name: "Hardware".into(),
            status: if good { Status::Ok } else { Status::Warn },
            label: if good {
                "Good".into()
            } else {
                "Limited".into()
            },
            metric: gpu,
            note: format!(
                "{} · {} threads · {:.0} GB RAM",
                h.cpu, h.cpu_threads, h.ram_gb
            ),
            measured: true,
        });
    }
    if let Some(t) = c.test {
        let hot = t.gpu_temp_max.map(|x| x >= 87.0).unwrap_or(false)
            || t.gpu_throttle.is_some()
            || t.cpu_temp_max.map(|x| x >= 95.0).unwrap_or(false);
        if hot {
            let mut opts = vec![
                opt(
                    "air",
                    "Improve airflow and clean dust",
                    "Case fans, filters and the GPU cooler",
                    "Lower temperatures, full speed",
                    "A little time and effort",
                ),
                opt(
                    "cap",
                    format!("Cap FPS at {}", c.st.monitor_hz),
                    "In the game's video settings",
                    "Less heat from less work",
                    "Nothing visible at your refresh rate",
                ),
                opt(
                    "fans",
                    "Use a more aggressive fan curve",
                    "GPU vendor software (e.g. MSI Afterburner, AMD Adrenalin)",
                    "Cooler under load",
                    "Louder fans",
                ),
            ];
            suggest(&mut opts, "cap");
            issues.push(Issue {
                id: "heat".into(),
                priority: Priority::Medium,
                area: "System".into(),
                title: format!(
                    "Your PC got hot{}.",
                    t.gpu_temp_max.map(|x| format!(", with the GPU reaching {x:.0}°C")).unwrap_or_default()
                ),
                why: format!(
                    "Hot parts slow themselves down to stay safe, which settings can't fully fix. Peaks: GPU {}, CPU {}{}.",
                    t.gpu_temp_max.map(|x| format!("{x:.0}°C")).unwrap_or_else(|| "unknown".into()),
                    t.cpu_temp_max.map(|x| format!("{x:.0}°C")).unwrap_or_else(|| "unknown".into()),
                    if t.gpu_throttle.is_some() { ", and the GPU reported thermal slowdown" } else { "" }
                ),
                who: "Laptops and small or dusty cases especially.".into(),
                learn: "Hot parts lower their own speed to stay safe. Settings changes can't fully fix that; cooling can.".into(),
                measured: true,
                options: opts,
            });
        }
    }
}

fn scene(c: &Ctx, a: &mut Analysis, issues: &mut Vec<Issue>) {
    let Some(o) = c.obs.filter(|o| o.connected) else {
        a.checks.push(Check {
            name: "OBS scene".into(),
            status: Status::Unknown,
            label: "Not connected".into(),
            metric: "—".into(),
            note: "Connect OBS to check your scene.".into(),
            measured: false,
        });
        return;
    };
    // Only a definite whole-screen capture, and only when the game isn't captured on its own.
    let display = o.uses_display_capture() && !o.uses_game_capture();
    a.checks.push(Check {
        name: "OBS scene".into(),
        status: if display { Status::Warn } else { Status::Ok },
        label: if display {
            (if cfg!(windows) {
                "Privacy risk"
            } else {
                "Check capture"
            })
            .into()
        } else {
            "Good".into()
        },
        metric: format!(
            "{} source{}",
            o.sources.len(),
            if o.sources.len() == 1 { "" } else { "s" }
        ),
        note: if display {
            if cfg!(windows) {
                "Display capture can show messages and notifications to viewers.".into()
            } else {
                "Screen capture may show your whole screen to viewers.".into()
            }
        } else {
            "Scene sources look fine.".into()
        },
        measured: true,
    });
    if display {
        let linux = cfg!(not(windows));
        let mut opts = if linux {
            vec![
                opt(
                    "pw_window",
                    "Capture only the game window",
                    "OBS › Sources › your Screen Capture (PipeWire) › Properties › Open Selector › pick the game's Window",
                    "Only the game is shown",
                    "Pick the window again if the game restarts",
                ),
                opt(
                    "vkcapture",
                    "Use the obs-vkcapture plugin (Game Capture for Linux)",
                    "Install obs-vkcapture, then OBS › Sources › + › Game Capture, and launch the game with obs-gamecapture %command% in Steam's launch options",
                    "Only the game is shown, with low overhead",
                    "A plugin to install and a launch option per game",
                ),
                opt(
                    "dnd",
                    "Keep it, turn on Do Not Disturb",
                    "KDE: system tray › Notifications › Do not disturb  ·  GNOME: top bar › Do Not Disturb",
                    "Fewer surprises on stream",
                    "Still shows anything you open, like Discord",
                ),
            ]
        } else {
            vec![
                opt(
                    "game_capture",
                    "Use Game Capture (or Window Capture) instead",
                    "OBS › Sources › + › Game Capture",
                    "Only the game is shown",
                    "Need to set it up once per game",
                ),
                opt(
                    "dnd",
                    "Keep Display Capture, turn on Do Not Disturb",
                    "Windows › Settings › System › Notifications › Do not disturb",
                    "Fewer surprises on stream",
                    "Still shows anything you open, like Discord",
                ),
            ]
        };
        suggest(&mut opts, if linux { "pw_window" } else { "game_capture" });
        issues.push(Issue {
            id: "display_capture".into(),
            priority: Priority::Medium,
            area: "Privacy".into(),
            title: if linux { "Your scene may show your whole screen".into() } else { "Your whole screen is on stream".into() },
            why: if linux {
                "Your scene uses Screen Capture (PipeWire). If it was set to a whole screen rather than one window, Discord messages, notifications and any window you open will be visible to viewers. Preflight can't tell which one you picked.".into()
            } else {
                "Your current scene uses Display Capture, so Discord messages, notifications and any window you open will be visible to viewers.".into()
            },
            who: "Everyone capturing a whole screen for games.".into(),
            learn: if linux {
                "On Wayland, OBS captures through PipeWire, which can share a whole screen or a single window. Capturing just the game window (or using obs-vkcapture) keeps everything else private.".into()
            } else {
                "Game Capture only shows the game window, which is safer and often lighter on performance.".into()
            },
            measured: true,
            options: opts,
        });
    }
}

fn background(c: &Ctx, issues: &mut Vec<Issue>) {
    if let Some(t) = c.test {
        let names: Vec<String> = t.hogs.iter().map(|h| h.label.clone()).collect();
        if !names.is_empty() {
            let mut opts = vec![opt(
                "close",
                "Pause or close them while you stream",
                names.join(", "),
                "More bandwidth and CPU for the stream",
                "Updates and syncing wait until later",
            )];
            suggest(&mut opts, "close");
            issues.push(Issue {
                id: "hogs".into(),
                priority: Priority::Low,
                area: "System".into(),
                title: format!("{} background program{} ran during the test.", names.len(), if names.len() == 1 { "" } else { "s" }),
                why: format!("{}. They can use upload bandwidth or CPU, which shows up as dropped frames.", names.join(", ")),
                who: "Anyone with launchers, cloud sync or updates running.".into(),
                learn: "Launchers, cloud sync and updates can quietly use upload bandwidth or CPU, which shows up as dropped frames.".into(),
                measured: true,
                options: opts,
            });
        }
        if t.output_total > 0 && t.output_skipped as f32 / t.output_total as f32 > 0.01 {
            let pct = t.output_skipped as f32 * 100.0 / t.output_total as f32;
            let mut opts = vec![
                opt(
                    "hw",
                    "Use your GPU encoder, or a faster preset",
                    "OBS › Settings › Output",
                    "Encoder keeps up",
                    "Slightly lower quality at the same bitrate",
                ),
                opt(
                    "res",
                    "Lower output resolution or FPS",
                    "OBS › Settings › Video",
                    "Less work per second",
                    "Softer or less smooth video",
                ),
            ];
            suggest(&mut opts, "hw");
            issues.push(Issue {
                id: "encoder_lag".into(),
                priority: Priority::High,
                area: "OBS".into(),
                title: format!("OBS skipped {pct:.1}% of stream frames, {}.", one_in(pct / 100.0)),
                why: "The encoder couldn't keep up, so viewers would see stutters.".into(),
                who: "Anyone whose encoder is overloaded.".into(),
                learn: "\"Skipped due to encoding lag\" means the encoder couldn't keep up. Viewers see this as stutter.".into(),
                measured: true,
                options: opts,
            });
        }
    }
    if let Some(o) = c.obs
        && let Some(major) = o
            .obs_version
            .split('.')
            .next()
            .and_then(|m| m.parse::<u32>().ok())
        && major < 30
    {
        let mut opts = vec![opt(
            "update",
            "Update OBS",
            "obsproject.com",
            "Newer encoders and fixes",
            "A few minutes to install",
        )];
        suggest(&mut opts, "update");
        issues.push(Issue {
            id: "obs_old".into(),
            priority: Priority::Low,
            area: "OBS".into(),
            title: format!("OBS {} is out of date.", o.obs_version),
            why: "Newer OBS versions include encoder improvements and bug fixes.".into(),
            who: "Everyone.".into(),
            learn: "Updating OBS keeps your scenes and settings.".into(),
            measured: false,
            options: opts,
        });
    }
}

#[cfg(test)]
mod quality_tests {
    use super::*;

    pub(super) fn snap() -> ObsSnapshot {
        ObsSnapshot {
            connected: true,
            output_mode: "Advanced".into(),
            encoder_raw: "obs_nvenc_h264_tex".into(),
            encoder_kind: "NVIDIA NVENC".into(),
            rate_control: "CBR".into(),
            preset: "p5".into(),
            keyint_sec: Some(2),
            scale_type: "bicubic".into(),
            base_res: Some((2560, 1440)),
            output_res: Some((1920, 1080)),
            fps: Some(60.0),
            bitrate_kbps: Some(6000),
            ..Default::default()
        }
    }

    pub(super) fn ids(o: &ObsSnapshot) -> Vec<String> {
        let st = Settings::default();
        analyze(&st, None, &[], Some(o), None, None, false)
            .issues
            .into_iter()
            .map(|i| i.id)
            .collect()
    }

    #[test]
    fn good_settings_raise_nothing() {
        let i = ids(&snap());
        for id in ["rate_control", "keyframes", "preset", "downscale"] {
            assert!(
                !i.contains(&id.to_string()),
                "{id} flagged on good settings"
            );
        }
    }

    #[test]
    fn bad_settings_are_flagged() {
        let mut o = snap();
        o.rate_control = "CQP".into();
        o.keyint_sec = Some(0);
        o.ignore_recommended = Some(true);
        o.preset = "p1".into();
        o.scale_type = "bilinear".into();
        let i = ids(&o);
        for id in ["rate_control", "keyframes", "preset", "downscale"] {
            assert!(i.contains(&id.to_string()), "{id} not flagged");
        }
    }

    #[test]
    fn simple_mode_is_not_flagged_for_rate_or_keyframes() {
        let mut o = snap();
        o.output_mode = "Simple".into();
        o.rate_control = "CBR".into();
        o.keyint_sec = Some(2);
        let i = ids(&o);
        assert!(!i.contains(&"rate_control".to_string()) && !i.contains(&"keyframes".to_string()));
    }

    #[test]
    fn recording_checks() {
        let mut o = snap();
        o.rec_encoder_raw = "obs_nvenc_h264_tex".into();
        o.rec_encoder_kind = "NVIDIA NVENC".into();
        o.rec_preset = "p7".into();
        o.rec_format = "mp4".into();
        o.obs_version = "31.0.2".into();
        o.multitrack = Some(true);
        o.disk_free_mb = Some(2000.0);
        let i = ids(&o);
        for id in ["rec_encoder", "rec_format", "multitrack", "disk_space"] {
            assert!(i.contains(&id.to_string()), "{id} not flagged: {i:?}");
        }
        // Enhanced Broadcasting sets bitrate/preset itself, so those checks step aside.
        o.rate_control = "VBR".into();
        assert!(!ids(&o).contains(&"rate_control".to_string()));
    }

    #[test]
    fn shared_recording_is_fine() {
        let mut o = snap();
        o.rec_format = "hybrid_mp4".into();
        o.disk_free_mb = Some(500_000.0);
        let i = ids(&o);
        for id in ["rec_encoder", "rec_format", "disk_space", "multitrack"] {
            assert!(!i.contains(&id.to_string()), "{id} flagged on a good setup");
        }
    }

    #[test]
    fn places_and_presets() {
        assert_eq!(place_of("gpu_maxed"), Place::Game);
        assert_eq!(place_of("disk_space"), Place::Pc);
        assert_eq!(place_of("keyframes"), Place::Obs);
        assert!(preset_rank("p7") > preset_rank("p4"));
        assert_eq!(preset_rank("veryfast"), Some(3));
        assert_eq!(
            crate::hardware::nvenc_chips("NVIDIA GeForce RTX 4090"),
            Some(2)
        );
        assert_eq!(
            crate::hardware::nvenc_chips("NVIDIA GeForce RTX 4070"),
            Some(1)
        );
        assert_eq!(crate::hardware::nvenc_chips("AMD Radeon RX 7800 XT"), None);
    }

    /// Recreates the build 5 report: RTX 5090 at 98%, OBS kept up, game not
    /// recognised, only a mic in OBS, separate HEVC recording, Hybrid MOV.
    #[test]
    fn build5_report_case() {
        let mut o = snap();
        o.connected = true;
        o.keyint_sec = Some(0);
        o.rec_encoder_raw = "obs_nvenc_hevc_tex".into();
        o.rec_encoder_kind = "NVIDIA NVENC".into();
        o.rec_preset = "p5".into();
        o.rec_format = "hybrid_mov".into();
        o.recording_uses_stream_settings = Some(false);
        o.audio = vec![crate::obs::AudioInput {
            name: String::new(),
            kind: "pulse_input_capture".into(),
            role: "mic".into(),
            muted: Some(false),
            volume_db: Some(0.0),
            device: String::new(),
        }];
        let t = TestResult {
            gpu_avg: Some(97.6),
            output_total: 8158,
            output_skipped: 2,
            render_total: 8158,
            render_skipped: 2,
            mic_db: Some(-47.0),
            cpu_avg: 18.0,
            seconds: 137.0,
            ..Default::default()
        };
        let st = Settings {
            monitor_hz: 480,
            ..Settings::default()
        };
        let a = analyze(&st, None, &[], Some(&o), None, Some(&t), true);
        let get = |id: &str| a.issues.iter().find(|i| i.id == id);
        let g = get("gpu_maxed").expect("GPU at 98% must be shown even after a settings change");
        assert_eq!(g.priority, Priority::Medium, "OBS kept up, so not High");
        assert!(
            g.options.iter().any(|o| o.name.contains("120")),
            "60 FPS stream: cap at 120, not half of 480 Hz"
        );
        assert!(get("no_game_audio").is_some());
        assert!(get("mic_quiet").is_some());
        assert!(
            get("recording_mismatch").is_none(),
            "separate GPU recording is fine; the Test step offers stream + record"
        );
        assert!(get("rec_format").is_none(), "Hybrid MOV is crash-safe");
        let kf = a
            .recommended
            .iter()
            .find(|r| r.setting == "Keyframe interval")
            .unwrap();
        assert!(!kf.ok, "auto keyframes: row and issue must agree");
    }

    #[test]
    fn cap_targets() {
        assert_eq!(cap_target(480, Some(60.0), Style::Balanced, None), 120);
        assert_eq!(cap_target(480, Some(30.0), Style::Balanced, None), 60);
        assert_eq!(cap_target(144, Some(60.0), Style::Competitive, None), 144);
        assert_eq!(
            cap_target(480, Some(60.0), Style::Competitive, Some(150.0)),
            120
        );
        assert_eq!(cap_target(480, Some(60.0), Style::Balanced, Some(95.0)), 72);
    }

    /// The user's real Cyberpunk settings (Nov 14): capped at 240 on a 480 Hz
    /// monitor, Custom preset, GPU maxed. The cap never kicks in, so the fix
    /// is a lower cap with exact values, then the heavy settings.
    #[test]
    fn cyberpunk_custom_cap_too_high() {
        use crate::games::*;
        let g = GameResult {
            id: "cyberpunk2077".into(),
            name: "Cyberpunk 2077".into(),
            load: "gpu".into(),
            profile_version: 4,
            verified: String::new(),
            installed: true,
            install_hint: String::new(),
            running: true,
            platform_note: None,
            files: vec![],
            findings: vec![],
            unmatched_rules: vec![],
            key_names: vec![],
            current: vec![CurrentSetting {
                rule_id: "fps_uncapped".into(),
                setting: "FPS limit".into(),
                effect: "uncapped_fps".into(),
                menu: "Cyberpunk 2077 › Settings › Video › Maximum FPS".into(),
                value: "240".into(),
                flagged: false,
                change_to: None,
                gain: None,
                cost: None,
            }],
            preset: Some(PresetState {
                setting: "Quick preset".into(),
                menu: "m".into(),
                value: "Custom".into(),
                custom: true,
                lower: None,
            }),
            tunables: vec![
                TunableState {
                    id: "ssr".into(),
                    setting: "Screen Space Reflections Quality".into(),
                    menu: "m".into(),
                    now: "Ultra".into(),
                    change_to: Some("High".into()),
                    gain: "g".into(),
                    cost: "c".into(),
                },
                TunableState {
                    id: "fog".into(),
                    setting: "Volumetric Fog Resolution".into(),
                    menu: "m".into(),
                    now: "Ultra".into(),
                    change_to: Some("High".into()),
                    gain: "g".into(),
                    cost: "c".into(),
                },
            ],
            fps_cap: Some(240),
            unreadable: vec![],
            has_fps_limit: true,
            api: Some("dx12".into()),
            proton: true,
            display_res: None,
            display_hz: None,
            ..Default::default()
        };
        let mut o = snap();
        o.connected = true;
        o.fps = Some(60.0);
        let t = TestResult {
            gpu_avg: Some(97.0),
            output_total: 8000,
            output_skipped: 2,
            render_total: 8000,
            seconds: 130.0,
            game_running: Some("Cyberpunk 2077".into()),
            ..Default::default()
        };
        let st = Settings {
            monitor_hz: 480,
            ..Settings::default()
        };
        let a = analyze(&st, None, &[g], Some(&o), None, Some(&t), false);
        let i = a
            .issues
            .iter()
            .find(|i| i.id == "gpu_maxed")
            .expect("GPU maxed");
        let cap = i
            .options
            .iter()
            .find(|o| o.id == "cap")
            .expect("lower the cap");
        assert!(cap.suggested);
        assert_eq!(cap.changes[0].now.as_deref(), Some("240"));
        assert_eq!(cap.changes[0].to, "120");
        let tune = i
            .options
            .iter()
            .find(|o| o.id == "tune")
            .expect("heavy settings for Custom");
        assert_eq!(tune.changes.len(), 2);
        assert!(!i.options.iter().any(|o| o.name.contains("one step")));
        // Your setup shows the same change, from the same list.
        let row = a
            .setup
            .game
            .iter()
            .find(|r| r.setting == "FPS limit")
            .unwrap();
        assert_eq!(
            (row.now.as_str(), row.suggested.as_str(), row.tag),
            ("240", "120", Tag::Change)
        );
        assert!(a.smoothness.is_some());
    }

    /// Build 7 report: GPU ~70% for a minute then 97% for 40 s (avg 84%).
    #[test]
    fn gpu_maxed_by_time_not_average() {
        let o = snap();
        let mut samples = Vec::new();
        for i in 0..136 {
            samples.push(crate::hardware::Sample {
                t: i as f32,
                cpu: 30.0,
                gpu: Some(if i < 90 { 72.0 } else { 97.0 }),
                ..Default::default()
            });
        }
        let t = TestResult {
            samples,
            gpu_avg: Some(84.0),
            output_total: 8068,
            output_skipped: 2,
            render_total: 8068,
            render_skipped: 2,
            seconds: 136.0,
            ..Default::default()
        };
        let a = analyze(
            &Settings::default(),
            None,
            &[],
            Some(&o),
            None,
            Some(&t),
            false,
        );
        let g = a
            .issues
            .iter()
            .find(|i| i.id == "gpu_maxed")
            .expect("maxed for a third of the test must be flagged");
        assert!(g.title.contains("% of the test"), "{}", g.title);
    }

    #[test]
    fn game_audio_unknown_without_obs() {
        let mut o = snap();
        o.connected = false;
        o.audio.clear();
        let a = analyze(&Settings::default(), None, &[], Some(&o), None, None, false);
        let r = a
            .setup
            .obs
            .iter()
            .find(|r| r.setting == "Game audio")
            .unwrap();
        assert_eq!(r.tag, Tag::Unknown);
        assert_eq!(r.now, "Unknown");
        o.connected = true;
        let a = analyze(&Settings::default(), None, &[], Some(&o), None, None, false);
        let r = a
            .setup
            .obs
            .iter()
            .find(|r| r.setting == "Game audio")
            .unwrap();
        assert_eq!(r.now, "None");
        assert_ne!(r.tag, Tag::Unknown);
    }

    #[test]
    fn preset_words_match_everywhere() {
        assert_eq!(nvenc_preset_words(None, false), "P6 or P7");
        assert_eq!(
            nvenc_preset_words(Some("SHRED A BILLION"), false),
            "P6 or P7 (P5 for SHRED A BILLION)"
        );
        assert_eq!(nvenc_preset_words(Some("X"), true), "P5");
    }

    #[test]
    fn setup_view_covers_every_issue() {
        let mut o = snap();
        o.rate_control = "VBR".into();
        o.audio = vec![crate::obs::AudioInput {
            name: String::new(),
            kind: "pulse_input_capture".into(),
            role: "mic".into(),
            muted: Some(false),
            volume_db: None,
            device: String::new(),
        }];
        let t = TestResult {
            gpu_avg: Some(97.0),
            output_total: 1000,
            output_skipped: 20,
            render_total: 1000,
            seconds: 120.0,
            hogs: vec![crate::hardware::Hog {
                label: "Dropbox".into(),
                cpu: 30.0,
            }],
            ..Default::default()
        };
        let mut st = Settings::default();
        st.kept_issues.push("rate_control".into());
        let a = analyze(&st, None, &[], Some(&o), None, Some(&t), false);
        let rows: Vec<&SetupRow> = a
            .setup
            .obs
            .iter()
            .chain(a.setup.game.iter())
            .chain(a.setup.pc.iter())
            .collect();
        for i in &a.issues {
            assert!(
                rows.iter()
                    .any(|r| r.issue_id.as_deref() == Some(i.id.as_str())),
                "issue {} has no row",
                i.id
            );
        }
        let rc = a
            .setup
            .obs
            .iter()
            .find(|r| r.setting == "Rate control")
            .unwrap();
        assert_eq!(rc.tag, Tag::Kept);
        assert!(a.setup.summary.contains("change"), "{}", a.setup.summary);
        assert!(!a.verdict.is_empty());
    }

    #[test]
    fn no_downscale_issue_when_not_shrinking() {
        let mut o = snap();
        o.base_res = Some((1920, 1080));
        o.scale_type = "bilinear".into();
        assert!(!ids(&o).contains(&"downscale".to_string()));
    }
}

#[cfg(test)]
mod cities_advice_tests {
    use super::quality_tests::snap;
    use super::*;
    use crate::games::*;

    /// Cities: Skylines II with the GPU maxed: no in-game FPS cap is ever
    /// suggested; the heavy settings are, with exact values.
    #[test]
    fn cities_no_ingame_cap() {
        let m = crate::games::parse("coc", include_str!("testdata/cities2_settings.coc")).unwrap();
        let _ = m;
        let g = GameResult {
            id: "cities_skylines_2".into(),
            name: "Cities: Skylines II".into(),
            load: "cpu_gpu".into(),
            profile_version: 2,
            verified: String::new(),
            installed: true,
            install_hint: String::new(),
            running: true,
            platform_note: None,
            files: vec![],
            findings: vec![],
            unmatched_rules: vec![],
            key_names: vec![],
            current: vec![],
            preset: None,
            tunables: vec![TunableState {
                id: "dof".into(),
                setting: "Depth of field".into(),
                menu: "m".into(),
                now: "On".into(),
                change_to: Some("Off".into()),
                gain: "g".into(),
                cost: "c".into(),
            }],
            fps_cap: None,
            unreadable: vec![],
            has_fps_limit: false,
            api: Some("dx11".into()),
            proton: true,
            display_res: None,
            display_hz: Some(60.0),
            ..Default::default()
        };
        let mut o = snap();
        o.connected = true;
        o.fps = Some(60.0);
        let t = TestResult {
            gpu_avg: Some(97.0),
            output_total: 8000,
            render_total: 8000,
            seconds: 130.0,
            game_running: Some("Cities: Skylines II".into()),
            ..Default::default()
        };
        let st = Settings {
            monitor_hz: 480,
            ..Settings::default()
        };
        let a = analyze(&st, None, &[g], Some(&o), None, Some(&t), false);
        let i = a
            .issues
            .iter()
            .find(|i| i.id == "gpu_maxed")
            .expect("GPU maxed");
        assert!(
            !i.options.iter().any(|o| o.menu.contains("Video (Max FPS")),
            "no in-game cap for Cities"
        );
        let cap = i
            .options
            .iter()
            .find(|o| o.id == "cap")
            .expect("outside cap");
        if cfg!(not(windows)) {
            assert!(cap.changes[0].to.contains("DXVK_FRAME_RATE=120"));
        }
        assert!(i.options.iter().any(|o| o.id == "tune"));
        assert!(
            a.setup
                .game
                .iter()
                .any(|r| r.setting == "Game display mode" && r.why.contains("480"))
        );
    }

    #[test]
    fn style_order() {
        let o = |id: &str| opt(id, id, "m", "g", "c");
        let opts = vec![o("cap"), o("upscale"), o("tune")];
        let mut st = Settings::default();
        st.apply_style(Style::Competitive);
        assert_eq!(
            style_pick(&st, &opts),
            "tune",
            "Competitive lowers graphics first"
        );
        st.apply_style(Style::Balanced);
        assert_eq!(style_pick(&st, &opts), "upscale");
        st.apply_style(Style::Cinematic);
        assert_eq!(style_pick(&st, &opts), "cap", "Cinematic caps first");
        // Nothing to lower: Competitive falls back to DLSS/FSR, then the cap.
        st.apply_style(Style::Competitive);
        assert_eq!(style_pick(&st, &[o("cap"), o("upscale")]), "upscale");
        assert_eq!(style_pick(&st, &[o("cap")]), "cap");
        assert!(unscored("gpu_near"));
    }

    #[test]
    fn picture_for_bitrate() {
        assert_eq!(needed_kbps(1920, 1080, 60.0, "high", "H.264"), 6000);
        assert_eq!(
            best_height((2560, 1440), 6000, 60.0, "high", "H.264", 2160),
            (1920, 1080)
        );
        assert_eq!(
            best_height((2560, 1440), 6000, 60.0, "high", "H.264", 2160).1,
            1080
        );
        assert_eq!(
            best_height((2560, 1440), 12000, 60.0, "high", "H.264", 2160).1,
            1440,
            "1440p60 is fine at 12 Mbps"
        );
        assert_eq!(
            best_height((1920, 1080), 3500, 60.0, "high", "H.264", 2160).1,
            720
        );
        // Twitch, 1440p60 at 6000 kbps: suggest 1080p with Lanczos, never 30 FPS.
        let mut o = snap();
        o.output_res = Some((2560, 1440));
        let st = Settings::default();
        let a = analyze(&st, None, &[], Some(&o), None, None, false);
        let i = a
            .issues
            .iter()
            .find(|i| i.id == "resolution_bitrate")
            .expect("1440p at 6000 flagged");
        let d = i.options.iter().find(|o| o.suggested).unwrap();
        assert_eq!(d.id, "res_down");
        assert_eq!(d.changes[0].to, "1920x1080");
        assert!(d.changes.iter().any(|c| c.to.starts_with("Lanczos")));
        assert!(!i.options.iter().any(|o| o.name.contains("30")));
        assert!(
            i.options.iter().any(|o| o.id == "eb"),
            "Twitch: Enhanced Broadcasting offered"
        );
        assert_eq!(a.recommended_output, "1080p60 · 6000 kbps");
        let row = a
            .setup
            .obs
            .iter()
            .find(|r| r.setting.starts_with("Output"))
            .unwrap();
        assert_ne!(
            row.tag,
            Tag::Keep,
            "the output row isn't 'Keep' while it's too big"
        );
        // 1080p60 at 6000 is fine.
        assert!(!quality_tests::ids(&snap()).contains(&"resolution_bitrate".to_string()));
        // NVENC options: with GPU headroom in the test, the slowest preset and full two-pass.
        let mut o = snap();
        o.enc_multipass = "qres".into();
        o.enc_tune = "ll".into();
        let t = TestResult {
            gpu_avg: Some(49.0),
            output_total: 8000,
            seconds: 130.0,
            ..Default::default()
        };
        let a = analyze(&st, None, &[], Some(&o), None, Some(&t), false);
        let q = a
            .issues
            .iter()
            .find(|i| i.id == "enc_quality")
            .expect("quality options");
        let ch = &q.options[0].changes;
        assert!(
            ch.iter()
                .any(|c| c.setting == "Preset" && c.to.starts_with("P7"))
        );
        assert!(
            ch.iter()
                .any(|c| c.setting == "Multipass Mode" && c.to.contains("Full"))
        );
        assert!(
            ch.iter()
                .any(|c| c.setting == "Tuning" && c.to == "High Quality")
        );
        // A busy GPU gets the lighter options.
        let t = TestResult {
            gpu_avg: Some(97.0),
            output_total: 8000,
            seconds: 130.0,
            ..Default::default()
        };
        let a = analyze(&st, None, &[], Some(&o), None, Some(&t), false);
        let q = a.issues.iter().find(|i| i.id == "enc_quality").unwrap();
        assert!(!q.options[0].changes.iter().any(|c| c.to.starts_with("P7")));
        // Audio
        let mut o = snap();
        o.audio_kbps = Some(96);
        o.sample_rate = Some(44100);
        let v = quality_tests::ids(&o);
        assert!(v.contains(&"audio_bitrate".to_string()) && v.contains(&"sample_rate".to_string()));
    }

    #[test]
    fn p6_is_fine_and_changes_are_named() {
        let mut o = snap();
        o.preset = "p6".into();
        o.enc_multipass = "fullres".into();
        let t = TestResult {
            gpu_avg: Some(43.0),
            output_total: 8000,
            seconds: 130.0,
            ..Default::default()
        };
        let a = analyze(
            &Settings::default(),
            None,
            &[],
            Some(&o),
            None,
            Some(&t),
            false,
        );
        assert!(
            !a.issues.iter().any(|i| i.id == "enc_quality"),
            "P6 with full two-pass needs nothing"
        );
        let row = a.setup.obs.iter().find(|r| r.setting == "Preset").unwrap();
        assert_eq!(row.tag, Tag::Keep);
        let mut n = o.clone();
        n.preset = "p7".into();
        n.bitrate_kbps = Some(8000);
        let d = o.enc_settings().changes_to(&n.enc_settings());
        assert!(d.contains(&"Preset P6 → P7".to_string()), "{d:?}");
        assert!(
            d.iter()
                .any(|x| x.starts_with("Bitrate (kbps) 6000 → 8000"))
        );
    }

    #[test]
    fn plugin_outputs_and_encode_budget() {
        use crate::obs::{OutputInfo, SourceRecordInfo, classify_output};
        assert_eq!(classify_output("rtmp_output", "adv_stream").0, "Stream");
        assert_eq!(
            classify_output("rtmp_output", "multi-rtmp YouTube 1"),
            (
                "Extra stream (YouTube)".to_string(),
                true,
                Some("YouTube".to_string())
            )
        );
        assert_eq!(
            classify_output("rtmp_output", "vertical_canvas_stream").0,
            "Vertical canvas stream"
        );
        assert_eq!(
            classify_output("ffmpeg_muxer", "Source Record 3").0,
            "Source Record"
        );
        // A 1-chip card running stream + separate recording + vertical stream + a Source Record.
        let mut o = snap();
        o.recording_uses_stream_settings = Some(false);
        o.rec_encoder_kind = "NVIDIA NVENC".into();
        o.outputs = vec![OutputInfo {
            kind: "rtmp_output".into(),
            label: "Vertical canvas stream".into(),
            plugin: true,
            ..Default::default()
        }];
        o.source_records = vec![SourceRecordInfo {
            mode: "Always".into(),
            encoder: "CPU (x264)".into(),
            enabled: true,
            bitrate_kbps: None,
        }];
        let list = encode_list(&o, 0);
        assert_eq!(list.len(), 4, "{list:?}");
        let hw = crate::hardware::HardwareInfo {
            os: String::new(),
            os_family: "Linux",
            kernel: String::new(),
            session: String::new(),
            cpu: String::new(),
            cpu_physical_cores: None,
            cpu_threads: 16,
            ram_gb: 32.0,
            gpus: vec![crate::hardware::Gpu {
                name: "NVIDIA GeForce RTX 3060".into(),
                vendor: crate::hardware::Vendor::Nvidia,
                vram_mb: None,
                driver: None,
            }],
            encoders: vec!["NVIDIA NVENC".into()],
            notes: vec![],
        };
        let a = analyze(
            &Settings::default(),
            Some(&hw),
            &[],
            Some(&o),
            None,
            None,
            false,
        );
        let i = a
            .issues
            .iter()
            .find(|i| i.id == "encode_budget")
            .expect("3 GPU encodes on 1 chip");
        assert!(i.title.contains("3 GPU encodes"));
        assert!(
            a.issues.iter().any(|i| i.id == "cpu_encodes"),
            "Source Record on x264 flagged"
        );
    }

    #[test]
    fn several_platforms() {
        let hw = |gpu: &str| crate::hardware::HardwareInfo {
            os: String::new(),
            os_family: "Linux",
            kernel: String::new(),
            session: String::new(),
            cpu: String::new(),
            cpu_physical_cores: None,
            cpu_threads: 16,
            ram_gb: 32.0,
            gpus: vec![crate::hardware::Gpu {
                name: gpu.into(),
                vendor: crate::hardware::Vendor::Nvidia,
                vram_mb: None,
                driver: None,
            }],
            encoders: vec!["NVIDIA NVENC".into()],
            notes: vec![],
        };
        let st = Settings {
            platforms: vec!["Twitch".into(), "YouTube".into()],
            multistream_how: "plugin".into(),
            ..Default::default()
        };
        let mut o = snap();
        o.base_res = Some((2560, 1440));
        o.output_res = Some((1920, 1080));
        o.recording_uses_stream_settings = Some(false);
        o.rec_encoder_kind = "NVIDIA NVENC".into();
        // RTX 5090: 3 chips for stream + recording + YouTube.
        let big = hw("NVIDIA GeForce RTX 5090");
        let a = analyze(&st, Some(&big), &[], Some(&o), None, None, false);
        let i = a.issues.iter().find(|i| i.id == "multistream").unwrap();
        let s = i.options.iter().find(|o| o.suggested).unwrap();
        assert_eq!(s.id, "separate");
        assert!(
            s.changes.iter().any(|c| c.setting == "YouTube"
                && c.to.starts_with("1440p60")
                && c.to.contains("AV1")),
            "{:?}",
            s.changes
        );
        // RTX 3060: one chip already busy with stream + recording: share.
        let a = analyze(
            &st,
            Some(&hw("NVIDIA GeForce RTX 3060")),
            &[],
            Some(&o),
            None,
            None,
            false,
        );
        assert_eq!(
            a.issues
                .iter()
                .find(|i| i.id == "multistream")
                .unwrap()
                .options
                .iter()
                .find(|o| o.suggested)
                .unwrap()
                .id,
            "shared"
        );
        // A slow connection: a restream service.
        let net = crate::network::NetResult {
            ok: true,
            avg_mbps: 9.0,
            low_mbps: 8.0,
            high_mbps: 10.0,
            steadiness: 0.9,
            ..Default::default()
        };
        let a = analyze(&st, Some(&big), &[], Some(&o), Some(&net), None, false);
        assert_eq!(
            a.issues
                .iter()
                .find(|i| i.id == "multistream")
                .unwrap()
                .options
                .iter()
                .find(|o| o.suggested)
                .unwrap()
                .id,
            "service"
        );
    }
}

/// Two-PC checks, from what OBS reports about its capture sources.
fn two_pc(c: &Ctx, issues: &mut Vec<Issue>) {
    use crate::twopc::Role;
    let role = Role::from_str(&c.st.pc_role);
    if role == Role::One {
        return;
    }
    let Some(o) = c.obs.filter(|o| o.connected) else {
        return;
    };
    let out_fps = o.fps.unwrap_or(60.0);
    let out_h = o.output_res.map(|r| r.1).unwrap_or(1080);
    let obs_range = o.color_range.to_lowercase();
    let source_menu = "OBS › right-click your capture source › Properties";
    for cap in o.captures.iter().filter(|c| c.in_scene) {
        if cap.via == "network" {
            issues.push(Issue {
                id: "ndi_cost".into(),
                priority: Priority::Low,
                area: "Two PCs".into(),
                title: "Your game picture comes over the network (NDI).".into(),
                why: "NDI compresses the picture on your gaming PC, which costs some of its CPU or GPU, and it needs a steady connection between the two PCs.".into(),
                who: "Two-PC setups without a capture card.".into(),
                learn: "Use a wired gigabit connection for both PCs; Wi-Fi drops frames. If the gaming PC struggles, a capture card moves that work off it completely.".into(),
                measured: false,
                options: vec![opt("wired", "Connect both PCs by cable", "Your router or switch", "Steady picture, fewer dropped frames", "A network cable").with(change("Network", "Both PCs", None, "Wired (gigabit)"))],
            });
            continue;
        }
        if let Some(f) = cap.fps
            && f + 1.0 < out_fps
        {
            let mut o1 = opt(
                "cap_fps",
                format!("Capture at {out_fps:.0} fps"),
                source_menu,
                "Smooth motion on stream",
                "None if the card supports it at this resolution",
            )
            .with(change(
                "FPS",
                source_menu,
                Some(format!("{f:.0}")),
                format!("{out_fps:.0}"),
            ));
            o1.suggested = true;
            issues.push(Issue {
                    id: "capture_fps".into(),
                    priority: Priority::High,
                    area: "Capture card".into(),
                    title: format!("Your capture card captures {f:.0} fps, but OBS streams {out_fps:.0} fps."),
                    why: "OBS repeats frames to fill the gap, so motion on stream looks choppy even when your game is smooth.".into(),
                    who: "Two-PC setups with a capture card.".into(),
                    learn: "Many cards only do 60 fps at 1080p and less at higher resolutions. Pick a resolution where the card can capture at your stream's frame rate.".into(),
                    measured: false,
                    options: vec![o1],
                });
        }
        if let Some((w, h)) = cap.res
            && h < out_h
        {
            let mut o1 = opt(
                "cap_res",
                format!("Capture at {}p or more", out_h),
                source_menu,
                "A sharper stream",
                "Some cards can't do the higher resolution at 60 fps",
            )
            .with(change(
                "Resolution",
                source_menu,
                Some(format!("{w}x{h}")),
                format!("{}p or higher", out_h),
            ));
            o1.suggested = true;
            issues.push(Issue {
                    id: "capture_res".into(),
                    priority: Priority::Medium,
                    area: "Capture card".into(),
                    title: format!("The capture is {w}x{h}, smaller than your {out_h}p stream."),
                    why: "OBS has to stretch the picture up, which makes the stream softer than it needs to be.".into(),
                    who: "Two-PC setups with a capture card.".into(),
                    learn: "Capturing at the stream's resolution (or higher, letting OBS scale down) gives the sharpest result.".into(),
                    measured: false,
                    options: vec![o1],
                });
        }
        if !cap.range.is_empty() && !obs_range.is_empty() && cap.range != obs_range {
            let to = if obs_range.contains("full") {
                "Full"
            } else {
                "Partial (limited)"
            };
            let mut o1 = opt(
                "cap_range",
                format!("Set the capture colour range to {to}"),
                format!("{source_menu} › Color Range"),
                "Correct blacks and colours",
                "None",
            )
            .with(change(
                "Color Range",
                format!("{source_menu} › Color Range"),
                Some(cap.range.clone()),
                to,
            ));
            o1.suggested = true;
            issues.push(Issue {
                id: "capture_range".into(),
                priority: Priority::High,
                area: "Capture card".into(),
                title: "Your capture card and OBS use different colour ranges.".into(),
                why: "One says full range, the other limited. Viewers see washed-out greys or crushed, too-dark blacks.".into(),
                who: "Two-PC setups with a capture card.".into(),
                learn: "Your gaming PC's graphics driver, the capture card and OBS (Settings › Advanced › Color Range) should all agree. Limited (partial) is the safe choice for streaming.".into(),
                measured: false,
                options: vec![o1],
            });
        }
        if cap.format.eq_ignore_ascii_case("MJPEG") && cap.res.map(|r| r.1 >= 1080).unwrap_or(false)
        {
            issues.push(Issue {
                id: "capture_mjpeg".into(),
                priority: Priority::Low,
                area: "Capture card".into(),
                title: "Your capture card sends MJPEG (compressed) video.".into(),
                why: "MJPEG loses a little detail and your streaming PC's CPU has to unpack every frame.".into(),
                who: "USB capture cards.".into(),
                learn: "If the card offers NV12 or YUY2 at the same resolution and frame rate, pick it. If it doesn't, MJPEG is fine.".into(),
                measured: false,
                options: vec![opt("cap_fmt", "Pick NV12 or YUY2", format!("{source_menu} › Video Format"), "Cleaner picture, less CPU", "Only if the card offers it at this resolution").with(change("Video Format", format!("{source_menu} › Video Format"), Some("MJPEG".into()), "NV12 or YUY2"))],
            });
        }
        let monitor = if role == Role::Gaming {
            c.st.monitor_hz
        } else {
            c.st.other_monitor_hz
        };
        if let Some(f) = cap.fps {
            let f = f.round() as u32;
            if monitor > 0 && f > 0 && monitor % f != 0 && monitor > f {
                issues.push(Issue {
                    id: "capture_judder".into(),
                    priority: Priority::Low,
                    area: "Two PCs".into(),
                    title: format!("Your gaming monitor runs at {monitor} Hz, but the card captures {f} fps."),
                    why: format!("{monitor} isn't a multiple of {f}, so some frames are held longer than others and motion judders on stream."),
                    who: "Two-PC setups with high refresh monitors.".into(),
                    learn: format!("Use a refresh rate that's a multiple of {f} (for example {} or {} Hz), or a card with high refresh passthrough that captures at {f}.", f * 2, f * 4),
                    measured: false,
                    options: vec![opt("hz", format!("Set the monitor to {} Hz", (monitor / f).max(1) * f), "Your gaming PC's display settings", "Even motion on stream", "A slightly lower refresh rate").with(change("Refresh rate", "Gaming PC › Display settings", Some(format!("{monitor} Hz")), format!("{} Hz", (monitor / f).max(1) * f)))],
                });
            }
        }
    }
    let h264 = o.encoder_kind.to_lowercase().contains("h.264")
        || o.encoder_raw.to_lowercase().contains("h264")
        || o.encoder_raw.to_lowercase().contains("x264");
    if o.color_space.contains("2100") && h264 {
        issues.push(Issue {
            id: "capture_hdr".into(),
            priority: Priority::Medium,
            area: "OBS".into(),
            title: "OBS uses an HDR colour space, but your stream is H.264.".into(),
            why: "H.264 streams are SDR, so the picture comes out grey and washed out for viewers.".into(),
            who: "HDR gaming PCs and capture cards.".into(),
            learn: "Stream in SDR (Rec. 709), or turn HDR off on the gaming PC while streaming. HDR streaming needs HEVC or AV1 and a platform that supports it.".into(),
            measured: false,
            options: vec![opt("sdr", "Use Rec. 709", "OBS › Settings › Advanced › Color Space", "Correct colours for viewers", "None").with(change("Color Space", "OBS › Settings › Advanced", Some(o.color_space.clone()), "Rec. 709"))],
        });
    }
}
