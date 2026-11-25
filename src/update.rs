//! Updates straight from the GitHub repository (github.com/Matt-B-007/Stream-Preflight).
//!
//! The repo has a `download/` folder:
//!   download/version.txt                     which versions are there, and their checksums
//!   download/stream-preflight-linux.gz       the newest stable version (once there is one)
//!   download/beta/stream-preflight-linux.gz  the newest beta
//!
//! A check reads version.txt; nothing about you is sent. Installing downloads
//! the file, checks it against the SHA-256 in version.txt, unpacks it and
//! swaps the program file in place. No GitHub Releases or tags are involved.

use serde::Serialize;
use std::io::Read;
use std::time::Duration;

/// "owner/repo" on GitHub.
pub const REPO: Option<&str> = Some("Matt-B-007/Stream-Preflight");

#[derive(Clone, Debug, Serialize)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub newer: bool,
    pub notes: String,
    /// Where "What's new" goes (the changelog).
    pub url: String,
    pub prerelease: bool,
    /// The download for this computer, and its SHA-256 from version.txt.
    pub asset_url: Option<String>,
    pub sha256: Option<String>,
}

/// The file name for this computer inside download/ and download/beta/.
pub fn asset_name() -> &'static str {
    if cfg!(windows) { "stream-preflight-windows.exe.gz" } else { "stream-preflight-linux.gz" }
}

/// A version like 0.1.0-beta.4 (anything after a dash) is a beta.
pub fn is_beta_version(v: &str) -> bool {
    v.trim_start_matches('v').contains('-')
}

/// The download folder: the repo's download/ on GitHub, or
/// STREAM_PREFLIGHT_DOWNLOADS (a test folder, for testing updates only).
pub fn base_url() -> Option<String> {
    if let Ok(b) = std::env::var("STREAM_PREFLIGHT_DOWNLOADS") {
        return Some(b.trim_end_matches('/').to_string());
    }
    REPO.map(|r| format!("https://raw.githubusercontent.com/{r}/main/download"))
}

fn agent(secs: u64) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(secs)))
        .user_agent(format!("StreamPreflight/{}", crate::util::VERSION))
        .build()
        .into()
}

/// GitHub caches these files for a few minutes; a changing query asks for a fresh copy.
fn fresh(url: &str) -> String {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    format!("{url}?t={t}")
}

/// One channel's entry in version.txt.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Entry {
    pub version: String,
    pub sha256: String,
    pub notes: String,
}

/// Reads version.txt: `key=value` lines, `#` comments.
/// Keys: stable_version, stable_sha256, stable_notes, beta_version, beta_sha256, beta_notes.
pub fn parse_manifest(text: &str) -> (Option<Entry>, Option<Entry>) {
    let mut s = Entry::default();
    let mut b = Entry::default();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        let v = v.trim().to_string();
        match k.trim() {
            "stable_version" => s.version = v.trim_start_matches('v').to_string(),
            "stable_sha256" => s.sha256 = v.to_lowercase(),
            "stable_notes" => s.notes = v,
            "beta_version" => b.version = v.trim_start_matches('v').to_string(),
            "beta_sha256" => b.sha256 = v.to_lowercase(),
            "beta_notes" => b.notes = v,
            _ => {}
        }
    }
    let ok = |e: Entry| (!e.version.is_empty() && e.sha256.len() == 64).then_some(e);
    (ok(s), ok(b))
}

pub fn check(beta: bool) -> Result<UpdateInfo, String> {
    pick(beta, false)
}

/// The newest stable version, even if it's older than this beta ("Switch back to stable").
pub fn check_stable() -> Result<UpdateInfo, String> {
    pick(false, true)
}

fn pick(beta: bool, stable_even_if_older: bool) -> Result<UpdateInfo, String> {
    let base = base_url().ok_or("Update checks are off in this build.")?;
    let manifest = match agent(10).get(&fresh(&format!("{base}/version.txt"))).call() {
        Ok(mut r) => r.body_mut().read_to_string().map_err(|e| e.to_string())?,
        Err(ureq::Error::StatusCode(404)) => {
            return Err("GitHub doesn't have a download list yet (download/version.txt). If it was just uploaded, try again in a few minutes.".into())
        }
        Err(e) => return Err(format!("Couldn't reach GitHub: {e}")),
    };
    choose(&manifest, &base, beta, stable_even_if_older)
}

/// Picks the right entry from version.txt (kept apart from the network for testing).
pub fn choose(manifest: &str, base: &str, beta: bool, stable_even_if_older: bool) -> Result<UpdateInfo, String> {
    let current = semver::Version::parse(crate::util::VERSION).map_err(|e| e.to_string())?;
    let (stable, betav) = parse_manifest(manifest);
    let mut options: Vec<(semver::Version, Entry, String)> = Vec::new();
    if let Some(e) = stable {
        if let Ok(v) = semver::Version::parse(&e.version) {
            options.push((v, e, format!("{base}/{}", asset_name())));
        }
    }
    if beta {
        if let Some(e) = betav {
            if let Ok(v) = semver::Version::parse(&e.version) {
                options.push((v, e, format!("{base}/beta/{}", asset_name())));
            }
        }
    }
    let (latest, entry, url) = options
        .into_iter()
        .max_by(|a, b| a.0.cmp(&b.0))
        .ok_or(if beta { "Nothing has been put in the download folder yet." } else { "There's no stable version yet. Turn on Include beta versions to get betas." })?;
    Ok(UpdateInfo {
        current: current.to_string(),
        latest: latest.to_string(),
        newer: if stable_even_if_older { latest != current } else { latest > current },
        notes: entry.notes.clone(),
        url: REPO.map(|r| format!("https://github.com/{r}/blob/main/CHANGELOG.md")).unwrap_or_default(),
        prerelease: is_beta_version(&latest.to_string()),
        asset_url: Some(url),
        sha256: Some(entry.sha256),
    })
}

