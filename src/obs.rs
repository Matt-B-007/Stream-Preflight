//! OBS Studio connector. Uses OBS's built-in WebSocket server (v5) to READ
//! settings and stats. It never changes anything in OBS and never starts or
//! stops recording or streaming. When OBS is closed it reads the saved
//! profile files instead, skipping service.json (which holds the stream key).

use base64::Engine;
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::net::TcpStream;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tungstenite::{Message, WebSocket};

const SUB_OUTPUTS: u32 = 1 << 6;
const SUB_VOLUME: u32 = 1 << 16;

#[derive(Debug)]
pub enum ObsError {
    NotRunning,
    AuthNeeded,
    AuthFailed,
    Other(String),
}

impl std::fmt::Display for ObsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ObsError::NotRunning => write!(
                f,
                "Couldn't reach OBS. Make sure OBS is open and its WebSocket server is on (OBS › Tools › WebSocket Server Settings › Enable WebSocket server)."
            ),
            ObsError::AuthNeeded => write!(
                f,
                "OBS asked for a password. Copy it from OBS › Tools › WebSocket Server Settings › Show Connect Info, and paste it into Preflight (Settings › OBS connection)."
            ),
            ObsError::AuthFailed => write!(
                f,
                "OBS rejected the password. Copy it again from OBS › Tools › WebSocket Server Settings › Show Connect Info."
            ),
            ObsError::Other(s) => write!(f, "OBS connection problem: {s}"),
        }
    }
}

pub struct Client {
    ws: WebSocket<TcpStream>,
    next_id: u64,
    pub events: VecDeque<Value>,
    pub ws_version: String,
}

impl Client {
    pub fn connect(host: &str, port: u16, password: Option<&str>) -> Result<Self, ObsError> {
        crate::logger::activity("connecting to OBS");
        let addr = format!("{host}:{port}");
        let sock_addr = std::net::ToSocketAddrs::to_socket_addrs(&addr)
            .map_err(|e| ObsError::Other(e.to_string()))?
            .next()
            .ok_or_else(|| ObsError::Other("bad address".into()))?;
        let stream = TcpStream::connect_timeout(&sock_addr, Duration::from_secs(2))
            .map_err(|_| ObsError::NotRunning)?;
        stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
        let url = format!("ws://{addr}");
        let (mut ws, _) = tungstenite::client(url.as_str(), stream)
            .map_err(|e| ObsError::Other(e.to_string()))?;

        let hello = read_json(&mut ws).map_err(|e| ObsError::Other(format!("no hello: {e}")))?;
        if hello["op"] != 0 {
            return Err(ObsError::Other("unexpected first message".into()));
        }
        let ws_version = hello["d"]["obsWebSocketVersion"]
            .as_str()
            .unwrap_or("?")
            .to_string();
        let mut identify = json!({"rpcVersion": 1, "eventSubscriptions": SUB_OUTPUTS});
        if let Some(auth) = hello["d"].get("authentication") {
            let pw = password
                .filter(|p| !p.is_empty())
                .ok_or(ObsError::AuthNeeded)?;
            let salt = auth["salt"].as_str().unwrap_or("");
            let challenge = auth["challenge"].as_str().unwrap_or("");
            let b64 = base64::engine::general_purpose::STANDARD;
            let secret = b64.encode(Sha256::digest(format!("{pw}{salt}").as_bytes()));
            let resp = b64.encode(Sha256::digest(format!("{secret}{challenge}").as_bytes()));
            identify["authentication"] = json!(resp);
        }
        send_json(&mut ws, &json!({"op": 1, "d": identify}))
            .map_err(|e| ObsError::Other(e.to_string()))?;
        match read_json(&mut ws) {
            Ok(v) if v["op"] == 2 => {}
            Ok(_) => return Err(ObsError::Other("unexpected reply to identify".into())),
            Err(e) => {
                let s = e.to_string();
                if s.contains("4009") || s.to_lowercase().contains("auth") || s.contains("closed") {
                    return Err(ObsError::AuthFailed);
                }
                return Err(ObsError::Other(s));
            }
        }
        log::info!("Connected to OBS WebSocket {ws_version}");
        Ok(Self {
            ws,
            next_id: 1,
            events: VecDeque::new(),
            ws_version,
        })
    }

    pub fn request(&mut self, kind: &str, data: Value) -> Result<Value, String> {
        let id = self.next_id.to_string();
        self.next_id += 1;
        let msg =
            json!({"op": 6, "d": {"requestType": kind, "requestId": id, "requestData": data}});
        send_json(&mut self.ws, &msg).map_err(|e| e.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if Instant::now() > deadline {
                return Err(format!("{kind}: no answer"));
            }
            let v = read_json(&mut self.ws).map_err(|e| e.to_string())?;
            match v["op"].as_i64() {
                Some(7) if v["d"]["requestId"] == json!(id) => {
                    let ok = v["d"]["requestStatus"]["result"].as_bool().unwrap_or(false);
                    if !ok {
                        let c = v["d"]["requestStatus"]["comment"]
                            .as_str()
                            .unwrap_or("")
                            .to_string();
                        return Err(format!("{kind} failed: {c}"));
                    }
                    return Ok(v["d"]["responseData"].clone());
                }
                Some(5) => {
                    if self.events.len() > 2000 {
                        self.events.pop_front();
                    }
                    self.events.push_back(v["d"].clone());
                }
                _ => {}
            }
        }
    }

