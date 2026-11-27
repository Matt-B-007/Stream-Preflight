// SPDX-License-Identifier: GPL-3.0-or-later
//! Updates from GitHub Releases (github.com/Matt-B-007/Stream-Preflight).
//!
//! A release counts only when it has all three files:
//!   stream-preflight-linux.gz   the program
//!   SHA256SUMS                  its checksum
//!   SHA256SUMS.minisig          a minisign signature of SHA256SUMS
//! Installing checks the signature (either of the two embedded public keys),
//! then the checksum, unpacks the program, runs its `--self-test --quick`,
//! keeps the current program for "Go back", and only then swaps it in. A
//! release without a valid signature is treated as "not available yet".
//!
//! A check reads GitHub's public release list; nothing about you is sent.

use serde::Serialize;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// "owner/repo" on GitHub.
pub const REPO: Option<&str> = Some("Matt-B-007/Stream-Preflight");

/// The maintainer's minisign public keys. A signature from either is accepted.
/// PRIMARY signs in GitHub Actions; BACKUP is kept offline for emergencies.
pub const PUBLIC_KEYS: [(&str, &str); 2] = [
    (
        "primary",
        "RWQcs3jtP2jTiAMafwQX8UwciSjOH4o9tQcCVvYl2NkdnOeVupKPlhvC",
    ),
    (
        "backup",
        "RWRObC9qlP598z1kmTFw7W+TKYKk3w2/63qWyRWkvRSkqGwGEgjmOlGe",
    ),
];

pub const SUMS: &str = "SHA256SUMS";
pub const SIG: &str = "SHA256SUMS.minisig";

#[derive(Clone, Debug, Serialize)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub newer: bool,
    pub notes: String,
    /// The release page ("What's new").
    pub url: String,
    pub prerelease: bool,
    pub tag: String,
    pub asset_url: Option<String>,
    pub sums_url: Option<String>,
    pub sig_url: Option<String>,
}

/// The program's file name in a release, for this computer.
pub fn asset_name() -> &'static str {
    if cfg!(windows) {
        "stream-preflight-windows.exe.gz"
    } else {
        "stream-preflight-linux.gz"
    }
}

/// A version like 0.1.0-beta.8 (anything after a dash) is a beta.
pub fn is_beta_version(v: &str) -> bool {
    v.trim_start_matches('v').contains('-')
}

/// GitHub's release list, or STREAM_PREFLIGHT_API (a test server). Any
/// source still has to carry a signature from one of the embedded keys.
pub fn api_url() -> Option<String> {
    if let Ok(u) = std::env::var("STREAM_PREFLIGHT_API") {
        return Some(u);
    }
    REPO.map(|r| format!("https://api.github.com/repos/{r}/releases?per_page=30"))
}

/// Installed by a package manager (in /usr): updates come from it instead.
pub fn managed_install() -> Option<String> {
    let me = std::env::current_exe().ok()?;
    let s = me.to_string_lossy();
    (s.starts_with("/usr/") || s.starts_with("/opt/")).then(|| {
        "Stream Preflight was installed by your package manager, so update it there (for example: paru -Syu stream-preflight).".to_string()
    })
}

fn agent(secs: u64) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(secs)))
        .user_agent(format!("StreamPreflight/{}", crate::util::VERSION))
        .build()
        .into()
}

pub fn check(beta: bool) -> Result<UpdateInfo, String> {
    pick(beta, false)
}

/// The newest stable version, even if it's older than this beta ("Switch back to stable").
pub fn check_stable() -> Result<UpdateInfo, String> {
    pick(false, true)
}

