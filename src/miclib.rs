//! The microphone library: corrections that even out a microphone's own
//! voicing, the way AutoEq does for headphones. There is no public measured
//! database for microphones, so equinox keeps its own list (`library/mics.json`
//! in the repository). It is re-checked every time the app starts; the copy
//! built into the program is what you get before the first download.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context as _, Result};
use serde::Deserialize;

use crate::autoeq::normalize;
use crate::fetch;
use crate::model::{Band, Correction};

const URL: &str = "https://raw.githubusercontent.com/visiuun/equinox/main/library/mics.json";
const BUNDLED: &str = include_str!("../library/mics.json");

#[derive(Deserialize)]
struct RawLibrary {
    #[serde(default)]
    updated: String,
    mics: Vec<RawMic>,
}

#[derive(Deserialize)]
struct RawMic {
    name: String,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    note: String,
    #[serde(default)]
    generic: bool,
    /// Equalizer APO filter bodies: "ON PK Fc 5500 Hz Gain -2 dB Q 1.2".
    filters: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Mic {
    pub name: String,
    /// "Dynamic · XLR", "Condenser · USB"...
    pub kind: String,
    pub note: String,
    /// A typical curve for a kind of microphone, not one model.
    pub generic: bool,
    pub filters: Vec<Band>,
    norm: String,
    /// Name and aliases without spaces, for matching "hd600"-style input and device names.
    keys: Vec<String>,
}

impl Mic {
    /// Stable id stored in settings (dismissed suggestions).
    pub fn key(&self) -> String {
        format!("mic|{}", self.name)
    }

    pub fn correction(&self) -> Correction {
        Correction {
            name: self.name.clone(),
            source: if self.generic {
                "equinox mic library · generic starting point".into()
            } else {
                "equinox mic library · from the published response".into()
            },
            filters: self.filters.clone(),
            graphic: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Library {
    pub mics: Vec<Mic>,
    /// ISO date the list was last edited.
    pub updated: String,
}

pub fn parse(json: &str) -> Result<Library> {
    let raw: RawLibrary = serde_json::from_str(json).context("reading the microphone library")?;
    let mics = raw
        .mics
        .into_iter()
        .filter_map(|m| {
            let text: String = m.filters.iter().map(|f| format!("Filter: {f}\n")).collect();
            let filters = crate::eqtext::parse(&text).bands;
            if filters.is_empty() {
                return None;
            }
            let norm = normalize(&m.name);
            let mut keys = vec![norm.replace(' ', "")];
            keys.extend(m.aliases.iter().map(|a| normalize(a).replace(' ', "")).filter(|a| !a.is_empty()));
            Some(Mic {
                name: m.name,
                kind: m.kind,
                note: m.note,
                generic: m.generic,
                filters,
                norm,
                keys,
            })
        })
        .collect();
    Ok(Library {
        mics,
        updated: raw.updated,
    })
}

fn cache_file() -> PathBuf {
    crate::paths::cache_dir().join("mics.json")
}

/// The list built into this build of the program.
pub fn bundled() -> Library {
    parse(BUNDLED).unwrap_or_default()
}

/// The newest list available without the network: the downloaded copy, unless
/// this build ships a later one.
pub fn cached() -> Library {
    let built_in = bundled();
    match std::fs::read_to_string(cache_file()).ok().and_then(|t| parse(&t).ok()) {
        Some(c) if !c.mics.is_empty() && c.updated >= built_in.updated => c,
        _ => built_in,
    }
}

/// Check the repository for a newer list. `Ok(None)`: nothing newer.
pub fn refresh() -> Result<Option<Library>> {
    let Some(text) = fetch::refresh(URL, &cache_file()).map_err(|e| e.context("couldn't download the microphone library"))? else {
        return Ok(None);
    };
    let lib = parse(&text)?;
    Ok((!lib.mics.is_empty() && lib.updated >= bundled().updated).then_some(lib))
}

/// The list: re-checked unless it was confirmed within `max_age` (zero always
/// checks). Never fails: offline, it is the saved or built-in copy.
pub fn load(max_age: Duration) -> Library {
    if fetch::age(&cache_file()).is_none_or(|a| a >= max_age) {
        let _ = refresh();
    }
    cached()
}

/// How long ago the list was last confirmed current (`None`: never downloaded).
pub fn age() -> Option<Duration> {
    fetch::age(&cache_file())
}

/// Every query word must match the name or an alias (spaces and dashes are
/// optional). An empty query lists everything, models before generic curves.
pub fn search<'a>(lib: &'a Library, query: &str, limit: usize) -> Vec<&'a Mic> {
    let q = normalize(query);
    if q.is_empty() {
        let mut all: Vec<&Mic> = lib.mics.iter().collect();
        all.sort_by(|a, b| a.generic.cmp(&b.generic).then(a.name.cmp(&b.name)));
        all.truncate(limit);
        return all;
    }
    let words: Vec<&str> = q.split(' ').collect();
    let qcompact = q.replace(' ', "");
    let mut scored: Vec<(i32, &Mic)> = lib
        .mics
        .iter()
        .filter_map(|m| {
            let mut score = 0;
            for w in &words {
                if m.norm.split(' ').any(|t| t == *w) {
                    score += 30;
                } else if m.norm.split(' ').any(|t| t.starts_with(w)) {
                    score += 18;
                } else if m.keys.iter().any(|k| k.contains(w)) {
                    score += 8;
                } else {
                    return None;
                }
            }
            if m.keys.contains(&qcompact) {
                score += 120;
            } else if m.keys.iter().any(|k| k.starts_with(&qcompact)) {
                score += 40;
            } else if m.keys.iter().any(|k| k.contains(&qcompact)) {
                score += 20;
            }
            score -= (m.keys[0].len() as i32 - qcompact.len() as i32).max(0) / 3;
            if m.generic {
                score -= 25;
            }
            Some((score, m))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.name.cmp(&b.1.name)));
    scored.into_iter().take(limit).map(|(_, m)| m).collect()
}

/// Guess the microphone model from a device's name: USB microphones report
/// theirs ("Microphone (HyperX SoloCast)"). The longest name or alias found in
/// the device text wins; generic curves are never suggested.
pub fn suggest<'a>(lib: &'a Library, device_text: &str) -> Option<&'a Mic> {
    let text = normalize(device_text).replace(' ', "");
    lib.mics
        .iter()
        .filter(|m| !m.generic)
        .filter_map(|m| {
            m.keys
                .iter()
                .filter(|k| k.len() >= 4 && text.contains(k.as_str()))
                .map(|k| k.len())
                .max()
                .map(|len| (len, m))
        })
        .max_by_key(|(len, _)| *len)
        .map(|(_, m)| m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_library_is_sound() {
        let lib = bundled();
        assert!(lib.mics.len() >= 40, "{} microphones", lib.mics.len());
        assert!(!lib.updated.is_empty());
        let raw: RawLibrary = serde_json::from_str(BUNDLED).unwrap();
        assert_eq!(raw.mics.len(), lib.mics.len(), "an entry has no usable filters");
        for (m, r) in lib.mics.iter().zip(&raw.mics) {
            assert_eq!(m.filters.len(), r.filters.len(), "{}: a filter line didn't parse", m.name);
            // Corrections are gentle by design: nothing here should be a big swing.
            for b in &m.filters {
                assert!(b.gain.abs() <= 6.0, "{}: {} dB", m.name, b.gain);
                assert!((20.0..=20_000.0).contains(&b.freq), "{}: {} Hz", m.name, b.freq);
            }
        }
        let mut names: Vec<&str> = lib.mics.iter().map(|m| m.name.as_str()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), lib.mics.len(), "duplicate names");
    }