    /// Changes which events OBS sends (used to get volume meters only
    /// during a test).
    pub fn subscribe_volume(&mut self, on: bool) -> Result<(), String> {
        let mask = if on {
            SUB_OUTPUTS | SUB_VOLUME
        } else {
            SUB_OUTPUTS
        };
        send_json(
            &mut self.ws,
            &json!({"op": 3, "d": {"eventSubscriptions": mask}}),
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Reads any events waiting, without blocking for long.
    pub fn pump_events(&mut self, max_wait: Duration) {
        let _ = self
            .ws
            .get_mut()
            .set_read_timeout(Some(max_wait.max(Duration::from_millis(1))));
        let start = Instant::now();
        while start.elapsed() < max_wait {
            match read_json(&mut self.ws) {
                Ok(v) => {
                    if v["op"] == 5 {
                        if self.events.len() > 4000 {
                            self.events.pop_front();
                        }
                        self.events.push_back(v["d"].clone());
                    }
                }
                Err(_) => break,
            }
        }
        let _ = self
            .ws
            .get_mut()
            .set_read_timeout(Some(Duration::from_secs(5)));
    }

    fn param(&mut self, cat: &str, name: &str) -> Option<String> {
        self.request(
            "GetProfileParameter",
            json!({"parameterCategory": cat, "parameterName": name}),
        )
        .ok()
        .and_then(|v| {
            let pv = v["parameterValue"].as_str().map(|s| s.to_string());
            pv.or_else(|| v["defaultParameterValue"].as_str().map(|s| s.to_string()))
        })
    }
}

fn send_json(ws: &mut WebSocket<TcpStream>, v: &Value) -> Result<(), tungstenite::Error> {
    ws.send(Message::Text(v.to_string().into()))
}

fn read_json(ws: &mut WebSocket<TcpStream>) -> Result<Value, String> {
    loop {
        match ws.read() {
            Ok(Message::Text(t)) => {
                return serde_json::from_str(t.as_str()).map_err(|e| e.to_string());
            }
            Ok(Message::Close(c)) => {
                return Err(format!(
                    "closed {}",
                    c.map(|f| format!("{} {}", u16::from(f.code), f.reason))
                        .unwrap_or_default()
                ));
            }
            Ok(_) => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
}

// ---------------------------------------------------------------------------
// What we learn about OBS

#[derive(Clone, Debug, Default, Serialize)]
pub struct SourceInfo {
    /// Real source name, shown only in the app. Bug reports replace it.
    #[serde(skip)]
    #[allow(dead_code)]
    pub name: String,
    pub kind: String,
    pub kind_label: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct AudioInput {
    #[serde(skip)]
    pub name: String,
    pub kind: String,
    pub role: String,
    pub muted: Option<bool>,
    pub volume_db: Option<f32>,
    /// The device it captures ("default" or a device id). Only compared, never shown or reported.
    #[serde(skip)]
    pub device: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct OutputInfo {
    pub kind: String,
    pub label: String,
    pub active: bool,
    #[serde(skip)]
    pub name: String,
    pub skipped: Option<u64>,
    pub total: Option<u64>,
    /// Made by a plugin (Source Record, a vertical canvas, extra streams...).
    pub plugin: bool,
    /// Platform named in a plugin stream's name ("Twitch", "YouTube", "Kick").
    pub platform_hint: Option<String>,
}

/// OBS's own output names; anything else comes from a plugin.
const OBS_OUTPUTS: [&str; 10] = [
    "simple_stream",
    "adv_stream",
    "simple_file_output",
    "adv_file_output",
    "adv_ffmpeg_output",
    "replay buffer",
    "replay_buffer",
    "virtualcam_output",
    "multitrack_video",
    "simple_replay_buffer",
];

/// A plain label for an output. Plugin outputs are told apart by their kind
/// and a few words in their name; the name itself is never shown or saved.
pub fn classify_output(kind: &str, name: &str) -> (String, bool, Option<String>) {
    let n = name.to_lowercase();
    let own =
        OBS_OUTPUTS.iter().any(|o| n == *o) || n.starts_with("simple_") || n.starts_with("adv_");
    let base = output_label(kind);
    if own || kind == "virtualcam_output" || kind == "replay_buffer" {
        return (base.into(), false, None);
    }
    let platform = [
        "twitch", "youtube", "kick", "facebook", "tiktok", "x.com", "trovo",
    ]
    .iter()
    .find(|p| n.contains(*p))
    .map(|p| match *p {
        "youtube" => "YouTube".to_string(),
        "tiktok" => "TikTok".to_string(),
        "x.com" => "X".to_string(),
        x => {
            let mut c = x.chars();
            c.next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or_default()
        }
    });
    let label =
        if n.contains("source record") || n.contains("source_record") || n.contains("sourcerecord")
        {
            "Source Record".to_string()
        } else if n.contains("vertical") || n.contains("aitum") || n.contains("canvas") {
            format!("Vertical canvas {}", base.to_lowercase())
        } else if base == "Stream" {
            match &platform {
                Some(p) => format!("Extra stream ({p})"),
                None => "Extra stream".into(),
            }
        } else if base == "Recording" {
            "Extra recording".into()
        } else {
            "Plugin output".into()
        };
    (label, true, platform)
}

pub fn output_label(kind: &str) -> &'static str {
    match kind {
        "rtmp_output" | "ffmpeg_output" | "whip_output" | "ftl_output" => "Stream",
        "ffmpeg_muxer" | "mp4_output" | "mkv_output" | "mov_output" | "flv_output"
        | "ffmpeg_hls_muxer" => "Recording",
        "replay_buffer" => "Replay buffer",
        "virtualcam_output" => "Virtual camera",
        _ => "Other output",
    }
}

/// A Source Record filter: one extra recording of one source or scene.
#[derive(Clone, Debug, Default, Serialize)]
pub struct SourceRecordInfo {
    /// "Always", "While streaming", "While recording"... or "Off".
    pub mode: String,
    pub encoder: String,
    pub bitrate_kbps: Option<u32>,
    pub enabled: bool,
}

/// Reads Source Record filters (settings only) from every source and scene.
pub fn read_source_record(c: &mut Client) -> Vec<SourceRecordInfo> {
    let mut names: Vec<String> = Vec::new();
    if let Ok(r) = c.request("GetInputList", json!({})) {
        names.extend(
            r["inputs"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|i| i["inputName"].as_str().map(|s| s.to_string())),
        );
    }
    if let Ok(r) = c.request("GetSceneList", json!({})) {
        names.extend(
            r["scenes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|i| i["sceneName"].as_str().map(|s| s.to_string())),
        );
    }
    let mut out = Vec::new();
    for n in names.iter().take(200) {
        let Ok(r) = c.request("GetSourceFilterList", json!({"sourceName": n})) else {
            continue;
        };
        for f in r["filters"].as_array().into_iter().flatten() {
            if f["filterKind"].as_str() != Some("source_record_filter") {
                continue;
            }
            let st = &f["filterSettings"];
            let mode = match st["record_mode"].as_i64().unwrap_or(0) {
                1 => "Always",
                2 => "While streaming",
                3 => "While recording",
                4 => "While streaming or recording",
                5 => "With the virtual camera",
                _ => "Off",
            };
            out.push(SourceRecordInfo {
                mode: mode.into(),
                encoder: encoder_kind(st["encoder"].as_str().unwrap_or("")).to_string(),
                bitrate_kbps: st["bitrate"].as_u64().map(|b| b as u32),
                enabled: f["filterEnabled"].as_bool().unwrap_or(true),
            });
        }
    }
    out
}

/// Third-party OBS plugins installed, by name only (folder or file names).
pub fn installed_plugins() -> Vec<String> {
    const KNOWN: [(&str, &str); 16] = [
        ("source-record", "Source Record"),
        ("vertical-canvas", "Aitum Vertical"),
        ("aitum-vertical", "Aitum Vertical"),
        ("aitum-multistream", "Aitum Multistream"),
        ("obs-multi-rtmp", "Multiple RTMP outputs"),
        ("multi-rtmp", "Multiple RTMP outputs"),
        ("backgroundremoval", "Background Removal"),
        ("linux-vkcapture", "Game capture (vkcapture)"),
        ("streamfx", "StreamFX"),
        ("move-transition", "Move"),
        ("shaderfilter", "Shader Filter"),
        ("advanced-scene-switcher", "Advanced Scene Switcher"),
        ("composite-blur", "Composite Blur"),
        ("distroav", "DistroAV (NDI)"),
        ("obs-ndi", "NDI"),
        ("downstream-keyer", "Downstream Keyer"),
    ];
    let mut dirs: Vec<std::path::PathBuf> = Vec::new();
    #[cfg(not(windows))]
    {
        if let Some(h) = crate::util::home_dir() {
            dirs.push(h.join(".config/obs-studio/plugins"));
            dirs.push(h.join(".var/app/com.obsproject.Studio/config/obs-studio/plugins"));
        }
        for d in [
            "/usr/lib/obs-plugins",
            "/usr/lib64/obs-plugins",
            "/usr/lib/x86_64-linux-gnu/obs-plugins",
            "/usr/local/lib/obs-plugins",
        ] {
            dirs.push(d.into());
        }
    }
    #[cfg(windows)]
    {
        if let Ok(p) = std::env::var("ProgramData") {
            dirs.push(std::path::PathBuf::from(p).join("obs-studio/plugins"));
        }
        if let Ok(p) = std::env::var("ProgramFiles") {
            dirs.push(std::path::PathBuf::from(p).join("obs-studio/obs-plugins/64bit"));
        }
        if let Ok(p) = std::env::var("APPDATA") {
            dirs.push(std::path::PathBuf::from(p).join("obs-studio/plugins"));
        }
    }
    let mut out: Vec<String> = Vec::new();
    for d in dirs {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_lowercase();
            if let Some((_, label)) = KNOWN.iter().find(|(k, _)| n.contains(k))
                && !out.iter().any(|x| x == label)
            {
                out.push(label.to_string());
            }
        }
    }
    out
}

/// Every output OBS has, with skipped frames for the active ones.
pub fn read_outputs(c: &mut Client) -> Vec<OutputInfo> {
    let mut v = Vec::new();
    if let Ok(r) = c.request("GetOutputList", json!({}))
        && let Some(list) = r["outputs"].as_array()
    {
        for o in list {
            let kind = o["outputKind"].as_str().unwrap_or("").to_string();
            let name = o["outputName"].as_str().unwrap_or("").to_string();
            let active = o["outputActive"].as_bool().unwrap_or(false);
            let (mut skipped, mut total) = (None, None);
            if active && let Ok(st) = c.request("GetOutputStatus", json!({"outputName": name})) {
                skipped = st["outputSkippedFrames"].as_u64();
                total = st["outputTotalFrames"].as_u64();
            }
            let (label, plugin, platform_hint) = classify_output(&kind, &name);
            v.push(OutputInfo {
                label,
                kind,
                active,
                name,
                skipped,
                total,
                plugin,
                platform_hint,
            });
        }
    }
    v
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ObsSnapshot {
    pub source: String,
    pub connected: bool,
    pub error: Option<String>,
    pub obs_version: String,
    pub ws_version: String,
    pub output_mode: String,
    pub encoder_raw: String,
    pub encoder_kind: String,
    pub codec: String,
    pub bitrate_kbps: Option<u32>,
    pub rate_control: String,
    pub preset: String,
    /// Seconds between keyframes. 0 means "auto". None if unknown.
    pub keyint_sec: Option<u32>,
    /// NVENC tuning ("hq", "ll", "ull"), multipass ("disabled", "qres",
    /// "fullres"), B-frames, profile, look-ahead and psycho-visual tuning.
    pub enc_tune: String,
    pub enc_multipass: String,
    pub enc_bframes: Option<u32>,
    pub enc_profile: String,
    pub enc_lookahead: Option<bool>,
    pub enc_psycho_aq: Option<bool>,
    /// Stream audio bitrate (kbps) and OBS's sample rate (Hz).
    pub audio_kbps: Option<u32>,
    pub sample_rate: Option<u32>,
    /// Downscale filter: bilinear, area, bicubic or lanczos.
    pub scale_type: String,
    /// "Ignore streaming service setting recommendations" in OBS.
    pub ignore_recommended: Option<bool>,
    // Recording, when it uses its own encoder (empty when it shares the stream's)
    pub rec_encoder_raw: String,
    pub rec_encoder_kind: String,
    pub rec_preset: String,
    pub rec_rate_control: String,
    pub rec_bitrate_kbps: Option<u32>,
    pub rec_cq: Option<u32>,
    /// Simple mode recording quality (Small, HQ, Lossless).
    pub rec_quality: String,
    /// Recording resolution when it's rescaled separately.
    pub rec_res: Option<(u32, u32)>,
    /// mkv, mp4, hybrid_mp4, mov, flv ...
    pub rec_format: String,
    /// Folder recordings go to. Kept out of reports (it contains the user's path).
    #[serde(skip)]
    pub rec_dir: String,
    pub replay_buffer: Option<bool>,
    pub replay_active: bool,
    /// Twitch/YouTube Enhanced Broadcasting (multitrack video).
    pub multitrack: Option<bool>,
    /// Outputs OBS has, by kind only (names are left out).
    pub outputs: Vec<OutputInfo>,
    /// Source Record filters (extra per-source recordings).
    pub source_records: Vec<SourceRecordInfo>,
    /// Third-party plugins installed (names only).
    pub plugins: Vec<String>,
    /// Capture cards and network (NDI) sources: how they capture. Two-PC setups.
    pub captures: Vec<crate::twopc::CaptureInfo>,
    /// OBS's colour range ("Partial" or "Full") and colour space ("709", "2100PQ"...).
    pub color_range: String,
    pub color_space: String,
    pub base_res: Option<(u32, u32)>,
    pub output_res: Option<(u32, u32)>,
    pub fps: Option<f32>,
    pub recording_uses_stream_settings: Option<bool>,
    pub recording_note: String,
    /// OBS › Settings › Advanced › Recording › "Automatically record when streaming".
    #[serde(default)]
    pub auto_record: Option<bool>,
    pub recording_active: bool,
    pub streaming_active: bool,
    #[serde(skip)]
    pub current_scene: String,
    pub sources: Vec<SourceInfo>,
    pub audio: Vec<AudioInput>,
    pub cpu_usage: Option<f32>,
    pub render_skipped: Option<u64>,
    pub output_skipped: Option<u64>,
    pub disk_free_mb: Option<f64>,
    pub files_read: Vec<String>,
}

impl ObsSnapshot {
    pub fn webcam(&self) -> Option<&SourceInfo> {
        self.sources.iter().find(|s| s.kind_label == "Webcam")
    }
    pub fn uses_display_capture(&self) -> bool {
        self.sources
            .iter()
            .any(|s| s.kind_label == "Display capture" && s.enabled)
    }
    /// A PipeWire portal capture: could be a window or a whole screen.
    pub fn uses_portal_capture(&self) -> bool {
        self.sources
            .iter()
            .any(|s| s.kind_label == "Screen or window (PipeWire)" && s.enabled)
    }
    pub fn uses_game_capture(&self) -> bool {
        self.sources.iter().any(|s| {
            (s.kind_label == "Game capture" || s.kind_label == "Window capture") && s.enabled
        })
    }
    /// A short fingerprint of the stream setup, to notice changes.
    pub fn signature(&self) -> String {
        format!(
            "{}|{}|{}|{:?}|{}|{}|{:?}|{:?}|{:?}|{}",
            self.encoder_raw,
            self.rate_control,
            self.preset,
            self.keyint_sec,
            self.scale_type,
            self.output_mode,
            self.bitrate_kbps,
            self.output_res,
            self.fps.map(|f| f.round() as u32),
            self.sources
                .iter()
                .map(|s| format!("{}:{}", s.kind, s.enabled))
                .collect::<Vec<_>>()
                .join(",")
        )
    }
    pub fn mic_volume_db(&self) -> Option<f32> {
        self.audio
            .iter()
            .find(|a| a.role == "mic")
            .and_then(|a| a.volume_db)
    }
    pub fn desktop_volume_db(&self) -> Option<f32> {
        self.audio
            .iter()
            .find(|a| a.role == "desktop" || a.role == "app")
            .and_then(|a| a.volume_db)
    }
    pub fn mics(&self) -> Vec<&AudioInput> {
        self.audio.iter().filter(|a| a.role == "mic").collect()
    }
    /// Two or more unmuted Desktop Audio sources capturing the same device: the game is heard twice.
    pub fn doubled_desktop_audio(&self) -> usize {
        let on: Vec<&AudioInput> = self
            .audio
            .iter()
            .filter(|a| a.role == "desktop" && a.muted != Some(true))
            .collect();
        on.iter()
            .map(|a| on.iter().filter(|b| b.device == a.device).count())
            .max()
            .unwrap_or(0)
    }

    pub fn desktop_audio(&self) -> Vec<&AudioInput> {
        self.audio
            .iter()
            .filter(|a| a.role == "desktop" || a.role == "app")
            .collect()
    }
}

pub fn encoder_kind(raw: &str) -> &'static str {
    let l = raw.to_lowercase();
    if l.is_empty() {
        "Unknown"
    } else if l.contains("nvenc") {
        "NVIDIA NVENC"
    } else if l.contains("x264") {
        "x264 (CPU)"
    } else if l.contains("amf") || l.contains("amd") {
        "AMD AMF"
    } else if l.contains("qsv") || l.contains("quicksync") {
        "Intel Quick Sync"
    } else if l.contains("vaapi") {
        "VA-API"
    } else if l.contains("svt") || l.contains("aom") {
        "AV1 (CPU)"
    } else {
        "Other"
    }
}

fn codec_of(raw: &str) -> &'static str {
    let l = raw.to_lowercase();
    if l.contains("av1") {
        "AV1"
    } else if l.contains("hevc") || l.contains("h265") {
        "HEVC"
    } else {
        "H.264"
    }
}

fn source_label(kind: &str) -> &'static str {
    let k = kind.to_lowercase();
    if k.contains("game_capture") || k.contains("vkcapture") || k.contains("gamecapture") {
        // Windows Game Capture, and obs-vkcapture (Linux Vulkan/OpenGL game capture).
        "Game capture"
    } else if k.contains("pipewire-window")
        || k.contains("window_capture")
        || k.contains("xcomposite")
    {
        "Window capture"
    } else if k.contains("pipewire") {
        // KDE/GNOME portal capture: the player picks a screen OR a window, and
        // OBS doesn't say which. Never assume a whole screen.
        "Screen or window (PipeWire)"
    } else if k.contains("monitor_capture")
        || k.contains("xshm")
        || k.contains("screen")
        || k.contains("display")
    {
        "Display capture"
    } else if k.contains("dshow")
        || k.contains("v4l2")
        || k.contains("av_capture")
        || k.contains("camera")
    {
        "Webcam"
    } else if k.contains("browser") {
        "Browser source"
    } else if k.contains("image") {
        "Image"
    } else if k.contains("text") {
        "Text"
    } else if k.contains("ffmpeg") || k.contains("vlc") || k.contains("media") {
        "Media"
    } else if k.contains("input_capture") {
        "Microphone"
    } else if k.contains("output_capture") {
        "Desktop audio"
    } else {
        "Other"
    }
}

fn audio_role(kind: &str) -> Option<&'static str> {
    let k = kind.to_lowercase();
    if k.contains("process_output_capture") || k.contains("application") {
        Some("app")
    } else if k.contains("input_capture") {
        Some("mic")
    } else if k.contains("output_capture") {
        Some("desktop")
    } else {
        None
    }
}

/// Reads everything we need from a connected OBS.
pub fn snapshot_live(c: &mut Client) -> ObsSnapshot {
    crate::logger::activity("reading OBS settings");
    let mut s = ObsSnapshot {
        source: "live connection".into(),
        connected: true,
        ws_version: c.ws_version.clone(),
        ..Default::default()
    };
    if let Ok(v) = c.request("GetVersion", json!({})) {
        s.obs_version = v["obsVersion"].as_str().unwrap_or("").to_string();
    }
    if let Ok(v) = c.request("GetVideoSettings", json!({})) {
        let g = |k: &str| v[k].as_u64().map(|x| x as u32);
        if let (Some(a), Some(b)) = (g("baseWidth"), g("baseHeight")) {
            s.base_res = Some((a, b));
        }
        if let (Some(a), Some(b)) = (g("outputWidth"), g("outputHeight")) {
            s.output_res = Some((a, b));
        }
        if let (Some(n), Some(d)) = (v["fpsNumerator"].as_f64(), v["fpsDenominator"].as_f64())
            && d > 0.0
        {
            s.fps = Some((n / d) as f32);
        }
    }
    s.output_mode = c.param("Output", "Mode").unwrap_or_else(|| "Simple".into());
    let advanced = s.output_mode.eq_ignore_ascii_case("advanced");
    if advanced {
        s.encoder_raw = c.param("AdvOut", "Encoder").unwrap_or_default();
        let rec_enc = c.param("AdvOut", "RecEncoder").unwrap_or_default();
        let rec_type = c.param("AdvOut", "RecType").unwrap_or_default();
        if rec_type.eq_ignore_ascii_case("ffmpeg") {
            s.recording_uses_stream_settings = Some(false);
            s.recording_note =
                "Recording uses Custom Output (FFmpeg), not the stream encoder.".into();
        } else {
            let same = rec_enc.is_empty() || rec_enc.eq_ignore_ascii_case("none");
            s.recording_uses_stream_settings = Some(same);
            if !same {
                s.recording_note = format!(
                    "Recording uses its own encoder ({}).",
                    encoder_kind(&rec_enc)
                );
                s.rec_encoder_raw = rec_enc.clone();
                if c.param("AdvOut", "RecRescale")
                    .map(|x| x.eq_ignore_ascii_case("true"))
                    .unwrap_or(false)
                {
                    s.rec_res = c
                        .param("AdvOut", "RecRescaleRes")
                        .and_then(|r| parse_res(&r));
                }
            }
        }
        s.rec_format = c
            .param("AdvOut", "RecFormat2")
            .or_else(|| c.param("AdvOut", "RecFormat"))
            .unwrap_or_default();
        s.rec_dir = c.param("AdvOut", "RecFilePath").unwrap_or_default();
        s.replay_buffer = c
            .param("AdvOut", "RecRB")
            .map(|x| x.eq_ignore_ascii_case("true"));
    } else {
        s.encoder_raw = c.param("SimpleOutput", "StreamEncoder").unwrap_or_default();
        s.bitrate_kbps = c
            .param("SimpleOutput", "VBitrate")
            .and_then(|b| b.trim().parse().ok());
        let enc = s.encoder_raw.clone();
        simple_quality(&mut s, &enc, |k| c.param("SimpleOutput", k));
        let rq = c.param("SimpleOutput", "RecQuality").unwrap_or_default();
        let same = rq.eq_ignore_ascii_case("stream");
        s.recording_uses_stream_settings = Some(same);
        if !same {
            s.recording_note =
                format!("Recording quality is set to \"{rq}\" instead of \"Same as stream\".");
            s.rec_encoder_raw = c.param("SimpleOutput", "RecEncoder").unwrap_or_default();
            s.rec_quality = rq.clone();
        }
        s.rec_format = c
            .param("SimpleOutput", "RecFormat2")
            .or_else(|| c.param("SimpleOutput", "RecFormat"))
            .unwrap_or_default();
        s.rec_dir = c.param("SimpleOutput", "FilePath").unwrap_or_default();
        s.replay_buffer = c
            .param("SimpleOutput", "RecRB")
            .map(|x| x.eq_ignore_ascii_case("true"));
    }
    // Profile folder files for the advanced encoder settings.
    if let Ok(v) = c.request("GetProfileList", json!({})) {
        let name = v["currentProfileName"].as_str().unwrap_or("").to_string();
        if let Some(dir) = profile_dir_by_name(&name) {
            read_encoder_json(&dir, &mut s);
            if advanced && !s.rec_encoder_raw.is_empty() {
                read_rec_json(&dir, &mut s);
            }
        }
    }
    s.scale_type = c
        .param("Video", "ScaleType")
        .filter(|x| !x.is_empty())
        .unwrap_or_else(|| "bicubic".into());
    s.sample_rate = Some(
        c.param("Audio", "SampleRate")
            .and_then(|x| x.parse().ok())
            .unwrap_or(48000),
    );
    s.audio_kbps = Some(if advanced {
        let track = c
            .param("AdvOut", "TrackIndex")
            .and_then(|x| x.parse::<u32>().ok())
            .unwrap_or(1)
            .clamp(1, 6);
        c.param("AdvOut", &format!("Track{track}Bitrate"))
            .and_then(|x| x.parse().ok())
            .unwrap_or(160)
    } else {
        c.param("SimpleOutput", "ABitrate")
            .and_then(|x| x.parse().ok())
            .unwrap_or(160)
    });
    s.ignore_recommended = c
        .param("Stream1", "IgnoreRecommended")
        .map(|x| x.eq_ignore_ascii_case("true"));
    s.multitrack = c
        .param("Stream1", "EnableMultitrackVideo")
        .map(|x| x.eq_ignore_ascii_case("true"));
    if let Ok(v) = c.request("GetReplayBufferStatus", json!({})) {
        s.replay_active = v["outputActive"].as_bool().unwrap_or(false);
    }
    s.outputs = read_outputs(c);
    s.plugins = installed_plugins();
    if s.plugins.iter().any(|p| p == "Source Record") {
        s.source_records = read_source_record(c);
    }
    if advanced {
        advanced_defaults(&mut s);
    }
    s.encoder_kind = encoder_kind(&s.encoder_raw).to_string();
    s.codec = codec_of(&s.encoder_raw).to_string();
    nvenc_defaults(&mut s);
    finish_rec(&mut s);

    if let Ok(v) = c.request("GetRecordStatus", json!({})) {
        s.recording_active = v["outputActive"].as_bool().unwrap_or(false);
    }
    if let Ok(v) = c.request("GetStreamStatus", json!({})) {
        s.streaming_active = v["outputActive"].as_bool().unwrap_or(false);
    }
    if let Ok(v) = c.request("GetStats", json!({})) {
        s.cpu_usage = v["cpuUsage"].as_f64().map(|x| x as f32);
        s.render_skipped = v["renderSkippedFrames"].as_u64();
        s.output_skipped = v["outputSkippedFrames"].as_u64();
        s.disk_free_mb = v["availableDiskSpace"].as_f64();
    }
    if let Ok(v) = c.request("GetCurrentProgramScene", json!({})) {
        s.current_scene = v["currentProgramSceneName"]
            .as_str()
            .or_else(|| v["sceneName"].as_str())
            .unwrap_or("")
            .to_string();
    }
    if !s.current_scene.is_empty()
        && let Ok(v) = c.request("GetSceneItemList", json!({"sceneName": s.current_scene}))
        && let Some(items) = v["sceneItems"].as_array()
    {
        for it in items {
            let kind = it["inputKind"]
                .as_str()
                .unwrap_or(if it["isGroup"].as_bool() == Some(true) {
                    "group"
                } else {
                    "scene"
                })
                .to_string();
            s.sources.push(SourceInfo {
                name: it["sourceName"].as_str().unwrap_or("").to_string(),
                kind_label: source_label(&kind).to_string(),
                kind,
                enabled: it["sceneItemEnabled"].as_bool().unwrap_or(true),
            });
        }
    }
    s.color_range = c.param("Video", "ColorRange").unwrap_or_default();
    s.color_space = c.param("Video", "ColorSpace").unwrap_or_default();
    if let Ok(v) = c.request("GetInputList", json!({}))
        && let Some(inputs) = v["inputs"].as_array()
    {
        for i in inputs {
            let kind = i["inputKind"].as_str().unwrap_or("").to_string();
            if crate::twopc::is_capture_kind(&kind) {
                let name = i["inputName"].as_str().unwrap_or("").to_string();
                if let Ok(st) = c.request("GetInputSettings", json!({"inputName": name})) {
                    let audio = c
                        .request("GetInputAudioSyncOffset", json!({"inputName": name}))
                        .ok()
                        .and_then(|m| m["inputAudioSyncOffset"].as_i64());
                    let in_scene = s.sources.iter().any(|src| src.name == name && src.enabled);
                    s.captures.push(crate::twopc::CaptureInfo::from_settings(
                        &kind,
                        &st["inputSettings"],
                        audio,
                        in_scene,
                    ));
                }
            }
            if let Some(role) = audio_role(&kind) {
                let name = i["inputName"].as_str().unwrap_or("").to_string();
                let muted = c
                    .request("GetInputMute", json!({"inputName": name}))
                    .ok()
                    .and_then(|m| m["inputMuted"].as_bool());
                let volume_db = c
                    .request("GetInputVolume", json!({"inputName": name}))
                    .ok()
                    .and_then(|m| m["inputVolumeDb"].as_f64())
                    .map(|v| v as f32);
                let device = if role == "desktop" {
                    c.request("GetInputSettings", json!({"inputName": name}))
                        .ok()
                        .and_then(|v| {
                            v["inputSettings"]["device_id"]
                                .as_str()
                                .map(|d| d.to_string())
                        })
                        .unwrap_or_else(|| "default".into())
                } else {
                    String::new()
                };
                s.audio.push(AudioInput {
                    name,
                    kind,
                    role: role.into(),
                    muted,
                    volume_db,
                    device,
                });
            }
        }
    }
    crate::logger::info_changed(
        "obs",
        format!(
            "OBS {}: {} mode, encoder {} ({}), bitrate {:?}, output {:?} @ {:?} fps, recording matches stream: {:?}",
            s.obs_version,
            s.output_mode,
            s.encoder_kind,
            s.encoder_raw,
            s.bitrate_kbps,
            s.output_res,
            s.fps,
            s.recording_uses_stream_settings
        ),
    );
    s.auto_record = auto_record_setting();
    s
}

// ---------------------------------------------------------------------------
// Saved OBS files (used when OBS is closed)

pub fn obs_config_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    #[cfg(windows)]
    if let Ok(a) = std::env::var("APPDATA") {
        v.push(PathBuf::from(a).join("obs-studio"));
    }
    #[cfg(not(windows))]
    if let Some(h) = crate::util::home_dir() {
        v.push(h.join(".config/obs-studio"));
        v.push(h.join(".var/app/com.obsproject.Studio/config/obs-studio"));
    }
    v.into_iter()
        .filter(|p| p.join("basic/profiles").exists())
        .collect()
}

/// "Automatically record when streaming" from OBS's own settings file
/// (user.ini, or global.ini before OBS 31). Read-only; OBS's default is off.
pub fn auto_record_setting() -> Option<bool> {
    for base in obs_config_dirs() {
        for f in ["user.ini", "global.ini"] {
            if let Ok(t) = std::fs::read_to_string(base.join(f)) {
                return Some(parse_auto_record(&t));
            }
        }
    }
    None
}

pub fn parse_auto_record(ini: &str) -> bool {
    let mut sect = "";
    for line in ini.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            sect = l;
        } else if sect == "[BasicWindow]"
            && let Some(v) = l.strip_prefix("RecordWhenStreaming=")
        {
            return v.trim().eq_ignore_ascii_case("true");
        }
    }
    false
}