fn pick(beta: bool, stable_even_if_older: bool) -> Result<UpdateInfo, String> {
    let url = api_url().ok_or("Update checks are off in this build.")?;
    let body = match agent(15).get(&url).header("Accept", "application/vnd.github+json").call() {
        Ok(mut r) => r.body_mut().read_to_string().map_err(|e| e.to_string())?,
        Err(ureq::Error::StatusCode(403)) | Err(ureq::Error::StatusCode(429)) => {
            return Err("GitHub is limiting checks right now. Try again in an hour.".into())
        }
        Err(ureq::Error::StatusCode(404)) => {
            return Err("GitHub couldn't find the release list. If the repository is private, updates can't be checked.".into())
        }
        Err(e) => return Err(format!("Couldn't reach GitHub: {e}. Check your internet connection and try again.")),
    };
    choose(&body, beta, stable_even_if_older)
}

/// Picks the right release from GitHub's JSON (kept apart from the network for testing).
pub fn choose(json: &str, beta: bool, stable_even_if_older: bool) -> Result<UpdateInfo, String> {
    let current = semver::Version::parse(crate::util::VERSION).map_err(|e| e.to_string())?;
    let list: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("GitHub sent something unexpected: {e}"))?;
    let mut best: Option<(semver::Version, UpdateInfo)> = None;
    let mut unsigned = 0;
    for r in list.as_array().into_iter().flatten() {
        if r["draft"].as_bool() == Some(true) {
            continue;
        }
        let tag = r["tag_name"].as_str().unwrap_or("");
        let Ok(v) = semver::Version::parse(tag.trim_start_matches('v')) else {
            continue;
        };
        let pre = r["prerelease"].as_bool().unwrap_or(false) || !v.pre.is_empty();
        if pre && !beta {
            continue;
        }
        let asset = |name: &str| {
            r["assets"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|a| a["name"].as_str() == Some(name))
                .and_then(|a| a["browser_download_url"].as_str())
                .map(String::from)
        };
        let (a, s, g) = (asset(asset_name()), asset(SUMS), asset(SIG));
        // Never offer a release whose program, checksum or signature is missing.
        if a.is_none() || s.is_none() || g.is_none() {
            unsigned += 1;
            continue;
        }
        let info = UpdateInfo {
            current: current.to_string(),
            latest: v.to_string(),
            newer: false,
            notes: r["body"]
                .as_str()
                .unwrap_or("")
                .chars()
                .take(4000)
                .collect(),
            url: r["html_url"].as_str().unwrap_or("").to_string(),
            prerelease: pre,
            tag: tag.to_string(),
            asset_url: a,
            sums_url: s,
            sig_url: g,
        };
        if best.as_ref().map(|(b, _)| v > *b).unwrap_or(true) {
            best = Some((v, info));
        }
    }
    let Some((v, mut u)) = best else {
        return Err(if unsigned > 0 {
            "The newest release isn't ready yet (its signed files aren't there). Try again in a few minutes.".into()
        } else if beta {
            "There are no releases on GitHub yet.".into()
        } else {
            "There's no stable version yet. Turn on Beta in Settings › Updates to get betas.".into()
        });
    };
    u.newer = if stable_even_if_older {
        v != current
    } else {
        v > current
    };
    Ok(u)
}

/// The public keys to trust: the two embedded ones (debug builds can add a test key).
fn trusted_keys() -> Vec<String> {
    #[allow(unused_mut)]
    let mut v: Vec<String> = PUBLIC_KEYS.iter().map(|(_, k)| k.to_string()).collect();
    #[cfg(debug_assertions)]
    if let Ok(k) = std::env::var("STREAM_PREFLIGHT_TEST_PUBKEY") {
        v.push(k);
    }
    v
}

/// Checks a minisign signature of `data` against the trusted keys.
/// Returns which key matched.
pub fn verify_signature(data: &[u8], sig_text: &str, keys: &[String]) -> Result<usize, String> {
    let sig = minisign_verify::Signature::decode(sig_text).map_err(|_| {
        "The update's signature file is damaged, so it wasn't installed.".to_string()
    })?;
    for (i, k) in keys.iter().enumerate() {
        if let Ok(pk) = minisign_verify::PublicKey::from_base64(k)
            && pk.verify(data, &sig, false).is_ok()
        {
            return Ok(i);
        }
    }
    Err("The update isn't signed by Stream Preflight's maintainer, so it wasn't installed.".into())
}