pub fn sha256_hex(b: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
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

/// Downloads the update, checks it, and replaces this program file.
/// Returns the program path to start afterwards.
pub fn install(u: &UpdateInfo) -> Result<std::path::PathBuf, String> {
    let url = u.asset_url.as_ref().ok_or("There's no download for your system yet.")?;
    let want = u.sha256.as_ref().ok_or("The download list has no checksum for this version, so it can't be checked.")?;
    let gz = agent(300)
        .get(&fresh(url))
        .call()
        .map_err(|e| match e {
            ureq::Error::StatusCode(404) => "GitHub doesn't have the download file yet. If it was just uploaded, try again in a few minutes.".to_string(),
            e => format!("Couldn't download the update: {e}"),
        })?
        .body_mut()
        .with_config()
        .limit(200 * 1024 * 1024)
        .read_to_vec()
        .map_err(|e| format!("Download failed: {e}"))?;
    if &sha256_hex(&gz) != want {
        return Err("The download didn't match its checksum, so it wasn't installed. If a new version was just uploaded, GitHub may still be catching up: try again in a few minutes.".into());
    }
    let bytes = unpack(&gz)?;
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let tmp = me.with_extension("new");
    std::fs::write(&tmp, &bytes).map_err(|e| format!("Couldn't save the update: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755));
    }
    #[cfg(windows)]
    {
        // A running .exe can be renamed but not overwritten.
        let old = me.with_extension("old");
        let _ = std::fs::remove_file(&old);
        std::fs::rename(&me, &old).map_err(|e| format!("Couldn't move the old version aside: {e}"))?;
    }
    if let Err(e) = std::fs::rename(&tmp, &me) {
        #[cfg(windows)]
        let _ = std::fs::rename(me.with_extension("old"), &me);
        return Err(format!("Couldn't put the update in place: {e}"));
    }
    log::info!("Updated to v{} (checksum ok)", u.latest);
    Ok(me)
}

#[cfg(test)]
mod tests {
    use super::*;

    const H1: &str = "1111111111111111111111111111111111111111111111111111111111111111";
    const H2: &str = "2222222222222222222222222222222222222222222222222222222222222222";

    #[test]
    fn betas_by_name_and_order() {
        assert!(is_beta_version("v0.1.0-beta.4"));
        assert!(!is_beta_version("0.1.0"));
        let b4 = semver::Version::parse("0.1.0-beta.4").unwrap();
        let b3 = semver::Version::parse("0.1.0-beta.3").unwrap();
        let s = semver::Version::parse("0.1.0").unwrap();
        assert!(b4 > b3 && s > b4, "stable 0.1.0 is newer than its betas");
    }

    #[test]
    fn manifest_parsing() {
        let m = format!("# comment\nstable_version=\nstable_sha256=\nbeta_version=v0.1.0-beta.9\nbeta_sha256={H1}\nbeta_notes=Hello = world\n");
        let (s, b) = parse_manifest(&m);
        assert!(s.is_none(), "an empty stable entry means no stable yet");
        let b = b.unwrap();
        assert_eq!(b.version, "0.1.0-beta.9");
        assert_eq!(b.notes, "Hello = world");
        // A bad checksum makes the entry unusable.
        assert!(parse_manifest("beta_version=0.1.0-beta.9\nbeta_sha256=abc").1.is_none());
    }

    #[test]
    fn choosing_versions() {
        // Only a newer beta in the folder.
        let only_beta = format!("beta_version=9.0.0-beta.1\nbeta_sha256={H1}\n");
        let u = choose(&only_beta, "http://x/download", true, false).unwrap();
        assert!(u.newer && u.prerelease);
        assert_eq!(u.asset_url.as_deref(), Some(format!("http://x/download/beta/{}", asset_name()).as_str()));
        assert_eq!(u.sha256.as_deref(), Some(H1));
        assert!(choose(&only_beta, "http://x/download", false, false).is_err(), "stable-only users don't get betas");

        // Stable and beta: the newest wins when betas are on; stable only when off.
        let both = format!("stable_version=9.0.0\nstable_sha256={H2}\nbeta_version=9.1.0-beta.1\nbeta_sha256={H1}\n");
        assert_eq!(choose(&both, "b", true, false).unwrap().latest, "9.1.0-beta.1");
        let s = choose(&both, "b", false, false).unwrap();
        assert_eq!((s.latest.as_str(), s.prerelease), ("9.0.0", false));
        assert_eq!(s.asset_url.as_deref(), Some(format!("b/{}", asset_name()).as_str()));

        // This exact version isn't "newer".
        let same = format!("beta_version={}\nbeta_sha256={H1}\n", crate::util::VERSION);
        assert!(!choose(&same, "b", true, false).unwrap().newer);

        // Switch back to stable offers an older stable.
        let older = format!("stable_version=0.0.1\nstable_sha256={H2}\n");
        assert!(choose(&older, "b", false, true).unwrap().newer);
        assert!(!choose(&older, "b", false, false).unwrap().newer);
    }

    #[test]
    fn unpack_checks_the_program() {
        use std::io::Write;
        let gz = |b: &[u8]| {
            let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
            e.write_all(b).unwrap();
            e.finish().unwrap()
        };
        let magic: &[u8] = if cfg!(windows) { b"MZ rest" } else { b"\x7fELF rest" };
        assert_eq!(unpack(&gz(magic)).unwrap(), magic);
        assert!(unpack(&gz(b"<html>404</html>")).is_err());
        assert!(unpack(b"not gzip").is_err());
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}
