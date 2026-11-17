//! `herdr-contr-feed home move [--dry-run]`: moves the backend root from the Herdr home
//! (`~/.config/herdr-app/herdr-contr`) to the neutral folder (`~/.config/contr-backend`), and leaves a link at the
//! old path. The owner runs it; no install and no agent does.
//! Plain text for a person, and an exit code (0 done, 1 stopped, 2 bad arguments); not JSON Lines.

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use herdr_core::places;

const HELP: &str = "\
Moves the backend root from the Herdr home to the neutral folder.
Usage: herdr-contr-feed home move [--dry-run]
 - It moves ~/.config/herdr-app/herdr-contr to ~/.config/contr-backend (HOME, else USERPROFILE), and
   leaves a link at the old path. The mod's paths, --home and Claude Code's marketplace entry stay valid.
 - It stops and writes nothing when: the neutral folder exists and is not empty; agents.json.lock or
   owner.json.lock is held; `herdr-contr-orch agents ls --json` shows a live worker, or cannot run; the
   phone service file names the old path; the two folders are on different file systems.
 - --dry-run prints what it would do and writes nothing.
";

/// What the move looks at outside the two folders. `Probe::real` is the machine; tests give their own.
pub struct Probe<'a> {
    /// The device of a folder that exists. None: unknown.
    pub device: &'a dyn Fn(&Path) -> Option<u64>,
    /// True when another process holds the lock on this file.
    pub locked: &'a dyn Fn(&Path) -> bool,
    /// The names of the live workers, or why they cannot be read.
    pub live_workers: &'a dyn Fn(&Path) -> Result<Vec<String>, String>,
    /// The phone service file, when there is one.
    pub service: Option<PathBuf>,
}

pub fn run(args: &[String]) -> i32 {
    let mut dry = false;
    let mut sub = None;
    for a in args {
        match a.as_str() {
            "--dry-run" => dry = true,
            "-h" | "--help" => {
                print!("{HELP}");
                return 0;
            }
            "move" if sub.is_none() => sub = Some(a),
            _ => {
                eprintln!("unknown argument: {a} (see home --help)");
                return 2;
            }
        }
    }
    if sub.is_none() {
        eprintln!("home needs move (see home --help)");
        return 2;
    }
    let env = |k: &str| std::env::var(k).unwrap_or_default();
    let (Some(old), Some(new)) = (places::herdr_root(&env), places::neutral(&env)) else {
        return stop("STOP: HOME and USERPROFILE are not set, so there is no ~/.config. Nothing was written.");
    };
    let service = crate::phone::Env::from_process().file();
    let probe = Probe { device: &device, locked: &locked, live_workers: &live_workers, service: Some(service) };
    match move_root(&old, &new, &probe, dry) {
        Ok(lines) => {
            for l in lines {
                println!("{l}");
            }
            0
        }
        Err(e) => stop(&e),
    }
}

fn stop(line: &str) -> i32 {
    eprintln!("{line}");
    1
}

/// Moves `old` to `new` and links `old` to `new`; with `dry`, only says so. Ok: the lines for the owner. Err: the
/// STOP line; nothing was written.
pub fn move_root(old: &Path, new: &Path, probe: &Probe, dry: bool) -> Result<Vec<String>, String> {
    let stop = |why: String| Err(format!("STOP: {why}. Nothing was written."));
    match fs::symlink_metadata(old) {
        Err(_) => return stop(format!("there is no {}", old.display())),
        Ok(m) if m.file_type().is_symlink() => return stop(format!("{} is a link already", old.display())),
        Ok(m) if !m.is_dir() => return stop(format!("{} is not a folder", old.display())),
        Ok(_) => {}
    }
    let new_there = fs::symlink_metadata(new).is_ok();
    if new_there {
        let empty = fs::read_dir(new).map(|mut d| d.next().is_none()).unwrap_or(false);
        if !empty {
            return stop(format!("{} exists and is not empty", new.display()));
        }
    }
    for name in ["agents.json.lock", "owner.json.lock"] {
        let f = old.join(name);
        if f.exists() && (probe.locked)(&f) {
            return stop(format!("{} is held: a tool writes the files now", f.display()));
        }
    }
    match (probe.live_workers)(old) {
        Ok(live) if !live.is_empty() => return stop(format!("live workers: {}. Stop them first", live.join(", "))),
        Ok(_) => {}
        Err(e) => return stop(format!("cannot read the workers: {e}")),
    }
    if let Some(s) = &probe.service {
        if fs::read_to_string(s).is_ok_and(|t| t.contains(&*old.to_string_lossy())) {
            return stop(format!("the phone service file {} names {}. Turn the phone page off first", s.display(), old.display()));
        }
    }
    let parent = new.parent().unwrap_or(Path::new("/"));
    let target = if new_there { new } else { nearest(parent) };
    match ((probe.device)(old), (probe.device)(target)) {
        (Some(a), Some(b)) if a == b => {}
        (Some(_), Some(_)) => return stop(format!("{} and {} are on different file systems", old.display(), new.display())),
        _ => return stop(format!("cannot read the file system of {} or {}", old.display(), target.display())),
    }
    let plan = vec![format!("move {} to {}", old.display(), new.display()), format!("link {} to {}", old.display(), new.display())];
    if dry {
        return Ok(plan.into_iter().map(|l| format!("would {l}")).chain(["Dry run: nothing was written.".into()]).collect());
    }
    let fail = |what: &str, e: std::io::Error| format!("STOP: cannot {what}: {e}.");
    if new_there {
        fs::remove_dir(new).map_err(|e| fail("remove the empty neutral folder", e) + " Nothing was written.")?;
    }
    fs::create_dir_all(parent).map_err(|e| fail("make the parent of the neutral folder", e) + " Nothing was written.")?;
    fs::rename(old, new).map_err(|e| fail("move the folder", e) + " Nothing was written.")?;
    if let Err(e) = link(new, old) {
        // Put the folder back, so the old path works as before.
        let back = fs::rename(new, old).map(|_| "it was moved back. Nothing was changed".to_string());
        return Err(fail("make the link", e) + " " + &back.unwrap_or_else(|e| format!("and cannot move it back: {e}. Move {} to {} by hand", new.display(), old.display())));
    }
    Ok(plan.into_iter().map(|l| format!("did {l}")).collect())
}

