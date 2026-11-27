//! Paired mode: Preflight on the gaming PC and Preflight on the streaming PC
//! share what each can see, over the home network only.
//!
//! - The streaming PC (where OBS runs) listens on TCP port 47715.
//! - The gaming PC connects to it at the same address it uses for OBS.
//! - Pairing needs a 6-digit code shown on the streaming PC, valid 5 minutes,
//!   5 tries. After that both PCs keep a random key (stored like the OBS
//!   password) and every message carries it.
//! - Only private home-network addresses are answered. No internet, no names:
//!   the PCs call each other "Gaming PC" and "Streaming PC".
//! - What's shared: game name, GPU/CPU load, monitor refresh rate, open fix
//!   titles; OBS status, stream bitrate, skipped frames, encoder load. Never
//!   keys, passwords, scene names, addresses or video.

use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const PORT: u16 = 47715;
const MAX_LINE: usize = 64 * 1024;

/// What the gaming PC shares.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct GameSide {
    pub game: Option<String>,
    pub gpu: Option<f32>,
    pub gpu_temp: Option<f32>,
    pub cpu: f32,
    pub monitor_hz: u32,
    pub fixes: Vec<String>,
    /// Measured on the gaming PC while the streaming PC's test ran.
    pub test: Option<SideTest>,
}

/// What the streaming PC shares.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct StreamSide {
    pub obs_connected: bool,
    pub live: bool,
    pub kbps: Option<f32>,
    pub skipped_pct: Option<f32>,
    pub encoder_busy: Option<f32>,
    pub cpu: f32,
    /// A test is measuring right now; the gaming PC measures along.
    pub measuring: bool,
    pub test_id: u64,
    pub fixes: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct SideTest {
    pub test_id: u64,
    pub seconds: f32,
    pub gpu_avg: Option<f32>,
    pub gpu_maxed_pct: f32,
    pub cpu_avg: f32,
    pub game: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "t")]
enum Msg {
    #[serde(rename = "pair")]
    Pair { code: String },
    #[serde(rename = "paired")]
    Paired { token: String },
    #[serde(rename = "hello")]
    Hello { token: String, game: GameSide },
    #[serde(rename = "report")]
    Report { stream: StreamSide },
    #[serde(rename = "no")]
    No { why: String },
}

/// Shared between the app and the network thread.
#[derive(Default)]
pub struct PairState {
    /// Streaming PC: the code on screen, when it was made, tries left.
    pub code: Option<(String, Instant, u8)>,
    pub token: Option<String>,
    pub my_stream: StreamSide,
    pub my_game: GameSide,
    pub peer_game: Option<(GameSide, Instant)>,
    pub peer_stream: Option<(StreamSide, Instant)>,
    /// Gaming PC: a code typed in, waiting to be sent.
    pub pair_with: Option<String>,
    /// A line for the app: "Paired", "Wrong code", "Can't reach the streaming PC".
    pub status: String,
    /// Set when pairing just succeeded; the app saves the key.
    pub new_token: Option<String>,
}

pub struct Pairing {
    pub state: Arc<Mutex<PairState>>,
    stop: Arc<AtomicBool>,
}

impl Drop for Pairing {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// 10.x, 172.16–31.x, 192.168.x, link-local, unique-local IPv6, and loopback.
pub fn is_home_network(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => v.is_private() || v.is_link_local() || v.is_loopback(),
        IpAddr::V6(v) => {
            let s = v.segments()[0];
            v.is_loopback()
                || (s & 0xffc0) == 0xfe80
                || (s & 0xfe00) == 0xfc00
                || v.to_ipv4_mapped()
                    .map(|m| m.is_private() || m.is_loopback())
                    .unwrap_or(false)
        }
    }
}

