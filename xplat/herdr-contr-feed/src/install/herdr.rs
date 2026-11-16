//! The Herdr app part of `install`: the layouts, themes and mods in the home, the keys merge and the
//! `herdr-app check` call.

use indexmap::IndexMap;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use super::files::{rel_of, sha, sha_file, stop, walk_files};
pub use super::embedded::{FILES, KEYS, KNOWN};

/// The mods that install puts in <home>/mods: herdr-contr, and our copies of Herdr's spaces and tabs mods.
pub const MODS: [&str; 3] = ["herdr-contr", "herdr-contr-spaces", "herdr-contr-tabs"];

/// Mods that an earlier install put there and this version does not have (#98: the Projects list draws the
/// agents). Their unchanged files go, and the folder goes when only files that herdr-app writes stay.
pub const RETIRED: [&str; 1] = ["herdr-contr-agents"];

/// Whether `rel` is a file in one of our mods, or in a mod we retired.
pub(super) fn in_our_mod(rel: &str) -> bool {
    MODS.iter().chain(RETIRED.iter()).any(|m| rel.strip_prefix("mods/").and_then(|r| r.strip_prefix(m)).is_some_and(|r| r.starts_with('/')))
}

/// A file that herdr-app writes at a mod's top level (see .gitignore), or a Finder file.
fn generated(name: &str) -> bool {
    name == "jsconfig.json" || name.ends_with(".d.ts") || name == ".DS_Store"
}

/// Removes a retired mod's folder when only generated files stay in it. A file of the person's keeps it.
pub(super) fn remove_retired(home: &Path) -> std::io::Result<()> {
    for m in RETIRED {
        let dir = home.join("mods").join(m);
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        let names: Vec<_> = entries.filter_map(|e| e.ok()).map(|e| (e.file_type().map(|t| t.is_file()).unwrap_or(false), e.file_name().to_string_lossy().into_owned())).collect();
        if names.iter().all(|(file, n)| *file && generated(n)) {
            fs::remove_dir_all(&dir)?;
        }
    }
    Ok(())
}


pub(super) type Keys = IndexMap<String, IndexMap<String, Value>>;

/// The known-files table: path in the home -> (sha256 -> the herdr-contr version that had it).
pub type Known = BTreeMap<String, BTreeMap<String, String>>;

/// Reads `<sha256>  <version>  <path>` lines (HERDR_CONTR_KNOWN_FILES at build time); `#` lines and bad lines are skipped.
pub fn known_files(text: &[u8]) -> Known {
    let mut k = Known::new();
    for line in String::from_utf8_lossy(text).lines() {
        let mut it = line.split_whitespace();
        if let (Some(h), Some(v), Some(p), None) = (it.next(), it.next(), it.next(), it.next()) {
            if h.len() == 64 && h.bytes().all(|c| c.is_ascii_hexdigit()) && !line.starts_with('#') {
                k.entry(p.to_string()).or_default().insert(h.to_ascii_lowercase(), v.to_string());
            }
        }
    }
    k
}

/// What the install finds in a home before it writes.
#[derive(Debug, Default, PartialEq)]
pub struct Sorted {
    /// The person's files (or files they changed): the install stops.
    pub clash: BTreeSet<String>,
    /// Files of an older herdr-contr that this version replaces: path -> that version.
    pub older: BTreeMap<String, String>,
    /// Files of an older herdr-contr that this version no longer has, removed: path -> that version.
    pub gone: BTreeMap<String, String>,
}

/// A path a herdr-contr install owns: only these are ever replaced or removed as an older herdr-contr's.
fn own_path(rel: &str) -> bool {
    !rel.split('/').any(|c| c == ".." || c.is_empty())
        && (rel == "layouts/herdr-contr-own.json" || rel == "layouts/herdr-contr.json" || in_our_mod(rel) || (rel.starts_with("themes/") && rel.matches('/').count() == 1))
}