/// The folder itself, or its nearest parent that exists.
fn nearest(p: &Path) -> &Path {
    p.ancestors().find(|a| a.exists()).unwrap_or(p)
}

#[cfg(unix)]
fn link(to: &Path, at: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(to, at)
}

#[cfg(windows)]
fn link(to: &Path, at: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(to, at)
}

#[cfg(unix)]
fn device(p: &Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    fs::metadata(p).ok().map(|m| m.dev())
}

#[cfg(windows)]
fn device(p: &Path) -> Option<u64> {
    // The drive letter stands in for the device.
    fs::canonicalize(p).ok().and_then(|c| c.to_string_lossy().chars().find(|c| c.is_ascii_alphabetic())).map(|c| c.to_ascii_uppercase() as u64)
}

/// True when another process holds the lock: a lock that is not blocking fails.
fn locked(p: &Path) -> bool {
    let Ok(f) = fs::OpenOptions::new().read(true).write(true).open(p) else { return false };
    matches!(f.try_lock(), Err(fs::TryLockError::WouldBlock))
}

/// The live workers from `herdr-contr-orch agents ls --json`: the tool in `<root>/bin`, else on the PATH.
fn live_workers(root: &Path) -> Result<Vec<String>, String> {
    let installed = places::Places::at(root).orch();
    let program = if installed.is_file() { installed } else { PathBuf::from("herdr-contr-orch") };
    let o = Command::new(&program)
        .args(["agents", "ls", "--json"])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("{}: {e}", program.display()))?;
    if !o.status.success() {
        return Err(format!("{} agents ls: {}", program.display(), String::from_utf8_lossy(&o.stderr).trim()));
    }
    parse_live(&String::from_utf8_lossy(&o.stdout))
}

/// The names of the workers with `"live": true` in the answer of `agents ls --json`.
pub fn parse_live(text: &str) -> Result<Vec<String>, String> {
    let Ok(Value::Array(ws)) = serde_json::from_str::<Value>(text) else {
        return Err("agents ls --json did not give a JSON list".into());
    };
    Ok(ws.iter().filter(|w| w["live"] == true).map(|w| w["name"].as_str().unwrap_or("?").to_string()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe<'a>(device: &'a dyn Fn(&Path) -> Option<u64>) -> Probe<'a> {
        Probe { device, locked: &|_| false, live_workers: &|_| Ok(vec![]), service: None }
    }

    #[test]
    fn two_file_systems_stop_and_write_nothing() {
        let t = tempfile::tempdir().unwrap();
        let (old, new) = (t.path().join("herdr-app").join("herdr-contr"), t.path().join("contr-backend"));
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("owner.json"), "{}").unwrap();
        let other = |p: &Path| Some(if p.starts_with(t.path().join("herdr-app")) { 1 } else { 2 });
        let e = move_root(&old, &new, &probe(&other), false).unwrap_err();
        assert!(e.starts_with("STOP: ") && e.contains("different file systems") && e.ends_with("Nothing was written."), "{e}");
        assert!(old.join("owner.json").is_file() && !new.exists());
        let unknown = |_: &Path| None;
        let e = move_root(&old, &new, &probe(&unknown), false).unwrap_err();
        assert!(e.contains("cannot read the file system"), "{e}");
        assert!(old.join("owner.json").is_file() && !new.exists());
    }

    #[test]
    fn a_tool_that_cannot_list_stops() {
        let t = tempfile::tempdir().unwrap();
        let (old, new) = (t.path().join("old"), t.path().join("new"));
        fs::create_dir_all(&old).unwrap();
        let p = Probe { device: &|_| Some(1), locked: &|_| false, live_workers: &|_| Err("no tool".into()), service: None };
        let e = move_root(&old, &new, &p, false).unwrap_err();
        assert!(e.contains("cannot read the workers: no tool"), "{e}");
        assert!(old.is_dir() && !new.exists());
    }

    #[test]
    fn live_workers_from_ls() {
        assert_eq!(parse_live(r#"[{"name":"a","live":true},{"name":"b","live":false}]"#), Ok(vec!["a".to_string()]));
        assert_eq!(parse_live("[]"), Ok(vec![]));
        assert!(parse_live("no workers").is_err());
    }
}