/// The checksum for `name` in a SHA256SUMS file ("<hash>  <name>" lines).
pub fn sum_for(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let mut p = l.split_whitespace();
        let (h, n) = (p.next()?, p.next()?);
        (n.trim_start_matches('*') == name && h.len() == 64).then(|| h.to_lowercase())
    })
}

pub fn sha256_hex(b: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(b)
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect()
}

/// Unpacks a downloaded .gz and makes sure it's a program for this system.
pub fn unpack(gz: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(gz)
        .take(400 * 1024 * 1024)
        .read_to_end(&mut out)
        .map_err(|e| format!("Couldn't unpack the update: {e}"))?;
    let magic: &[u8] = if cfg!(windows) { b"MZ" } else { b"\x7fELF" };
    if !out.starts_with(magic) {
        return Err("The download isn't a program for this system, so it wasn't installed.".into());
    }
    Ok(out)
}

fn get_bytes(url: &str, limit: u64) -> Result<Vec<u8>, String> {
    agent(300)
        .get(url)
        .call()
        .map_err(|e| match e {
            ureq::Error::StatusCode(404) => {
                "GitHub doesn't have that file (yet). Try again in a few minutes.".to_string()
            }
            e => format!("Couldn't download the update: {e}"),
        })?
        .body_mut()
        .with_config()
        .limit(limit)
        .read_to_vec()
        .map_err(|e| format!("Download failed: {e}"))
}

/// Where the previous version is kept for "Go back".
pub fn previous_dir() -> PathBuf {
    crate::util::dirs().data.join("previous")
}

/// The version kept for "Go back", if any.
pub fn previous_version() -> Option<String> {
    let d = previous_dir();
    d.join(prog_name())
        .exists()
        .then(|| std::fs::read_to_string(d.join("version.txt")).ok())
        .flatten()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn prog_name() -> &'static str {
    if cfg!(windows) {
        "stream-preflight.exe"
    } else {
        "stream-preflight"
    }
}

/// Runs `<program> --self-test --quick` and waits up to 30 seconds.
pub fn quick_self_test(p: &Path) -> Result<(), String> {
    let mut c = std::process::Command::new(p);
    c.args(["--self-test", "--quick"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = c
        .spawn()
        .map_err(|e| format!("The new version couldn't start: {e}"))?;
    let t = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(st)) if st.success() => return Ok(()),
            Ok(Some(st)) => {
                let mut err = String::new();
                if let Some(mut o) = child.stdout.take() {
                    let _ = o.read_to_string(&mut err);
                }
                log::warn!("New version's self-test failed ({st}): {}", err.trim());
                return Err("The new version failed its self-test, so it wasn't installed. You're still on the current version.".into());
            }
            Ok(None) if t.elapsed() < Duration::from_secs(30) => {
                std::thread::sleep(Duration::from_millis(100))
            }
            _ => {
                let _ = child.kill();
                return Err(
                    "The new version didn't finish its self-test, so it wasn't installed.".into(),
                );
            }
        }
    }
}

#[cfg(unix)]
fn make_executable(p: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755));
}
#[cfg(not(unix))]
fn make_executable(_p: &Path) {}

/// Keeps a copy of the running program (and its version) for "Go back".
fn keep_previous(me: &Path, version: &str) -> Result<(), String> {
    let d = previous_dir();
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    let tmp = d.join(format!("{}.tmp", prog_name()));
    std::fs::copy(me, &tmp)
        .map_err(|e| format!("Couldn't keep the current version for Go back: {e}"))?;
    make_executable(&tmp);
    std::fs::rename(&tmp, d.join(prog_name())).map_err(|e| e.to_string())?;
    let vt = d.join("version.txt.tmp");
    std::fs::write(&vt, version).map_err(|e| e.to_string())?;
    std::fs::rename(&vt, d.join("version.txt")).map_err(|e| e.to_string())
}