fn profile_dir_by_name(name: &str) -> Option<PathBuf> {
    for base in obs_config_dirs() {
        let profiles = base.join("basic/profiles");
        if let Ok(rd) = std::fs::read_dir(&profiles) {
            for e in rd.flatten() {
                let ini = e.path().join("basic.ini");
                if let Ok(t) = std::fs::read_to_string(&ini)
                    && t.lines().any(|l| l.trim() == format!("Name={name}"))
                {
                    return Some(e.path());
                }
            }
        }
    }
    None
}

fn current_profile_dir() -> Option<PathBuf> {
    for base in obs_config_dirs() {
        for f in ["user.ini", "global.ini"] {
            if let Ok(t) = std::fs::read_to_string(base.join(f)) {
                let mut in_basic = false;
                for line in t.lines() {
                    let l = line.trim();
                    if l.starts_with('[') {
                        in_basic = l == "[Basic]";
                    } else if in_basic && let Some(d) = l.strip_prefix("ProfileDir=") {
                        let p = base.join("basic/profiles").join(d);
                        if p.exists() {
                            return Some(p);
                        }
                    }
                }
            }
        }
        if let Ok(rd) = std::fs::read_dir(base.join("basic/profiles"))
            && let Some(e) = rd.flatten().find(|e| e.path().join("basic.ini").exists())
        {
            return Some(e.path());
        }
    }
    None
}