    #[test]
    fn searches() {
        let lib = bundled();
        assert_eq!(search(&lib, "sm7b", 5)[0].name, "Shure SM7B");
        assert_eq!(search(&lib, "at 2020", 5)[0].name, "Audio-Technica AT2020");
        assert_eq!(search(&lib, "yeti", 5)[0].name, "Blue Yeti");
        assert_eq!(search(&lib, "quadcast s", 5)[0].name, "HyperX QuadCast");
        assert!(search(&lib, "zzzz", 5).is_empty());
        // Empty query: everything, generic curves last.
        let all = search(&lib, "", 500);
        assert_eq!(all.len(), lib.mics.len());
        assert!(!all[0].generic && all.last().unwrap().generic);
    }

    #[test]
    fn suggests_from_device_name() {
        let lib = bundled();
        let s = |t: &str| suggest(&lib, t).map(|m| m.name.as_str());
        assert_eq!(s("Microphone (HyperX SoloCast)"), Some("HyperX SoloCast"));
        assert_eq!(s("Microphone (Yeti Stereo Microphone)"), Some("Blue Yeti"));
        assert_eq!(s("Microphone (Yeti Nano)"), Some("Blue Yeti Nano"));
        assert_eq!(s("Microphone (Yeti X)"), Some("Blue Yeti X"));
        assert_eq!(s("Microphone (AT2020USB+)"), Some("Audio-Technica AT2020"));
        assert_eq!(s("Microphone (RODE NT-USB Mini)"), Some("Rode NT-USB Mini"));
        assert_eq!(s("Microphone (Elgato Wave:3)"), Some("Elgato Wave:3"));
        // Interfaces, webcams and headsets say nothing about the microphone.
        assert_eq!(s("Input 1 (Minifuse 1)"), None);
        assert_eq!(s("Webcam (USB Audio Device)"), None);
        assert_eq!(s("Headset Microphone (Oculus Virtual Audio Device)"), None);
        assert_eq!(s("Microphone (Realtek(R) Audio)"), None);
    }
}