/// Puts `new` (already next to the program) in place of the running program.
fn swap_in(new: &Path, me: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        // A running .exe can be renamed but not overwritten.
        let old = me.with_extension("old");
        let _ = std::fs::remove_file(&old);
        std::fs::rename(me, &old)
            .map_err(|e| format!("Couldn't move the old version aside: {e}"))?;
    }
    if let Err(e) = std::fs::rename(new, me) {
        #[cfg(windows)]
        let _ = std::fs::rename(me.with_extension("old"), me);
        return Err(format!("Couldn't put the update in place: {e}"));
    }
    Ok(())
}

/// Downloads, checks (signature, checksum, self-test), keeps the current
/// version for Go back, and swaps in the update. Returns the program to start.
pub fn install(u: &UpdateInfo) -> Result<PathBuf, String> {
    install_with(u, &trusted_keys())
}

pub fn install_with(u: &UpdateInfo, keys: &[String]) -> Result<PathBuf, String> {
    if let Some(m) = managed_install() {
        return Err(m);
    }
    let (Some(au), Some(su), Some(gu)) = (&u.asset_url, &u.sums_url, &u.sig_url) else {
        return Err("This release isn't ready yet (its signed files aren't there).".into());
    };
    let sums = get_bytes(su, 64 * 1024)?;
    let sig = String::from_utf8(get_bytes(gu, 16 * 1024)?)
        .map_err(|_| "The signature file is damaged.".to_string())?;
    let which = verify_signature(&sums, &sig, keys)?;
    let want = sum_for(&String::from_utf8_lossy(&sums), asset_name())
        .ok_or("The checksum list doesn't include the program, so it wasn't installed.")?;
    let gz = get_bytes(au, 200 * 1024 * 1024)?;
    if sha256_hex(&gz) != want {
        return Err("The download didn't match its checksum, so it wasn't installed. Try again in a few minutes.".into());
    }
    let bytes = unpack(&gz)?;
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let new = me.with_extension("new");
    std::fs::write(&new, &bytes).map_err(|e| format!("Couldn't save the update: {e}"))?;
    make_executable(&new);
    if let Err(e) = quick_self_test(&new) {
        let _ = std::fs::remove_file(&new);
        return Err(e);
    }
    keep_previous(&me, crate::util::VERSION)?;
    swap_in(&new, &me)?;
    log::info!(
        "Updated to v{} (signature: {} key, checksum ok, self-test ok)",
        u.latest,
        if which == 0 { "primary" } else { "backup" }
    );
    Ok(me)
}