fn read_encoder_json(dir: &std::path::Path, s: &mut ObsSnapshot) {
    let p = dir.join("streamEncoder.json");
    if let Ok(t) = std::fs::read_to_string(&p)
        && let Ok(v) = serde_json::from_str::<Value>(&t)
    {
        if let Some(b) = v["bitrate"].as_u64() {
            s.bitrate_kbps = Some(b as u32);
        }
        s.rate_control = v["rate_control"].as_str().unwrap_or("").to_string();
        s.preset = v["preset2"]
            .as_str()
            .or_else(|| v["preset"].as_str())
            .map(|x| x.to_string())
            .or_else(|| v["target_usage"].as_str().map(|x| x.to_string()))
            .unwrap_or_default();
        s.keyint_sec = v["keyint_sec"].as_u64().map(|k| k as u32);
        s.enc_tune = v["tune"].as_str().unwrap_or("").to_string();
        s.enc_multipass = v["multipass"].as_str().unwrap_or("").to_string();
        s.enc_bframes = v["bf"].as_u64().map(|b| b as u32);
        s.enc_profile = v["profile"].as_str().unwrap_or("").to_string();
        s.enc_lookahead = v["lookahead"].as_bool();
        s.enc_psycho_aq = v["psycho_aq"].as_bool();
        s.files_read.push("streamEncoder.json".into());
    }
}

