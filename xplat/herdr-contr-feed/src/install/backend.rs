//! The backend part of `install`: the kit, the marketplace file, the two programs and the helpers for other
//! machines, all in <home>/herdr-contr.

use std::fs;
use std::path::{Path, PathBuf};

use herdr_core::places::Places;

use super::files::{put, rel_of, sha};
pub use super::embedded::KIT;

/// The marketplace file of the kit: the repository's, which names the plugin at `./kit`. Claude Code registers
/// the kit through it (`claude plugin marketplace add <home>/herdr-contr`).
pub const MARKETPLACE: &[u8] = include_bytes!("../../../../.claude-plugin/marketplace.json");

/// One helper for another machine: its target name and the bytes, already checked against SHA256SUMS.
pub(super) struct Remote {
    target: String,
    bytes: Vec<u8>,
}

fn valid_target(t: &str) -> bool {
    !t.is_empty() && !t.starts_with('.') && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
}

/// Reads <dir>/SHA256SUMS (`<sum>  <target>/herdr-contr-feed` per line) and checks every helper it names.
pub(super) fn read_remote(dir: &Path) -> Result<(Vec<Remote>, Vec<u8>), String> {
    let sums_path = dir.join("SHA256SUMS");
    let sums = fs::read(&sums_path).map_err(|e| format!("STOP: cannot read {}: {e}", sums_path.display()))?;
    let text = String::from_utf8_lossy(&sums);
    let mut out = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let mut parts = line.split_whitespace();
        let (Some(sum), Some(path), None) = (parts.next(), parts.next(), parts.next()) else {
            return Err(format!("STOP: {} has a line I do not understand: {line}", sums_path.display()));
        };
        let path = path.trim_start_matches('*');
        let target = path.strip_suffix("/herdr-contr-feed").unwrap_or("");
        if !valid_target(target) || sum.len() != 64 || !sum.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(format!("STOP: {} has a line I do not understand: {line}", sums_path.display()));
        }
        if out.iter().any(|r: &Remote| r.target == target) {
            return Err(format!("STOP: {} names {target} twice", sums_path.display()));
        }
        let file = dir.join(target).join("herdr-contr-feed");
        let bytes = fs::read(&file).map_err(|e| format!("STOP: the helper for {target} is missing ({}: {e})", file.display()))?;
        if sha(&bytes) != sum.to_ascii_lowercase() {
            return Err(format!("STOP: the helper for {target} ({}) does not match SHA256SUMS", file.display()));
        }
        out.push(Remote { target: target.to_string(), bytes });
    }
    if out.is_empty() {
        return Err(format!("STOP: {} names no helpers", sums_path.display()));
    }
    Ok((out, sums))
}

/// The files of the orchestration tool that the contr app kept in the user config folder (macOS:
/// ~/Library/Application Support/contr; else $XDG_CONFIG_HOME/contr or ~/.config/contr) and that have no copy
/// in the Herdr home yet, as (the new place, the old file). The tool reads them in <home>/herdr-contr now (#160).
pub(super) fn old_orch_files(home: &Path, env: &dyn Fn(&str) -> Option<String>, macos: bool) -> Vec<(PathBuf, PathBuf)> {
    let old = if macos {
        env("HOME").map(|h| Path::new(&h).join("Library/Application Support/contr"))
    } else {
        env("XDG_CONFIG_HOME").map(PathBuf::from).or_else(|| env("HOME").map(|h| Path::new(&h).join(".config"))).map(|d| d.join("contr"))
    };
    let Some(old) = old else { return Vec::new() };
    let places = Places::new(home);
    [places.agents(), places.owner()]
        .into_iter()
        .filter_map(|to| Some((old.join(to.file_name()?), to)))
        .filter(|(from, to)| from.is_file() && !to.exists())
        .map(|(from, to)| (to, from))
        .collect()
}

/// What the install puts in <home>/herdr-contr: the programs given, the helpers for other machines, the files
/// copied from the contr app (`moved`) and the kit files this version does not have (`kit_old`).
pub(super) struct Backend<'a> {
    pub helper: Option<&'a str>,
    pub orch: Option<&'a str>,
    pub remote: Option<(Vec<Remote>, Vec<u8>)>,
    pub moved: Vec<(PathBuf, PathBuf)>,
    pub kit_old: Vec<PathBuf>,
}

impl Backend<'_> {
    /// The plan lines of the backend part. `base`: the folder that the lines are relative to; `at`: the root
    /// relative to it (`herdr-contr/` in a Herdr home, empty for the backend install).
    pub(super) fn plan(&self, base: &Path, at: &str) {
        if let Some(h) = self.helper {
            println!("  {at}bin/herdr-contr-feed (from {h})");
        }
        println!("  {at}kit/... ({} files)", KIT.len());
        println!("  {at}.claude-plugin/marketplace.json");
        for (to, from) in &self.moved {
            println!("  {} (a copy of {}; the old file stays)", rel_of(to, base), from.display());
        }
        for p in &self.kit_old {
            println!("    {}: not in this version, removed", rel_of(p, base));
        }
        if let Some(o) = self.orch {
            println!("  {at}bin/herdr-contr-orch (from {o})");
        }
        if let Some((list, _)) = &self.remote {
            for r in list {
                println!("  {at}remote/{}/herdr-contr-feed", r.target);
            }
            println!("  {at}remote/SHA256SUMS");
        }
    }

    /// The writes of the backend part.
    pub(super) fn write(&self, places: &Places) -> std::io::Result<()> {
        if let Some(h) = self.helper {
            put(&places.feed(), &fs::read(h)?, true)?;
        }
        let kit_dest = places.kit();
        for (rel, bytes) in KIT {
            put(&kit_dest.join(rel), bytes, false)?;
        }
        put(&places.marketplace(), MARKETPLACE, false)?;
        for (to, from) in &self.moved {
            put(to, &fs::read(from)?, false)?;
        }
        for p in &self.kit_old {
            fs::remove_file(p)?;
        }
        if let Some(o) = self.orch {
            put(&places.orch(), &fs::read(o)?, true)?;
        }
        if let Some((list, sums)) = &self.remote {
            let dir = places.folder().join("remote");
            for r in list {
                put(&dir.join(&r.target).join("herdr-contr-feed"), &r.bytes, true)?;
            }
            put(&dir.join("SHA256SUMS"), sums, false)?;
        }
        Ok(())
    }
}
