//! Self-update from GitHub Releases (github.com/visiuun/equinox).
//!
//! The release zip for this machine is downloaded, checked against the size
//! and SHA-256 GitHub publishes for it, and unpacked next to the running
//! program. Each exe is swapped by renaming: the running one becomes `.old`
//! (Windows allows renaming a running image), the new one takes its name, and
//! the app is started again. The `.old` files are deleted on the next start.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use sha2::{Digest, Sha256};

pub const RELEASES_PAGE: &str = "https://github.com/visiuun/equinox/releases";
const API: &str = "https://api.github.com/repos/visiuun/equinox/releases/latest";
const UA: &str = concat!("equinox/", env!("CARGO_PKG_VERSION"));
/// The programs a release zip carries.
const EXES: [&str; 2] = ["equinox.exe", "eqx.exe"];
/// Passed to the restarted app, which then waits for this one to exit.
pub const RESTART_FLAG: &str = "--updated";

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: String,
    pub url: String,
    pub size: u64,
    pub sha256: Option<String>,
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<ApiAsset>,
}

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    digest: Option<String>,
}

/// The release zip built for this program's architecture.
fn asset_name() -> Option<&'static str> {
    match std::env::consts::ARCH {
        "x86_64" => Some("equinox-x64.zip"),
        "x86" => Some("equinox-x86.zip"),
        _ => None,
    }
}

/// The version this build reports, which `EQUINOX_VERSION` can override to
/// rehearse an update without waiting for a real one.
pub fn current_version() -> String {
    std::env::var("EQUINOX_VERSION").unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string())
}

/// Latest release if it is newer than this build; `None` when up to date.
pub fn check() -> Result<Option<Release>, String> {
    let resp = match ureq::get(API)
        .set("User-Agent", UA)
        .set("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(20))
        .call()
    {
        Ok(r) => r,
        // Nothing published yet.
        Err(ureq::Error::Status(404, _)) => return Ok(None),
        Err(ureq::Error::Status(code, _)) => return Err(format!("GitHub answered HTTP {code}")),
        Err(e) => return Err(format!("couldn't reach GitHub ({e})")),
    };
    let rel: ApiRelease =
        serde_json::from_reader(resp.into_reader().take(4 << 20)).map_err(|e| format!("unexpected answer from GitHub ({e})"))?;
    if rel.draft || rel.prerelease {
        return Ok(None);
    }
    let version = rel.tag_name.trim_start_matches(['v', 'V']).to_string();
    if !is_newer(&version, &current_version()) {
        return Ok(None);
    }
    // A release still being built has no zip for this machine yet.
    let Some(asset) = asset_name().and_then(|want| rel.assets.into_iter().find(|a| a.name.eq_ignore_ascii_case(want))) else {
        return Ok(None);
    };
    Ok(Some(Release {
        version,
        url: asset.browser_download_url,
        size: asset.size,
        sha256: asset.digest.and_then(|d| d.strip_prefix("sha256:").map(str::to_ascii_lowercase)),
    }))
}

fn new_path(exe: &Path) -> PathBuf {
    exe.with_extension("exe.new")
}

fn old_path(exe: &Path) -> PathBuf {
    exe.with_extension("exe.old")
}

/// Download, verify and swap in the new programs. `progress` gets 0..1.
/// Returns the app to start again.
pub fn install(r: &Release, progress: impl Fn(f32)) -> Result<PathBuf, String> {
    let app = crate::paths::app_exe();
    let dir = app.parent().ok_or("can't tell where equinox is installed")?.to_path_buf();

    let resp = ureq::get(&r.url)
        .set("User-Agent", UA)
        .timeout(Duration::from_secs(600))
        .call()
        .map_err(|e| format!("download failed ({e})"))?;
    let total = resp.header("Content-Length").and_then(|v| v.parse::<u64>().ok()).unwrap_or(r.size);
    let mut body = resp.into_reader().take(256 << 20);
    let mut zip = Vec::with_capacity(total as usize);
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = body.read(&mut buf).map_err(|e| format!("download failed ({e})"))?;
        if n == 0 {
            break;
        }
        zip.extend_from_slice(&buf[..n]);
        if total > 0 {
            progress((zip.len() as f32 / total as f32).min(1.0));
        }
    }
    if r.size > 0 && zip.len() as u64 != r.size {
        return Err("the download was incomplete".into());
    }
    let digest: String = Sha256::digest(&zip).iter().map(|b| format!("{b:02x}")).collect();
    if r.sha256.as_ref().is_some_and(|want| *want != digest) {
        return Err("the download didn't match its checksum, so it was discarded".into());
    }

    // Unpack everything first: nothing is replaced unless the whole zip is good.
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip)).map_err(|e| format!("the update isn't a valid zip ({e})"))?;
    let mut staged = Vec::new();
    for name in EXES {
        let mut bytes = Vec::new();
        match archive.by_name(name) {
            Ok(mut f) => {
                f.read_to_end(&mut bytes).map_err(|e| format!("couldn't unpack {name} ({e})"))?;
            }
            // The command line is optional; the app is not.
            Err(_) if name != EXES[0] => continue,
            Err(e) => return Err(format!("the update has no {name} ({e})")),
        }
        if !bytes.starts_with(b"MZ") {
            return Err(format!("{name} in the update isn't a Windows program, so it was discarded"));
        }
        let target = dir.join(name);
        std::fs::write(new_path(&target), &bytes)
            .map_err(|e| format!("can't write to {} ({e}). Move equinox to a folder you own.", dir.display()))?;
        staged.push(target);
    }

    // The tray agent runs from the same exe and rewrites the config; stop it
    // so the restarted app brings up the new one.
    crate::sys::stop_agent();
    for target in &staged {
        let (new, old) = (new_path(target), old_path(target));
        let _ = std::fs::remove_file(&old);
        if target.exists() {
            std::fs::rename(target, &old).map_err(|e| format!("can't replace {} ({e})", target.display()))?;
        }
        if let Err(e) = std::fs::rename(&new, target) {
            let _ = std::fs::rename(&old, target);
            return Err(format!("can't replace {} ({e})", target.display()));
        }
    }
    Ok(app)
}

/// Start the updated app; the caller then exits.
pub fn relaunch(exe: &Path) -> Result<(), String> {
    std::process::Command::new(exe)
        .arg(RESTART_FLAG)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Remove leftovers from a previous update.
pub fn cleanup() {
    let app = crate::paths::app_exe();
    for name in EXES {
        let exe = app.with_file_name(name);
        let _ = std::fs::remove_file(old_path(&exe));
        let _ = std::fs::remove_file(new_path(&exe));
    }
}

/// Numeric dotted-version comparison: 1.10.0 > 1.9.2.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> {
        v.split('-')
            .next()
            .unwrap_or("")
            .split('.')
            .map(|p| p.parse().unwrap_or(0))
            .collect()
    };
    let (a, b) = (parts(candidate), parts(current));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn versions() {
        assert!(is_newer("0.2.0", "0.1.9"));
        assert!(is_newer("1.10.0", "1.9.0"));
        assert!(is_newer("2", "1.9.9"));
        assert!(!is_newer("0.2.0", "0.2.0"));
        assert!(!is_newer("0.2", "0.2.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
    }
}