/// Reads the saved OBS profile. Never opens service.json.
pub fn snapshot_saved() -> Option<ObsSnapshot> {
    let dir = current_profile_dir()?;
    let ini = std::fs::read_to_string(dir.join("basic.ini")).ok()?;
    let mut s = ObsSnapshot {
        source: "saved OBS files (OBS closed)".into(),
        ..Default::default()
    };
    s.files_read.push("basic.ini".into());
    let mut sect = String::new();
    let mut get = std::collections::HashMap::new();
    for line in ini.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            sect = l.trim_matches(['[', ']']).to_string();
        } else if let Some((k, v)) = l.split_once('=') {
            get.insert(format!("{sect}/{k}"), v.to_string());
        }
    }
    let g = |k: &str| get.get(k).cloned().unwrap_or_default();
    s.output_mode = if g("Output/Mode").is_empty() {
        "Simple".into()
    } else {
        g("Output/Mode")
    };
    if s.output_mode.eq_ignore_ascii_case("advanced") {
        s.encoder_raw = g("AdvOut/Encoder");
        let rec = g("AdvOut/RecEncoder");
        let same = rec.is_empty() || rec.eq_ignore_ascii_case("none");
        s.recording_uses_stream_settings =
            Some(same && !g("AdvOut/RecType").eq_ignore_ascii_case("ffmpeg"));
        read_encoder_json(&dir, &mut s);
        advanced_defaults(&mut s);
        if !same {
            s.rec_encoder_raw = rec.clone();
            if g("AdvOut/RecRescale").eq_ignore_ascii_case("true") {
                s.rec_res = parse_res(&g("AdvOut/RecRescaleRes"));
            }
            read_rec_json(&dir, &mut s);
        }
        s.rec_format = if g("AdvOut/RecFormat2").is_empty() {
            g("AdvOut/RecFormat")
        } else {
            g("AdvOut/RecFormat2")
        };
        s.rec_dir = g("AdvOut/RecFilePath");
        s.replay_buffer = Some(g("AdvOut/RecRB").eq_ignore_ascii_case("true"));
    } else {
        s.encoder_raw = g("SimpleOutput/StreamEncoder");
        s.bitrate_kbps = g("SimpleOutput/VBitrate").parse().ok();
        let enc = s.encoder_raw.clone();
        simple_quality(&mut s, &enc, |k| {
            Some(g(&format!("SimpleOutput/{k}"))).filter(|x| !x.is_empty())
        });
        let rq = g("SimpleOutput/RecQuality");
        if !rq.is_empty() && !rq.eq_ignore_ascii_case("stream") {
            s.rec_encoder_raw = g("SimpleOutput/RecEncoder");
            s.rec_quality = rq;
        }
        s.rec_format = if g("SimpleOutput/RecFormat2").is_empty() {
            g("SimpleOutput/RecFormat")
        } else {
            g("SimpleOutput/RecFormat2")
        };
        s.rec_dir = g("SimpleOutput/FilePath");
        s.replay_buffer = Some(g("SimpleOutput/RecRB").eq_ignore_ascii_case("true"));
        s.recording_uses_stream_settings =
            Some(g("SimpleOutput/RecQuality").eq_ignore_ascii_case("stream"));
    }
    if s.recording_uses_stream_settings == Some(false) {
        s.recording_note = "Recording doesn't use the stream's encoder settings.".into();
    }
    s.scale_type = if g("Video/ScaleType").is_empty() {
        "bicubic".into()
    } else {
        g("Video/ScaleType")
    };
    s.sample_rate = Some(g("Audio/SampleRate").parse().unwrap_or(48000));
    s.audio_kbps = Some(if s.output_mode.eq_ignore_ascii_case("advanced") {
        let track = g("AdvOut/TrackIndex")
            .parse::<u32>()
            .unwrap_or(1)
            .clamp(1, 6);
        g(&format!("AdvOut/Track{track}Bitrate"))
            .parse()
            .unwrap_or(160)
    } else {
        g("SimpleOutput/ABitrate").parse().unwrap_or(160)
    });
    if !g("Stream1/EnableMultitrackVideo").is_empty() {
        s.multitrack = Some(g("Stream1/EnableMultitrackVideo").eq_ignore_ascii_case("true"));
    }
    if !g("Stream1/IgnoreRecommended").is_empty() {
        s.ignore_recommended = Some(g("Stream1/IgnoreRecommended").eq_ignore_ascii_case("true"));
    }
    let num = |k: &str| g(k).parse::<u32>().ok();
    if let (Some(a), Some(b)) = (num("Video/BaseCX"), num("Video/BaseCY")) {
        s.base_res = Some((a, b));
    }
    if let (Some(a), Some(b)) = (num("Video/OutputCX"), num("Video/OutputCY")) {
        s.output_res = Some((a, b));
    }
    let fps_type = g("Video/FPSType");
    s.fps = match fps_type.as_str() {
        "1" => num("Video/FPSInt").map(|x| x as f32),
        "2" => match (num("Video/FPSNum"), num("Video/FPSDen")) {
            (Some(n), Some(d)) if d > 0 => Some(n as f32 / d as f32),
            _ => None,
        },
        _ => g("Video/FPSCommon")
            .split_whitespace()
            .next()
            .and_then(|x| x.parse().ok()),
    };
    s.encoder_kind = encoder_kind(&s.encoder_raw).to_string();
    s.codec = codec_of(&s.encoder_raw).to_string();
    nvenc_defaults(&mut s);
    finish_rec(&mut s);
    crate::logger::info_changed(
        "obs-profile",
        format!(
            "Read saved OBS profile: {} mode, encoder {}",
            s.output_mode, s.encoder_kind
        ),
    );
    s.auto_record = auto_record_setting();
    Some(s)
}