fn random_hex(n: usize) -> String {
    let mut b = vec![0u8; n];
    if getrandom::fill(&mut b).is_err() {
        return String::new();
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// A fresh 6-digit pairing code.
pub fn new_code() -> String {
    let mut b = [0u8; 4];
    let _ = getrandom::fill(&mut b);
    format!("{:06}", u32::from_le_bytes(b) % 1_000_000)
}

/// Compares without stopping at the first different character.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

fn read_msg(s: &mut TcpStream) -> Option<Msg> {
    s.set_read_timeout(Some(Duration::from_secs(3))).ok()?;
    let mut line = String::new();
    let mut r = BufReader::new(s.try_clone().ok()?).take(MAX_LINE as u64);
    r.read_line(&mut line).ok()?;
    serde_json::from_str(line.trim()).ok()
}

fn send_msg(s: &mut TcpStream, m: &Msg) -> bool {
    let Ok(mut t) = serde_json::to_string(m) else {
        return false;
    };
    t.push('\n');
    s.write_all(t.as_bytes()).is_ok()
}

use std::io::Read;

/// Streaming PC: answer the gaming PC.
pub fn start_host(token: Option<String>, ctx: eframe::egui::Context) -> Result<Pairing, String> {
    let listener =
        TcpListener::bind(("0.0.0.0", PORT)).map_err(|e| format!("Can't open port {PORT}: {e}"))?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let state = Arc::new(Mutex::new(PairState {
        token,
        ..Default::default()
    }));
    let stop = Arc::new(AtomicBool::new(false));
    let (st, sp) = (state.clone(), stop.clone());
    std::thread::Builder::new()
        .name("pair-host".into())
        .spawn(move || {
            while !sp.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut s, peer)) => {
                        if !is_home_network(peer.ip()) {
                            log::info!(
                                "Paired mode: ignored a connection from outside the home network"
                            );
                            continue;
                        }
                        let _ = s.set_nonblocking(false);
                        handle(&mut s, &st);
                        ctx.request_repaint();
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(150))
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(500)),
                }
            }
        })
        .map_err(|e| e.to_string())?;
    log::info!("Paired mode: listening on the home network (port {PORT})");
    Ok(Pairing { state, stop })
}

fn handle(s: &mut TcpStream, st: &Arc<Mutex<PairState>>) {
    let Some(m) = read_msg(s) else { return };
    let reply = {
        let mut g = st.lock().unwrap_or_else(|e| e.into_inner());
        match m {
            Msg::Pair { code } => match g.code.take() {
                Some((want, made, tries))
                    if made.elapsed() < Duration::from_secs(300) && tries > 0 =>
                {
                    if same(&want, code.trim()) {
                        let token = random_hex(32);
                        g.token = Some(token.clone());
                        g.new_token = Some(token.clone());
                        g.status = "Paired with your gaming PC".into();
                        log::info!("Paired mode: paired with the gaming PC");
                        Msg::Paired { token }
                    } else {
                        g.code = Some((want, made, tries - 1));
                        Msg::No {
                            why: "wrong code".into(),
                        }
                    }
                }
                _ => Msg::No {
                    why: "no code on screen".into(),
                },
            },
            Msg::Hello { token, game } => match &g.token {
                Some(t) if same(t, &token) => {
                    g.peer_game = Some((game, Instant::now()));
                    Msg::Report {
                        stream: g.my_stream.clone(),
                    }
                }
                _ => Msg::No {
                    why: "not paired".into(),
                },
            },
            _ => Msg::No {
                why: "unexpected".into(),
            },
        }
    };
    send_msg(s, &reply);
}

