//! `herdr-contr-feed install`: the port of herdr-app/install.sh, with the mod built into the helper.
//! Plain text for a person, and an exit code (0 done, 1 stopped, 2 bad arguments); not JSON Lines.

use indexmap::IndexMap;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/embedded.rs"));
}
pub use embedded::{FILES, KEYS, KIT, KNOWN};

/// The marketplace file of the kit: the repository's, which names the plugin at `./kit`. Claude Code registers
/// the kit through it (`claude plugin marketplace add <home>/herdr-contr`).
pub const MARKETPLACE: &[u8] = include_bytes!("../../../.claude-plugin/marketplace.json");

/// The mods that install puts in <home>/mods: herdr-contr, and our copies of Herdr's spaces and tabs mods.
pub const MODS: [&str; 3] = ["herdr-contr", "herdr-contr-spaces", "herdr-contr-tabs"];

/// Mods that an earlier install put there and this version does not have (#98: the Projects list draws the
/// agents). Their unchanged files go, and the folder goes when only files that herdr-app writes stay.
pub const RETIRED: [&str; 1] = ["herdr-contr-agents"];

/// Whether `rel` is a file in one of our mods, or in a mod we retired.
fn in_our_mod(rel: &str) -> bool {
    MODS.iter().chain(RETIRED.iter()).any(|m| rel.strip_prefix("mods/").and_then(|r| r.strip_prefix(m)).is_some_and(|r| r.starts_with('/')))
}

/// A file that herdr-app writes at a mod's top level (see .gitignore), or a Finder file.
fn generated(name: &str) -> bool {
    name == "jsconfig.json" || name.ends_with(".d.ts") || name == ".DS_Store"
}

/// Removes a retired mod's folder when only generated files stay in it. A file of the person's keeps it.
fn remove_retired(home: &Path) -> std::io::Result<()> {
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

const HELP: &str = "\
Installs the herdr-contr layout, mod, themes, keys and helper into a herdr-app home.
Usage: herdr-contr-feed install [--home <herdr-app home>] [--helper <herdr-contr-feed>] [--orch <herdr-contr-orch>]
                                [--remote <folder>] [--dry-run]
 - The home is --home, else $HERDR_APP_HOME, else ~/.config/herdr-app. With no HOME, give --home.
 - A keys.json, layout or theme that is a symlink stays one; the file it points to is written.
 - It never overwrites what you have: if a layout, mod or theme with the same name is there and
   this did not put it there (or you changed it since), it stops and writes nothing.
 - keys.json is merged: your sections and keys stay; a key you already use keeps your command.
 - --helper copies herdr-contr-feed to <home>/herdr-contr/bin/. Without it, a helper already there
   stays; with none there, it says where to get one.
 - The kit (the Claude Code plugin of a coordinator and its workers) goes to <home>/herdr-contr/kit/.
   That folder is this install's: a file there that this version does not have is removed.
   <home>/herdr-contr/.claude-plugin/marketplace.json names it for `claude plugin marketplace add`.
 - --orch copies the orchestration tool herdr-contr-orch to <home>/herdr-contr/bin/, where the kit's
   hooks and MCP server look for it first.
 - --remote copies the helpers for other machines (<folder>/<target>/herdr-contr-feed, checked
   against <folder>/SHA256SUMS) to <home>/herdr-contr/remote/.
 - After copying it runs `herdr-app check` on the mod. --dry-run prints the plan and writes nothing.
 - What it installed (with hashes) is kept in <home>/herdr-contr/install.json, so a later install
   can update its own files.
";

fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

fn sha_file(p: &Path) -> Option<String> {
    fs::read(p).ok().map(|b| sha(&b))
}

fn stop(lines: &[String]) -> i32 {
    for l in lines {
        eprintln!("{l}");
    }
    1
}

/// Writes through a temp file beside `dest` and a rename, so a reader never sees half a file.
/// When `dest` is a symlink (a keys.json kept in a dotfiles folder), the file it points to is written,
/// in that file's own folder, and the link stays.
fn put(dest: &Path, bytes: &[u8], exe: bool) -> std::io::Result<()> {
    let real;
    let dest = if fs::symlink_metadata(dest).is_ok_and(|m| m.file_type().is_symlink()) {
        // A link to a file that is not there yet: create it where the link points.
        real = match fs::canonicalize(dest) {
            Ok(p) => p,
            Err(_) => dest.parent().unwrap_or(Path::new(".")).join(fs::read_link(dest)?),
        };
        real.as_path()
    } else {
        dest
    };
    if let Some(d) = dest.parent() {
        fs::create_dir_all(d)?;
    }
    let mut tmp = dest.as_os_str().to_owned();
    tmp.push(".herdr-contr-new");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, bytes)?;
    #[cfg(unix)]
    if exe {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755))?;
    }
    #[cfg(not(unix))]
    let _ = exe;
    fs::rename(&tmp, dest)
}