fn parse_res(r: &str) -> Option<(u32, u32)> {
    let (a, b) = r.trim().split_once('x')?;
    Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
}

fn read_rec_json(dir: &std::path::Path, s: &mut ObsSnapshot) {
    if let Ok(t) = std::fs::read_to_string(dir.join("recordEncoder.json"))
        && let Ok(v) = serde_json::from_str::<Value>(&t)
    {
        s.rec_preset = v["preset2"]
            .as_str()
            .or_else(|| v["preset"].as_str())
            .or_else(|| v["target_usage"].as_str())
            .unwrap_or("")
            .to_string();
        s.rec_rate_control = v["rate_control"].as_str().unwrap_or("").to_string();
        s.rec_bitrate_kbps = v["bitrate"].as_u64().map(|b| b as u32);
        s.rec_cq = v["cqp"]
            .as_u64()
            .or_else(|| v["crf"].as_u64())
            .or_else(|| v["cq"].as_u64())
            .map(|x| x as u32);
        s.files_read.push("recordEncoder.json".into());
    }
}

/// Fills in recording defaults once the stream settings are known.
fn finish_rec(s: &mut ObsSnapshot) {
    if s.rec_encoder_raw.is_empty() {
        return;
    }
    s.rec_encoder_kind = encoder_kind(&s.rec_encoder_raw).to_string();
    let l = s.rec_encoder_raw.to_lowercase();
    if s.rec_preset.is_empty() {
        s.rec_preset = if !s.rec_quality.is_empty() {
            s.preset.clone()
        } else if l.contains("x264") {
            "veryfast".into()
        } else if l.contains("nvenc") {
            "p5".into()
        } else {
            String::new()
        };
    }
    if s.rec_rate_control.is_empty() {
        s.rec_rate_control = if s.rec_quality.is_empty() {
            "CBR".into()
        } else {
            "CQP".into()
        };
    }
}

