//! One stream setup for all your games: combines every game's saved test
//! into one OBS setup that holds up in the heaviest game. When a game needs
//! more room, the advice is an in-game change, not a different OBS setup.

use crate::games::GameResult;
use crate::livetest::TestResult;
use crate::obs::ObsSnapshot;

#[derive(Clone, Debug, Default)]
pub struct GameLine {
    pub game: String,
    pub gpu_avg: Option<f32>,
    pub maxed_pct: f32,
    pub skipped_pct: f32,
    pub motion: String,
    /// "Fine", or what to change in the game.
    pub verdict: String,
    pub needs_room: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Fleet {
    pub tested: Vec<GameLine>,
    /// Installed games Preflight can read that haven't been tested.
    pub untested: Vec<String>,
    /// The fastest-moving tested game's motion ("high", "medium", "low").
    pub motion: String,
    /// Some tested game kept the GPU or encoder busy: lighter encoder settings.
    pub busy: bool,
    /// The game that sets the limits.
    pub heaviest: Option<String>,
    /// One line for the card: "1080p60 · 6000 kbps · P6".
    pub setup: String,
    /// When the current OBS setup differs from what all games need.
    pub warning: Option<String>,
}

fn maxed_pct(t: &TestResult) -> f32 {
    let v: Vec<f32> = t.samples.iter().filter_map(|s| s.gpu).collect();
    if v.is_empty() {
        0.0
    } else {
        v.iter().filter(|g| **g >= 95.0).count() as f32 * 100.0 / v.len() as f32
    }
}

fn rank(m: &str) -> u8 {
    match m {
        "high" => 3,
        "medium" => 2,
        _ => 1,
    }
}

pub fn combine(tests: &[TestResult], games: &[GameResult], obs: Option<&ObsSnapshot>) -> Fleet {
    let mut f = Fleet {
        motion: "medium".into(),
        ..Default::default()
    };
    for t in tests {
        let Some(name) = t.game_running.clone().or_else(|| t.game_guess.clone()) else {
            continue;
        };
        if f.tested.iter().any(|l| l.game == name) {
            continue;
        }
        let g = games.iter().find(|g| g.name == name);
        let motion = g
            .map(|g| g.motion.clone())
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| "medium".into());
        let maxed = maxed_pct(t);
        let skipped = if t.output_total > 0 {
            t.output_skipped as f32 * 100.0 / t.output_total as f32
        } else {
            0.0
        };
        let needs_room = maxed >= 10.0 || skipped > 0.1;
        let verdict = if !needs_room {
            "Fine with your setup".to_string()
        } else {
            // What to change in the game, from its own settings.
            let heavy: Vec<String> = g
                .map(|g| {
                    let mut v: Vec<String> = g
                        .tunables
                        .iter()
                        .filter(|t| t.change_to.is_some())
                        .take(2)
                        .map(|t| {
                            format!(
                                "{} to {}",
                                t.setting.to_lowercase(),
                                t.change_to.clone().unwrap_or_default()
                            )
                        })
                        .collect();
                    if v.is_empty() && g.findings.iter().any(|f| f.effect == "uncapped_fps") {
                        v.push("cap its FPS".into());
                    }
                    v
                })
                .unwrap_or_default();
            if heavy.is_empty() {
                "Needs room: change the game (FPS cap or lighter graphics), not OBS".to_string()
            } else {
                format!(
                    "Needs room: change the game, not OBS ({})",
                    heavy.join(", ")
                )
            }
        };
        if rank(&motion) > rank(&f.motion) {
            f.motion = motion.clone();
        }
        // Same thresholds as the per-game preset advice (0.5% skipped or GPU 85%+).
        f.busy |= skipped > 0.5 || t.gpu_avg.map(|g| g >= 85.0).unwrap_or(false);
        f.tested.push(GameLine {
            game: name,
            gpu_avg: t.gpu_avg,
            maxed_pct: maxed,
            skipped_pct: skipped,
            motion,
            verdict,
            needs_room,
        });
    }
    f.heaviest = f
        .tested
        .iter()
        .max_by(|a, b| {
            (a.maxed_pct, a.gpu_avg.unwrap_or(0.0))
                .partial_cmp(&(b.maxed_pct, b.gpu_avg.unwrap_or(0.0)))
                .unwrap()
        })
        .map(|l| l.game.clone());
    f.untested = games
        .iter()
        .filter(|g| g.installed && g.relevant() && !f.tested.iter().any(|l| l.game == g.name))
        .map(|g| g.name.clone())
        .collect();
    if let Some(o) = obs
        && let (Some(b), Some(base), Some(out)) = (o.bitrate_kbps, o.base_res, o.output_res)
    {
        let fps = o.fps.unwrap_or(60.0);
        let best = crate::analysis::best_height(base, b, fps, &f.motion, &o.codec, 2160);
        // Matches the Fix list: P5 when a game keeps the GPU busy, P6 or P7 otherwise.
        // Name the busy game so it doesn't read as a contradiction while testing a lighter one.
        let preset = if !o.encoder_kind.contains("NVENC") {
            String::new()
        } else if f.busy {
            let busy_game = f
                .tested
                .iter()
                .filter(|l| l.skipped_pct > 0.5 || l.gpu_avg.map(|g| g >= 85.0).unwrap_or(false))
                .map(|l| l.game.clone())
                .next();
            match busy_game {
                Some(g) => {
                    format!(" · P5 ({g} keeps the GPU busy; P6 or P7 is fine in lighter games)")
                }
                None => " · P5".into(),
            }
        } else {
            " · P6 or P7".into()
        };
        f.setup = format!("{}p{} · {b} kbps{preset}", best.1, fps.round() as u32);
        if out.1 > best.1 && f.tested.len() > 1 {
            let fast = f
                .tested
                .iter()
                .filter(|l| l.motion == f.motion)
                .map(|l| l.game.clone())
                .collect::<Vec<_>>()
                .join(", ");
            f.warning = Some(format!(
                "{fast} moves fast, so {}p fits all your games at {b} kbps; your output is {}p.",
                best.1, out.1
            ));
        }
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::Sample;

    fn test(game: &str, gpu: f32, maxed_secs: usize) -> TestResult {
        let mut samples: Vec<Sample> = (0..100)
            .map(|i| Sample {
                t: i as f32,
                gpu: Some(gpu),
                ..Default::default()
            })
            .collect();
        for s in samples.iter_mut().take(maxed_secs) {
            s.gpu = Some(97.0);
        }
        TestResult {
            game_running: Some(game.into()),
            gpu_avg: Some(gpu),
            samples,
            output_total: 6000,
            ..Default::default()
        }
    }

    #[test]
    fn one_setup_for_all() {
        let games = vec![
            GameResult {
                name: "Counter-Strike 2".into(),
                motion: "high".into(),
                installed: true,
                ..Default::default()
            },
            GameResult {
                name: "Cities: Skylines II".into(),
                motion: "low".into(),
                installed: true,
                files: vec![crate::games::FileResult {
                    found: true,
                    read_ok: true,
                    ..Default::default()
                }],
                ..Default::default()
            },
        ];
        let tests = vec![
            test("Counter-Strike 2", 43.0, 0),
            test("Cyberpunk 2077", 80.0, 20),
        ];
        let mut o = ObsSnapshot {
            bitrate_kbps: Some(6000),
            base_res: Some((2560, 1440)),
            output_res: Some((2560, 1440)),
            fps: Some(60.0),
            encoder_kind: "NVIDIA NVENC".into(),
            codec: "H.264".into(),
            ..Default::default()
        };
        let f = combine(&tests, &games, Some(&o));
        assert_eq!(f.tested.len(), 2);
        assert_eq!(f.motion, "high");
        assert_eq!(f.heaviest.as_deref(), Some("Cyberpunk 2077"));
        assert!(f.tested[1].needs_room && f.tested[1].verdict.contains("change the game"));
        assert!(f.setup.starts_with("1080p60 · 6000 kbps"));
        assert!(
            f.warning.is_some(),
            "1440p output flagged for the fast game"
        );
        o.output_res = Some((1920, 1080));
        assert!(combine(&tests, &games, Some(&o)).warning.is_none());
    }
}
