//! Updates from GitHub Releases (github.com/Matt-B-007/Stream-Preflight).
//! A check reads the public release list; nothing about you is sent.
//! Installing downloads this platform's file and the release's SHA256SUMS,
//! checks the file against it, and swaps the program file in place.

use serde::Serialize;
use std::time::Duration;

/// "owner/repo" once the project is on GitHub.
pub const REPO: Option<&str> = Some("Matt-B-007/Stream-Preflight");

#[derive(Clone, Debug, Serialize)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub newer: bool,
    pub notes: String,
    pub url: String,
    pub prerelease: bool,
    /// This platform's download and the checksum list, from the release.
    pub asset_url: Option<String>,
    pub sums_url: Option<String>,
}

/// The release file for this computer.
pub fn asset_name() -> &'static str {
    if cfg!(windows) { "stream-preflight-windows-x86_64.exe" } else { "stream-preflight-linux-x86_64" }
}

pub fn check(beta: bool) -> Result<UpdateInfo, String> {
    let Some(repo) = REPO else {
        return Err("Update checks turn on once the project's GitHub repository is set. This test build doesn't contact anything.".into());
    };
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .user_agent(format!("StreamPreflight/{}", crate::util::VERSION))
        .build()
        .into();
    let url = format!("https://api.github.com/repos/{repo}/releases");
    let body = agent
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("Couldn't reach GitHub: {e}"))?
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    let current = semver::Version::parse(crate::util::VERSION).map_err(|e| e.to_string())?;
    let mut best: Option<(semver::Version, serde_json::Value)> = None;
    for r in v.as_array().cloned().unwrap_or_default() {
        if r["draft"].as_bool() == Some(true) || (!beta && r["prerelease"].as_bool() == Some(true)) {
            continue;
        }
        let tag = r["tag_name"].as_str().unwrap_or("").trim_start_matches('v').to_string();
        if let Ok(ver) = semver::Version::parse(&tag) {
            if best.as_ref().map(|(b, _)| ver > *b).unwrap_or(true) {
                best = Some((ver, r));
            }
        }
    }
    let (latest, rel) = best.ok_or("No releases found.")?;
    Ok(UpdateInfo {
        current: current.to_string(),
        latest: latest.to_string(),
        newer: latest > current,
        notes: rel["body"].as_str().unwrap_or("").to_string(),
        url: rel["html_url"].as_str().unwrap_or("").to_string(),
        prerelease: rel["prerelease"].as_bool().unwrap_or(false),
        asset_url: find_asset(&rel, asset_name()),
        sums_url: find_asset(&rel, "SHA256SUMS.txt"),
    })
}

fn find_asset(rel: &serde_json::Value, name: &str) -> Option<String> {
    rel["assets"].as_array()?.iter().find(|a| a["name"].as_str() == Some(name)).and_then(|a| a["browser_download_url"].as_str()).map(|s| s.to_string())
}

fn agent(secs: u64) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(secs)))
        .user_agent(format!("StreamPreflight/{}", crate::util::VERSION))
        .build()
        .into()
}

/// The checksum for `name` in a SHA256SUMS file ("<hex>  <name>" lines).
pub fn sum_for(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let mut p = l.split_whitespace();
        let (h, n) = (p.next()?, p.next()?);
        (n.trim_start_matches('*') == name && h.len() == 64).then(|| h.to_lowercase())
    })
}

pub fn sha256_hex(b: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}

/// Downloads the update, checks it, and replaces this program file.
/// Returns the program path to start afterwards.
pub fn install(u: &UpdateInfo) -> Result<std::path::PathBuf, String> {
    let url = u.asset_url.as_ref().ok_or("This release has no download for your system yet.")?;
    let sums_url = u.sums_url.as_ref().ok_or("This release has no checksum file, so it can't be checked. Download it from the release page instead.")?;
    let a = agent(300);
    let sums = a.get(sums_url).call().map_err(|e| format!("Couldn't download the checksums: {e}"))?.body_mut().read_to_string().map_err(|e| e.to_string())?;
    let want = sum_for(&sums, asset_name()).ok_or("The checksum file doesn't list this download.")?;
    let bytes = a
        .get(url)
        .call()
        .map_err(|e| format!("Couldn't download the update: {e}"))?
        .body_mut()
        .with_config()
        .limit(200 * 1024 * 1024)
        .read_to_vec()
        .map_err(|e| format!("Download failed: {e}"))?;
    let got = sha256_hex(&bytes);
    if got != want {
        return Err("The download didn't match its checksum, so it wasn't installed. Try again later.".into());
    }
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
    #[test]
    fn checksum_lines() {
        let sums = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef  stream-preflight-linux-x86_64\nffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff *stream-preflight-windows-x86_64.exe\n";
        assert_eq!(super::sum_for(sums, "stream-preflight-linux-x86_64").unwrap().len(), 64);
        assert!(super::sum_for(sums, "stream-preflight-windows-x86_64.exe").unwrap().starts_with("ffff"));
        assert!(super::sum_for(sums, "other").is_none());
        assert_eq!(super::sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}