/// The OBS settings that shape the stream and recording, saved with each
/// test so two tests can be compared ("Preset P7 → P6").
#[derive(Clone, Debug, Default, PartialEq, Serialize, serde::Deserialize)]
#[serde(default)]
pub struct EncSettings {
    pub encoder: String,
    pub preset: String,
    pub multipass: String,
    pub tune: String,
    pub rate_control: String,
    pub bitrate_kbps: Option<u32>,
    pub keyint_sec: Option<u32>,
    pub bframes: Option<u32>,
    pub profile: String,
    pub output: String,
    pub fps: Option<u32>,
    pub scale: String,
    pub audio_kbps: Option<u32>,
    pub sample_rate: Option<u32>,
    pub rec_encoder: String,
    pub rec_preset: String,
    pub rec_rate_control: String,
    pub rec_cq: Option<u32>,
}

impl ObsSnapshot {
    pub fn enc_settings(&self) -> EncSettings {
        EncSettings {
            encoder: self.encoder_kind.clone(),
            preset: self.preset.clone(),
            multipass: self.enc_multipass.clone(),
            tune: self.enc_tune.clone(),
            rate_control: self.rate_control.clone(),
            bitrate_kbps: self.bitrate_kbps,
            keyint_sec: self.keyint_sec,
            bframes: self.enc_bframes,
            profile: self.enc_profile.clone(),
            output: self
                .output_res
                .map(|r| format!("{}x{}", r.0, r.1))
                .unwrap_or_default(),
            fps: self.fps.map(|f| f.round() as u32),
            scale: self.scale_type.clone(),
            audio_kbps: self.audio_kbps,
            sample_rate: self.sample_rate,
            rec_encoder: if self.recording_uses_stream_settings == Some(false) {
                self.rec_encoder_kind.clone()
            } else {
                String::new()
            },
            rec_preset: self.rec_preset.clone(),
            rec_rate_control: self.rec_rate_control.clone(),
            rec_cq: self.rec_cq,
        }
    }
}

