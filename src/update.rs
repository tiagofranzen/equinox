//! Self-update from GitHub Releases (github.com/visiuun/equinox).
//!
//! A release carries the programs themselves: `equinox.exe` and `eqx.exe`
//! (and `-x86` builds for 32-bit Windows). The ones for this machine are
//! downloaded next to the running program and checked against the size and
//! SHA-256 GitHub publishes. Each is then swapped by renaming: the running
//! exe becomes `.old` (Windows allows renaming a running image), the new one
//! takes its name, and the app is started again. The `.old` files are deleted
//! on the next start.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use sha2::{Digest, Sha256};

pub const RELEASES_PAGE: &str = "https://github.com/visiuun/equinox/releases";
const API: &str = "https://api.github.com/repos/visiuun/equinox/releases/latest";
const UA: &str = concat!("equinox/", env!("CARGO_PKG_VERSION"));
/// The app, which every release must have, then the command line.
const APP: &str = "equinox.exe";
const CLI: &str = "eqx.exe";
/// Passed to the restarted app, which then waits for this one to exit.
pub const RESTART_FLAG: &str = "--updated";

/// One program to fetch: the file it replaces, and where the new one is.
#[derive(Clone, Debug, PartialEq)]
pub struct Download {
    pub file: &'static str,
    pub url: String,
    pub size: u64,
    pub sha256: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: String,
    pub files: Vec<Download>,
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

/// What a program is called in a release for this build's architecture.
fn asset_name(file: &str) -> Option<String> {
    match std::env::consts::ARCH {
        "x86_64" => Some(file.to_string()),
        "x86" => Some(file.replace(".exe", "-x86.exe")),
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
    let files = pick(&rel.assets);
    // A release still being built has no app for this machine yet.
    if !files.iter().any(|f| f.file == APP) {
        return Ok(None);
    }
    Ok(Some(Release { version, files }))
}

/// The downloads in a release that belong to this build's architecture.
fn pick(assets: &[ApiAsset]) -> Vec<Download> {
    [APP, CLI]
        .into_iter()
        .filter_map(|file| {
            let want = asset_name(file)?;
            let a = assets.iter().find(|a| a.name.eq_ignore_ascii_case(&want))?;
            Some(Download {
                file,
                url: a.browser_download_url.clone(),
                size: a.size,
                sha256: a
                    .digest
                    .as_deref()
                    .and_then(|d| d.strip_prefix("sha256:"))
                    .map(str::to_ascii_lowercase),
            })
        })
        .collect()
}

fn new_path(exe: &Path) -> PathBuf {
    exe.with_extension("exe.new")
}

fn old_path(exe: &Path) -> PathBuf {
    exe.with_extension("exe.old")
}

/// Fetch one program to `<target>.new` and check it. `got` is called with each chunk's size.
fn download(d: &Download, target: &Path, got: &mut dyn FnMut(u64)) -> Result<(), String> {
    let resp = ureq::get(&d.url)
        .set("User-Agent", UA)
        .timeout(Duration::from_secs(600))
        .call()
        .map_err(|e| format!("download failed ({e})"))?;
    let mut body = resp.into_reader().take(256 << 20);
    let mut file = std::fs::File::create(new_path(target)).map_err(|e| {
        format!(
            "can't write to {} ({e}). Move equinox to a folder you own.",
            target.parent().unwrap_or(target).display()
        )
    })?;
    let mut hash = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let (mut read, mut head) = (0u64, [0u8; 2]);
    loop {
        let n = body.read(&mut buf).map_err(|e| format!("download failed ({e})"))?;
        if n == 0 {
            break;
        }
        if read == 0 && n >= 2 {
            head.copy_from_slice(&buf[..2]);
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        hash.update(&buf[..n]);
        read += n as u64;
        got(n as u64);
    }
    file.sync_all().map_err(|e| e.to_string())?;
    if d.size > 0 && read != d.size {
        return Err(format!("the download of {} was incomplete", d.file));
    }
    let digest: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if d.sha256.as_ref().is_some_and(|want| *want != digest) {
        return Err(format!("{} didn't match its checksum, so the update was discarded", d.file));
    }
    if &head != b"MZ" {
        return Err(format!("{} isn't a Windows program, so the update was discarded", d.file));
    }
    Ok(())
}

/// Download, verify and swap in the new programs. `progress` gets 0..1.
/// Returns the app to start again.
pub fn install(r: &Release, progress: impl Fn(f32)) -> Result<PathBuf, String> {
    let app = crate::paths::app_exe();
    let targets: Vec<PathBuf> = r.files.iter().map(|d| app.with_file_name(d.file)).collect();
    let total: u64 = r.files.iter().map(|d| d.size).sum();
    let mut done = 0u64;

    // Fetch everything first: nothing is replaced unless every download is good.
    for (d, target) in r.files.iter().zip(&targets) {
        let fetched = download(d, target, &mut |n| {
            done += n;
            if total > 0 {
                progress((done as f32 / total as f32).min(1.0));
            }
        });
        if let Err(e) = fetched {
            for t in &targets {
                let _ = std::fs::remove_file(new_path(t));
            }
            return Err(e);
        }
    }

    // The tray agent runs from the same exe and rewrites the config; stop it
    // so the restarted app brings up the new one.
    crate::sys::stop_agent();
    for target in &targets {
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
    for name in [APP, CLI] {
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
    use super::*;

    #[test]
    fn versions() {
        assert!(is_newer("0.2.0", "0.1.9"));
        assert!(is_newer("1.10.0", "1.9.0"));
        assert!(is_newer("2", "1.9.9"));
        assert!(!is_newer("0.2.0", "0.2.0"));
        assert!(!is_newer("0.2", "0.2.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
    }

    #[test]
    fn picks_this_architectures_programs() {
        let asset = |name: &str| ApiAsset {
            name: name.into(),
            browser_download_url: format!("https://example.invalid/{name}"),
            size: 1,
            digest: Some("sha256:ABCD".into()),
        };
        let assets = [
            asset("equinox.exe"),
            asset("eqx.exe"),
            asset("equinox-x86.exe"),
            asset("eqx-x86.exe"),
        ];
        let files = pick(&assets);
        // Both programs, saved under their plain names whatever the release calls them.
        assert_eq!(files.iter().map(|f| f.file).collect::<Vec<_>>(), [APP, CLI]);
        let suffix = if cfg!(target_arch = "x86") { "-x86.exe" } else { ".exe" };
        assert!(
            files
                .iter()
                .all(|f| f.url.ends_with(suffix) && f.url.contains("x86") == cfg!(target_arch = "x86"))
        );
        assert_eq!(files[0].sha256.as_deref(), Some("abcd"));
        // The command line is optional, the app is not.
        assert_eq!(pick(&[asset("equinox.exe"), asset("equinox-x86.exe")]).len(), 1);
        assert!(pick(&[asset("notes.txt")]).is_empty());
    }
}