/// "Go back": puts the kept previous version in place, and keeps the current
/// one so you can return. Returns (the version now installed, the program).
pub fn rollback() -> Result<(String, PathBuf), String> {
    if let Some(m) = managed_install() {
        return Err(m);
    }
    let prev_ver = previous_version()
        .ok_or("There's no earlier version kept on this PC yet. One is kept after each update.")?;
    let src = previous_dir().join(prog_name());
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let new = me.with_extension("new");
    std::fs::copy(&src, &new).map_err(|e| format!("Couldn't copy the earlier version: {e}"))?;
    make_executable(&new);
    // Versions before beta.8 don't have a quick self-test (they'd open a window).
    let has_self_test = semver::Version::parse(&prev_ver)
        .map(|v| v >= semver::Version::parse("0.1.0-beta.8").unwrap_or(v.clone()))
        .unwrap_or(false);
    if has_self_test && let Err(e) = quick_self_test(&new) {
        let _ = std::fs::remove_file(&new);
        return Err(e);
    }
    keep_previous(&me, crate::util::VERSION)?;
    swap_in(&new, &me)?;
    log::info!("Went back to v{prev_ver}");
    Ok((prev_ver, me))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, pre: bool, files: &[&str]) -> serde_json::Value {
        serde_json::json!({
            "tag_name": tag, "prerelease": pre, "draft": false, "html_url": format!("https://x/{tag}"), "body": "notes",
            "assets": files.iter().map(|f| serde_json::json!({"name": f, "browser_download_url": format!("https://x/{tag}/{f}")})).collect::<Vec<_>>()
        })
    }
    const ALL: [&str; 3] = ["stream-preflight-linux.gz", SUMS, SIG];

    #[test]
    fn embedded_keys_parse() {
        for (name, k) in PUBLIC_KEYS {
            assert!(
                minisign_verify::PublicKey::from_base64(k).is_ok(),
                "{name} key"
            );
        }
    }

    #[test]
    fn picks_signed_releases_only() {
        let list = serde_json::json!([
            release("v9.0.0-beta.2", true, &["stream-preflight-linux.gz", SUMS]), // no signature yet
            release("v9.0.0-beta.1", true, &ALL),
            release("v8.0.0", false, &ALL),
        ]);
        let u = choose(&list.to_string(), true, false).unwrap();
        assert_eq!(
            u.latest, "9.0.0-beta.1",
            "the unsigned newer beta is skipped"
        );
        assert!(u.newer && u.prerelease);
        assert_eq!(
            u.sig_url.as_deref(),
            Some("https://x/v9.0.0-beta.1/SHA256SUMS.minisig")
        );
        let s = choose(&list.to_string(), false, false).unwrap();
        assert_eq!((s.latest.as_str(), s.prerelease), ("8.0.0", false));
        // Only unsigned releases: "not ready yet".
        let only = serde_json::json!([release("v9.0.0", false, &["stream-preflight-linux.gz"])]);
        assert!(
            choose(&only.to_string(), true, false)
                .unwrap_err()
                .contains("isn't ready yet")
        );
        // Drafts never count.
        let mut d = release("v9.9.9", false, &ALL);
        d["draft"] = serde_json::json!(true);
        assert!(choose(&serde_json::json!([d]).to_string(), true, false).is_err());
    }

    #[test]
    fn switch_back_to_stable_offers_older() {
        let list = serde_json::json!([release("v0.0.1", false, &ALL)]);
        assert!(choose(&list.to_string(), false, true).unwrap().newer);
        assert!(!choose(&list.to_string(), false, false).unwrap().newer);
    }

    #[test]
    fn signatures_from_either_key() {
        let kp = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
        let other = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
        let data = b"abc  stream-preflight-linux.gz\n";
        let sig = minisign::sign(
            Some(&kp.pk),
            &kp.sk,
            &data[..],
            Some("Stream Preflight test"),
            None,
        )
        .unwrap()
        .into_string();
        let ours = kp.pk.to_base64();
        let theirs = other.pk.to_base64();
        assert_eq!(
            verify_signature(data, &sig, &[ours.clone(), theirs.clone()]),
            Ok(0)
        );
        assert_eq!(
            verify_signature(data, &sig, &[theirs.clone(), ours.clone()]),
            Ok(1),
            "the second key works too"
        );
        assert!(
            verify_signature(data, &sig, &[theirs]).is_err(),
            "a stranger's signature is refused"
        );
        assert!(
            verify_signature(b"changed", &sig, std::slice::from_ref(&ours)).is_err(),
            "changed data is refused"
        );
        assert!(verify_signature(data, "garbage", &[ours]).is_err());
    }

    #[test]
    fn checksum_lines() {
        let s = "1111111111111111111111111111111111111111111111111111111111111111  install.sh\nAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA *stream-preflight-linux.gz\n";
        assert_eq!(
            sum_for(s, "stream-preflight-linux.gz").as_deref(),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
        assert!(sum_for(s, "missing").is_none());
        assert!(is_beta_version("v0.1.0-beta.8") && !is_beta_version("0.1.0"));
    }

    #[test]
    fn unpack_checks_the_program() {
        use std::io::Write;
        let gz = |b: &[u8]| {
            let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
            e.write_all(b).unwrap();
            e.finish().unwrap()
        };
        let magic: &[u8] = if cfg!(windows) {
            b"MZ rest"
        } else {
            b"\x7fELF rest"
        };
        assert_eq!(unpack(&gz(magic)).unwrap(), magic);
        assert!(unpack(&gz(b"<html>404</html>")).is_err());
    }
}