impl EncSettings {
    /// What changed, in plain words: ["Preset P7 → P6", "Bitrate 6000 → 8000 kbps"].
    pub fn changes_to(&self, n: &EncSettings) -> Vec<String> {
        let mut v = Vec::new();
        let up = |s: &str| s.to_uppercase();
        let mut push = |name: &str, a: String, b: String| {
            if a != b {
                let a = if a.is_empty() { "default".into() } else { a };
                let b = if b.is_empty() { "default".into() } else { b };
                v.push(format!("{name} {a} → {b}"));
            }
        };
        let o = |x: Option<u32>| x.map(|n| n.to_string()).unwrap_or_default();
        push("Encoder", self.encoder.clone(), n.encoder.clone());
        push("Preset", up(&self.preset), up(&n.preset));
        push("Multipass", self.multipass.clone(), n.multipass.clone());
        push("Tuning", self.tune.clone(), n.tune.clone());
        push(
            "Rate control",
            self.rate_control.clone(),
            n.rate_control.clone(),
        );
        push("Bitrate (kbps)", o(self.bitrate_kbps), o(n.bitrate_kbps));
        push("Keyframes (s)", o(self.keyint_sec), o(n.keyint_sec));
        push("B-frames", o(self.bframes), o(n.bframes));
        push("Profile", self.profile.clone(), n.profile.clone());
        push("Output", self.output.clone(), n.output.clone());
        push("FPS", o(self.fps), o(n.fps));
        push("Downscale filter", self.scale.clone(), n.scale.clone());
        push("Audio (kbps)", o(self.audio_kbps), o(n.audio_kbps));
        push("Sample rate", o(self.sample_rate), o(n.sample_rate));
        push(
            "Recording encoder",
            self.rec_encoder.clone(),
            n.rec_encoder.clone(),
        );
        push("Recording preset", up(&self.rec_preset), up(&n.rec_preset));
        push(
            "Recording rate control",
            self.rec_rate_control.clone(),
            n.rec_rate_control.clone(),
        );
        push("Recording CQ", o(self.rec_cq), o(n.rec_cq));
        v
    }
}

/// NVENC's defaults for the quality options OBS leaves out of its files.
fn nvenc_defaults(s: &mut ObsSnapshot) {
    if !s.encoder_raw.to_lowercase().contains("nvenc") {
        return;
    }
    if s.enc_tune.is_empty() {
        s.enc_tune = "hq".into();
    }
    if s.enc_multipass.is_empty() {
        s.enc_multipass = "qres".into();
    }
    if s.enc_bframes.is_none() {
        s.enc_bframes = Some(2);
    }
    if s.enc_profile.is_empty() {
        s.enc_profile = "high".into();
    }
}

/// OBS leaves settings out of streamEncoder.json when they're at their
/// defaults, so fill those in.
fn advanced_defaults(s: &mut ObsSnapshot) {
    if s.rate_control.is_empty() {
        s.rate_control = "CBR".into();
    }
    if s.keyint_sec.is_none() {
        s.keyint_sec = Some(0);
    }
    if s.preset.is_empty() {
        let l = s.encoder_raw.to_lowercase();
        if l.contains("x264") {
            s.preset = "veryfast".into();
        } else if l.contains("nvenc") {
            s.preset = "p5".into();
        }
    }
}

/// Simple output mode always streams CBR with 2-second keyframes; only the
/// preset can change.
fn simple_quality(s: &mut ObsSnapshot, enc: &str, mut get: impl FnMut(&str) -> Option<String>) {
    s.rate_control = "CBR".into();
    s.keyint_sec = Some(2);
    let l = enc.to_lowercase();
    s.preset = if l.contains("x264") || l.is_empty() {
        get("Preset").unwrap_or_else(|| "veryfast".into())
    } else if l.contains("nvenc") {
        get("NVENCPreset2")
            .or_else(|| get("NVENCPreset"))
            .unwrap_or_else(|| "p5".into())
    } else if l.contains("amd") || l.contains("amf") {
        get("AMDPreset").unwrap_or_default()
    } else if l.contains("qsv") {
        get("QSVPreset").unwrap_or_default()
    } else {
        String::new()
    };
}

/// Converts OBS's volume-meter multiplier to decibels.
pub fn mul_to_db(m: f64) -> f64 {
    if m <= 0.000_001 {
        -100.0
    } else {
        20.0 * m.log10()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn linux_capture_sources() {
        assert_eq!(super::source_label("vkcapture-source"), "Game capture");
        assert_eq!(
            super::source_label("pipewire-screen-capture-source"),
            "Screen or window (PipeWire)"
        );
        assert_eq!(
            super::source_label("pipewire-window-capture-source"),
            "Window capture"
        );
        assert_eq!(super::source_label("xshm_input"), "Display capture");
        assert_eq!(super::source_label("game_capture"), "Game capture");
    }
}

#[cfg(test)]
mod audio_tests {
    use super::*;

    fn desk(device: &str, muted: bool) -> AudioInput {
        AudioInput {
            name: String::new(),
            kind: "pulse_output_capture".into(),
            role: "desktop".into(),
            muted: Some(muted),
            volume_db: Some(0.0),
            device: device.into(),
        }
    }

    #[test]
    fn doubled_desktop_audio() {
        let mut s = ObsSnapshot {
            audio: vec![desk("default", false), desk("default", false)],
            ..Default::default()
        };
        assert_eq!(
            s.doubled_desktop_audio(),
            2,
            "two captures of the same output, both on"
        );
        s.audio = vec![desk("default", false), desk("default", true)];
        assert_eq!(s.doubled_desktop_audio(), 1, "one is muted");
        s.audio = vec![desk("default", false), desk("headset", false)];
        assert_eq!(s.doubled_desktop_audio(), 1, "different devices");
    }
}