/// Gaming PC: talk to the streaming PC at the OBS address every 3 seconds.
pub fn start_client(
    host: String,
    token: Option<String>,
    ctx: eframe::egui::Context,
) -> Result<Pairing, String> {
    let state = Arc::new(Mutex::new(PairState {
        token,
        ..Default::default()
    }));
    let stop = Arc::new(AtomicBool::new(false));
    let (st, sp) = (state.clone(), stop.clone());
    std::thread::Builder::new()
        .name("pair-client".into())
        .spawn(move || {
            let mut last_error_logged = false;
            while !sp.load(Ordering::Relaxed) {
                let addr = std::net::ToSocketAddrs::to_socket_addrs(&(host.as_str(), PORT)).ok().and_then(|mut a| a.next());
                let pending = { st.lock().map(|g| g.pair_with.clone()).unwrap_or(None) };
                let token = { st.lock().map(|g| g.token.clone()).unwrap_or(None) };
                if let Some(addr) = addr.filter(|a: &SocketAddr| is_home_network(a.ip())) {
                    if let Some(code) = pending {
                        let res = TcpStream::connect_timeout(&addr, Duration::from_secs(2)).ok().and_then(|mut s| {
                            send_msg(&mut s, &Msg::Pair { code });
                            read_msg(&mut s)
                        });
                        let mut g = st.lock().unwrap_or_else(|e| e.into_inner());
                        g.pair_with = None;
                        match res {
                            Some(Msg::Paired { token }) => {
                                g.token = Some(token.clone());
                                g.new_token = Some(token);
                                g.status = "Paired with your streaming PC".into();
                                log::info!("Paired mode: paired with the streaming PC");
                            }
                            Some(Msg::No { why }) => g.status = if why == "wrong code" { "That code didn't match. Check it on your streaming PC.".into() } else { "Your streaming PC isn't showing a code. Click Pair there first.".into() },
                            _ => g.status = "Can't reach Preflight on your streaming PC. Is it open, and allowed through its firewall?".into(),
                        }
                        ctx.request_repaint();
                    } else if let Some(token) = token {
                        let game = { st.lock().map(|g| g.my_game.clone()).unwrap_or_default() };
                        let res = TcpStream::connect_timeout(&addr, Duration::from_secs(2)).ok().and_then(|mut s| {
                            send_msg(&mut s, &Msg::Hello { token, game });
                            read_msg(&mut s)
                        });
                        let mut g = st.lock().unwrap_or_else(|e| e.into_inner());
                        match res {
                            Some(Msg::Report { stream }) => {
                                g.peer_stream = Some((stream, Instant::now()));
                                g.status = "Connected".into();
                                last_error_logged = false;
                            }
                            Some(Msg::No { .. }) => g.status = "Your streaming PC no longer knows this PC. Pair again.".into(),
                            _ => {
                                g.status = "Can't reach your streaming PC right now".into();
                                if !last_error_logged {
                                    log::info!("Paired mode: streaming PC not reachable");
                                    last_error_logged = true;
                                }
                            }
                        }
                        ctx.request_repaint();
                    }
                } else if pending.is_some()
                    && let Ok(mut g) = st.lock() {
                        g.pair_with = None;
                        g.status = "The OBS address isn't on your home network, so pairing can't use it.".into();
                    }
                for _ in 0..30 {
                    if sp.load(Ordering::Relaxed) || st.lock().map(|g| g.pair_with.is_some()).unwrap_or(false) {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(Pairing { state, stop })
}

/// The gaming PC measures along with the streaming PC's test.
#[derive(Default)]
pub struct SideMeasure {
    pub test_id: u64,
    pub started: Option<Instant>,
    pub gpu: Vec<f32>,
    pub cpu: Vec<f32>,
    pub game: Option<String>,
}

impl SideMeasure {
    pub fn summary(&self) -> SideTest {
        let avg = |v: &[f32]| {
            if v.is_empty() {
                None
            } else {
                Some(v.iter().sum::<f32>() / v.len() as f32)
            }
        };
        let maxed = if self.gpu.is_empty() {
            0.0
        } else {
            self.gpu.iter().filter(|g| **g >= 92.0).count() as f32 * 100.0 / self.gpu.len() as f32
        };
        SideTest {
            test_id: self.test_id,
            seconds: self
                .started
                .map(|s| s.elapsed().as_secs_f32())
                .unwrap_or(0.0),
            gpu_avg: avg(&self.gpu),
            gpu_maxed_pct: maxed,
            cpu_avg: avg(&self.cpu).unwrap_or(0.0),
            game: self.game.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_home_network() {
        assert!(is_home_network("192.168.1.10".parse().unwrap()));
        assert!(is_home_network("10.0.0.5".parse().unwrap()));
        assert!(is_home_network("172.20.1.1".parse().unwrap()));
        assert!(is_home_network("fe80::1".parse().unwrap()));
        assert!(!is_home_network("8.8.8.8".parse().unwrap()));
        assert!(!is_home_network("2001:4860::8888".parse().unwrap()));
    }

    #[test]
    fn codes_and_keys() {
        let c = new_code();
        assert_eq!(c.len(), 6);
        assert!(c.chars().all(|x| x.is_ascii_digit()));
        assert_eq!(random_hex(32).len(), 64);
        assert!(same("123456", "123456"));
        assert!(!same("123456", "123457"));
    }

    #[test]
    fn pairing_round_trip_on_this_pc() {
        // Host and client on loopback: pair with the code, then exchange reports.
        let ctx = eframe::egui::Context::default();
        let host = match start_host(None, ctx.clone()) {
            Ok(h) => h,
            Err(_) => return, // port in use on this machine
        };
        host.state.lock().unwrap().code = Some(("424242".into(), Instant::now(), 5));
        host.state.lock().unwrap().my_stream = StreamSide {
            obs_connected: true,
            kbps: Some(6000.0),
            ..Default::default()
        };
        let client = start_client("127.0.0.1".into(), None, ctx).unwrap();
        client.state.lock().unwrap().pair_with = Some("000000".into());
        std::thread::sleep(Duration::from_millis(800));
        assert!(
            client.state.lock().unwrap().token.is_none(),
            "wrong code must not pair"
        );
        client.state.lock().unwrap().pair_with = Some("424242".into());
        std::thread::sleep(Duration::from_millis(800));
        assert!(
            client.state.lock().unwrap().token.is_some(),
            "right code pairs"
        );
        client.state.lock().unwrap().my_game = GameSide {
            game: Some("Test Game".into()),
            monitor_hz: 144,
            ..Default::default()
        };
        std::thread::sleep(Duration::from_millis(3500));
        let peer = client.state.lock().unwrap().peer_stream.clone();
        assert_eq!(peer.map(|p| p.0.kbps), Some(Some(6000.0)));
        let g = host.state.lock().unwrap().peer_game.clone();
        assert_eq!(g.map(|p| p.0.monitor_hz), Some(144));
    }
}