/// Sorts what is in `home` against this version's `files`, the install.json of an earlier install (`mine`) and
/// the files of every earlier version (`known`). A file is ours when it is missing, already the same, what
/// install.json says we put there, or byte for byte a file of an older herdr-contr.
pub fn sort_files(home: &Path, files: &BTreeMap<&str, &[u8]>, mine: &BTreeMap<String, String>, known: &Known) -> Sorted {
    let mut s = Sorted::default();
    let known_as = |rel: &str, h: &str| known.get(rel).and_then(|m| m.get(h)).filter(|_| own_path(rel)).cloned();
    for (rel, bytes) in files {
        let dest = home.join(rel);
        if !dest.exists() {
            continue;
        }
        match if dest.is_file() { sha_file(&dest) } else { None } {
            None => {
                s.clash.insert(rel.to_string());
            }
            Some(h) if h == sha(bytes) || mine.get(*rel) == Some(&h) => {}
            Some(h) => match known_as(rel, &h) {
                Some(v) => {
                    s.older.insert(rel.to_string(), v);
                }
                None => {
                    s.clash.insert(rel.to_string());
                }
            },
        }
    }
    // Files in the person's copies of our mods that this version does not have: an older herdr-contr's, or theirs.
    for m in MODS {
        let mod_dest = home.join("mods").join(m);
        if mod_dest.is_dir() {
            let mut found = Vec::new();
            walk_files(&mod_dest, &mut found);
            for p in found {
                let rel = rel_of(&p, home);
                let n = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                // herdr-app writes jsconfig.json and *.d.ts at the mod's top level (see .gitignore).
                let made = p.parent() == Some(mod_dest.as_path()) && generated(&n);
                if files.contains_key(rel.as_str()) || mine.contains_key(&rel) || n == ".DS_Store" || made {
                    continue;
                }
                match sha_file(&p).and_then(|h| known_as(&rel, &h)) {
                    Some(v) => {
                        s.gone.insert(rel, v);
                    }
                    None => {
                        s.clash.insert(rel);
                    }
                }
            }
        } else if mod_dest.exists() {
            s.clash.insert(format!("mods/{m}"));
        }
    }
    // A theme or layout of an older herdr-contr that this version dropped: removed only when unchanged.
    for (rel, sums) in known {
        if rel.starts_with("mods/") || files.contains_key(rel.as_str()) || mine.contains_key(rel) || !own_path(rel) {
            continue;
        }
        let dest = home.join(rel);
        if let Some(v) = dest.is_file().then(|| sha_file(&dest)).flatten().and_then(|h| sums.get(&h).cloned()) {
            s.gone.insert(rel.clone(), v);
        }
    }
    s
}

/// A key the person already set to another command: (section, key, their command, ours).
pub(super) type Kept = (String, String, Value, Value);

/// Keys: add our sections and keys; a key the person already set to another command stays theirs.
/// Gives (the merged keys, the keys before, the keys kept), or the STOP line.
pub(super) fn merge_keys(keys_path: &Path) -> Result<(Keys, Keys, Vec<Kept>), String> {
    let ours: Keys = serde_json::from_slice(KEYS).expect("herdr-app/keys.json");
    let mut keys: Keys = match fs::read(keys_path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Keys::new(),
        Err(e) => return Err(format!("STOP: cannot read {}: {e}. Nothing was written.", keys_path.display())),
        Ok(b) => match serde_json::from_slice::<Value>(&b) {
            Err(_) => return Err(format!("STOP: {} is not valid JSON. Nothing was written.", keys_path.display())),
            Ok(v) => match serde_json::from_value::<Keys>(v) {
                Ok(k) => k,
                Err(_) => return Err(format!("STOP: {} is not sections of key = command. Nothing was written.", keys_path.display())),
            },
        },
    };
    let old_keys = keys.clone();
    let mut kept = Vec::new();
    for (section, binds) in &ours {
        let have = keys.entry(section.clone()).or_default();
        for (key, cmd) in binds {
            match have.get(key) {
                Some(theirs) if theirs != cmd => kept.push((section.clone(), key.clone(), theirs.clone(), cmd.clone())),
                _ => {
                    have.insert(key.clone(), cmd.clone());
                }
            }
        }
    }
    Ok((keys, old_keys, kept))
}

/// Runs `herdr-app check` on each of our mods in `home`, and gives the exit code of the install.
pub(super) fn check(home: &Path) -> i32 {
    let mut status = Ok(std::process::ExitStatus::default());
    for m in MODS {
        status = std::process::Command::new("herdr-app").arg("check").arg(home.join("mods").join(m)).env("HERDR_APP_HOME", home).status();
        if !status.as_ref().is_ok_and(|s| s.success()) {
            break;
        }
    }
    match status {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            println!("Installed, but herdr-app is not on PATH, so herdr-app check did not run.");
            0
        }
        Err(e) => stop(&[format!("herdr-app check did not run: {e}. The files are installed.")]),
        Ok(s) if !s.success() => stop(&["herdr-app check found a problem (see above). The files are installed.".into()]),
        Ok(_) => {
            println!("Installed. In herdr-app run: layout.switch {{\"name\": \"herdr-contr\"}}");
            0
        }
    }
}