fn rel_of(p: &Path, base: &Path) -> String {
    p.strip_prefix(base).unwrap_or(p).components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/")
}

fn walk_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk_files(&p, out);
        } else {
            out.push(p);
        }
    }
}

/// One helper for another machine: its target name and the bytes, already checked against SHA256SUMS.
struct Remote {
    target: String,
    bytes: Vec<u8>,
}

fn valid_target(t: &str) -> bool {
    !t.is_empty() && !t.starts_with('.') && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
}

/// Reads <dir>/SHA256SUMS (`<sum>  <target>/herdr-contr-feed` per line) and checks every helper it names.
fn read_remote(dir: &Path) -> Result<(Vec<Remote>, Vec<u8>), String> {
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

type Keys = IndexMap<String, IndexMap<String, Value>>;

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

pub fn run(args: &[String]) -> i32 {
    let mut home: Option<String> = None;
    let mut helper: Option<String> = None;
    let mut orch: Option<String> = None;
    let mut remote: Option<String> = None;
    let mut dry = false;
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        let value = |what: &str| -> Result<String, i32> {
            args.get(i + 1).cloned().ok_or_else(|| {
                eprintln!("{a} needs {what}");
                2
            })
        };
        match a {
            "--home" => match value("a folder") {
                Ok(v) => home = Some(v),
                Err(c) => return c,
            },
            "--helper" => match value("a file") {
                Ok(v) => helper = Some(v),
                Err(c) => return c,
            },
            "--orch" => match value("a file") {
                Ok(v) => orch = Some(v),
                Err(c) => return c,
            },
            "--remote" => match value("a folder") {
                Ok(v) => remote = Some(v),
                Err(c) => return c,
            },
            "--dry-run" => {
                dry = true;
                i += 1;
                continue;
            }
            "-h" | "--help" => {
                print!("{HELP}");
                return 0;
            }
            _ => {
                eprintln!("unknown argument: {a} (see --help)");
                return 2;
            }
        }
        i += 2;
    }
    let nonempty = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let home = match home.or_else(|| nonempty("HERDR_APP_HOME")) {
        Some(h) => PathBuf::from(h),
        None => match nonempty("HOME").or_else(|| nonempty("USERPROFILE")) {
            Some(h) => Path::new(&h).join(".config").join("herdr-app"),
            None => return stop(&["STOP: HOME is not set, so there is no ~/.config/herdr-app. Name the folder with --home <herdr-app home>. Nothing was written.".into()]),
        },
    };
    if let Some(h) = &helper {
        if !Path::new(h).is_file() {
            return stop(&[format!("STOP: no helper at {h}")]);
        }
    }
    if let Some(o) = &orch {
        if !Path::new(o).is_file() {
            return stop(&[format!("STOP: no orchestration tool at {o}")]);
        }
    }
    let marker_path = home.join("herdr-contr").join("install.json");
    let files: BTreeMap<&str, &[u8]> = FILES.iter().copied().collect();

    let mine: BTreeMap<String, String> = fs::read(&marker_path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|v| v.get("files").and_then(|f| f.as_object()).cloned())
        .map(|m| m.into_iter().filter_map(|(k, v)| v.as_str().map(|s| (k, s.to_string()))).collect())
        .unwrap_or_default();

    let known = known_files(KNOWN);
    let Sorted { clash, older, gone } = sort_files(&home, &files, &mine, &known);
    if !clash.is_empty() {
        let mut lines = vec![format!("STOP: these are already in {} and were not installed by this script (or you changed them):", home.display())];
        lines.extend(clash.iter().map(|r| format!("  {r}")));
        lines.push("Nothing was written. Move or rename them, then run this again.".into());
        return stop(&lines);
    }

    // Keys: add our sections and keys; a key the person already set to another command stays theirs.
    let ours: Keys = serde_json::from_slice(KEYS).expect("herdr-app/keys.json");
    let keys_path = home.join("keys.json");
    let mut keys: Keys = match fs::read(&keys_path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Keys::new(),
        Err(e) => return stop(&[format!("STOP: cannot read {}: {e}. Nothing was written.", keys_path.display())]),
        Ok(b) => match serde_json::from_slice::<Value>(&b) {
            Err(_) => return stop(&[format!("STOP: {} is not valid JSON. Nothing was written.", keys_path.display())]),
            Ok(v) => match serde_json::from_value::<Keys>(v) {
                Ok(k) => k,
                Err(_) => {
                    return stop(&[format!("STOP: {} is not sections of key = command. Nothing was written.", keys_path.display())])
                }
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

    let remote = match remote {
        None => None,
        Some(dir) => match read_remote(Path::new(&dir)) {
            Ok(r) => Some(r),
            Err(e) => return stop(&[e, "Nothing was written.".into()]),
        },
    };

    let bin_dest = home.join("herdr-contr").join("bin").join(if cfg!(windows) { "herdr-contr-feed.exe" } else { "herdr-contr-feed" });
    let orch_dest = home.join("herdr-contr").join("bin").join(if cfg!(windows) { "herdr-contr-orch.exe" } else { "herdr-contr-orch" });
    let kit_dest = home.join("herdr-contr").join("kit");
    // The kit folder is ours: what is there and not in this version goes.
    let mut kit_old = Vec::new();
    walk_files(&kit_dest, &mut kit_old);
    kit_old.retain(|p| !KIT.iter().any(|(rel, _)| kit_dest.join(rel) == *p));
    let show = |v: &Value| v.as_str().map(String::from).unwrap_or_else(|| v.to_string());
    println!("{}{}", if dry { "Dry run: would install into " } else { "Installing into " }, home.display());
    for rel in files.keys().filter(|r| !r.starts_with("mods/")) {
        println!("  {rel}");
    }
    println!("  mods/herdr-contr*/... ({} files)", files.keys().filter(|r| r.starts_with("mods/")).count());
    for (rel, v) in &older {
        println!("    {rel}: older herdr-contr ({v}), replaced");
    }
    for (rel, v) in &gone {
        println!("    {rel}: older herdr-contr ({v}), removed (this version does not have it)");
    }
    println!("  keys.json ({})", if old_keys.is_empty() { "new" } else { "merged" });
    for (section, key, theirs, cmd) in &kept {
        println!("    kept your {section} {key} = {} (herdr-contr uses it for {})", show(theirs), show(cmd));
    }
    if let Some(h) = &helper {
        println!("  herdr-contr/bin/herdr-contr-feed (from {h})");
    }
    println!("  herdr-contr/kit/... ({} files)", KIT.len());
    println!("  herdr-contr/.claude-plugin/marketplace.json");
    for p in &kit_old {
        println!("    {}: not in this version, removed", rel_of(p, &home));
    }
    if let Some(o) = &orch {
        println!("  herdr-contr/bin/herdr-contr-orch (from {o})");
    }
    if let Some((list, _)) = &remote {
        for r in list {
            println!("  herdr-contr/remote/{}/herdr-contr-feed", r.target);
        }
        println!("  herdr-contr/remote/SHA256SUMS");
    }
    if dry {
        println!("Dry run: nothing was written; herdr-app check did not run.");
        return 0;
    }

    let written = (|| -> std::io::Result<()> {
        for (rel, bytes) in &files {
            put(&home.join(rel), bytes, false)?;
        }
        // Files of an older herdr-contr that this version no longer has, if still byte for byte that version's.
        for rel in gone.keys() {
            let dest = home.join(rel);
            if sha_file(&dest).is_some_and(|h| known.get(rel).is_some_and(|m| m.contains_key(&h))) {
                fs::remove_file(&dest)?;
            }
        }
        // Files an earlier install put there that this version no longer has, if still unchanged.
        for (rel, h) in &mine {
            let dest = home.join(rel);
            let own = (in_our_mod(rel) || rel.starts_with("themes/herdr-contr")) && !rel.split('/').any(|c| c == "..");
            if own && !files.contains_key(rel.as_str()) && dest.is_file() && sha_file(&dest).as_ref() == Some(h) {
                fs::remove_file(&dest)?;
            }
        }
        remove_retired(&home)?;
        if keys != old_keys {
            put(&keys_path, (serde_json::to_string_pretty(&keys).unwrap() + "\n").as_bytes(), false)?;
        }
        if let Some(h) = &helper {
            put(&bin_dest, &fs::read(h)?, true)?;
        }
        for (rel, bytes) in KIT {
            put(&kit_dest.join(rel), bytes, false)?;
        }
        put(&home.join("herdr-contr").join(".claude-plugin").join("marketplace.json"), MARKETPLACE, false)?;
        for p in &kit_old {
            fs::remove_file(p)?;
        }
        if let Some(o) = &orch {
            put(&orch_dest, &fs::read(o)?, true)?;
        }
        if let Some((list, sums)) = &remote {
            let dir = home.join("herdr-contr").join("remote");
            for r in list {
                put(&dir.join(&r.target).join("herdr-contr-feed"), &r.bytes, true)?;
            }
            put(&dir.join("SHA256SUMS"), sums, false)?;
        }
        let marker: BTreeMap<&str, String> = files.keys().map(|rel| (*rel, sha_file(&home.join(rel)).unwrap_or_default())).collect();
        put(&marker_path, (serde_json::to_string_pretty(&serde_json::json!({ "files": marker })).unwrap() + "\n").as_bytes(), false)
    })();
    if let Err(e) = written {
        return stop(&[format!("STOP: writing into {} failed: {e}", home.display())]);
    }
    // The phone page runs this helper as a service (herdr-contr settings): restart it, or it keeps the old one.
    if helper.is_some() {
        let path = bin_dest.canonicalize().unwrap_or_else(|_| bin_dest.clone());
        if let Some(m) = crate::phone::restart_after_install(&crate::phone::Env::from_process(), &path) {
            println!("{m}");
        }
    }

    if !bin_dest.is_file() {
        let me = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "herdr-contr-feed".into());
        println!("The helper herdr-contr-feed is not installed; the chat stays empty without it.");
        println!("This program is one: run this again with --helper {me}");
        println!("or build one in this repository: cargo build --release --manifest-path xplat/Cargo.toml -p herdr-contr-feed,");
        println!("then run: xplat/target/release/herdr-contr-feed install --helper xplat/target/release/herdr-contr-feed");
    }

    let _ = std::io::stdout().flush();
    let mut status = Ok(std::process::ExitStatus::default());
    for m in MODS {
        status = std::process::Command::new("herdr-app").arg("check").arg(home.join("mods").join(m)).env("HERDR_APP_HOME", &home).status();
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
