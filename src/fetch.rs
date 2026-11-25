//! Downloads with an on-disk copy: the libraries are re-checked every time the
//! app starts (a conditional GET, so an unchanged list costs one round trip),
//! and the last good copy is what you get when offline.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{Context as _, Result};

const TIMEOUT: Duration = Duration::from_secs(20);

fn etag_file(cache: &Path) -> PathBuf {
    let mut name = cache.file_name().unwrap_or_default().to_os_string();
    name.push(".etag");
    cache.with_file_name(name)
}

/// How long ago the cached copy was last confirmed current.
pub fn age(cache: &Path) -> Option<Duration> {
    std::fs::metadata(cache)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
}

fn touch(path: &Path) {
    if let Ok(f) = std::fs::File::options().append(true).open(path) {
        let _ = f.set_modified(SystemTime::now());
    }
}

/// Ask the server for `url`. `Ok(None)` means the cached copy is still current.
/// A new body is written to `cache` before it is returned.
pub fn refresh(url: &str, cache: &Path) -> Result<Option<String>> {
    let mut req = ureq::get(url).timeout(TIMEOUT);
    let etag = std::fs::read_to_string(etag_file(cache)).ok();
    if cache.exists()
        && let Some(tag) = &etag
    {
        req = req.set("If-None-Match", tag.trim());
    }
    let resp = req.call().with_context(|| format!("GET {url}"))?;
    if resp.status() == 304 {
        touch(cache);
        return Ok(None);
    }
    let tag = resp.header("ETag").map(str::to_string);
    let text = resp.into_string()?;
    if let Some(dir) = cache.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if crate::model::write_atomic(cache, text.as_bytes()).is_ok() {
        match tag {
            Some(t) => {
                let _ = std::fs::write(etag_file(cache), t);
            }
            None => {
                let _ = std::fs::remove_file(etag_file(cache));
            }
        }
    }
    Ok(Some(text))
}

/// The text behind `url`: checked against the server unless the cached copy
/// was confirmed within `max_age`; the cached copy when the network fails.
pub fn text(url: &str, cache: &Path, max_age: Duration) -> Result<String> {
    if age(cache).is_some_and(|a| a < max_age)
        && let Ok(t) = std::fs::read_to_string(cache)
    {
        return Ok(t);
    }
    match refresh(url, cache) {
        Ok(Some(t)) => Ok(t),
        Ok(None) => std::fs::read_to_string(cache).with_context(|| format!("reading {}", cache.display())),
        Err(e) => std::fs::read_to_string(cache).map_err(|_| e),
    }
}
