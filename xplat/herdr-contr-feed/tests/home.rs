//! `home move`: the move of the backend root to the neutral folder, its stops, and the root before and after it.
//! Each test has its own HOME; the orchestration tool is a fake in the old root's bin.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Home {
    _t: tempfile::TempDir,
    home: PathBuf,
}

impl Home {
    /// A HOME with `~/.config/herdr-app/herdr-contr` (an owner file, a kit file and a fake tool with no live worker).
    fn new() -> Home {
        let t = tempfile::tempdir().unwrap();
        let home = fs::canonicalize(t.path()).unwrap();
        let h = Home { _t: t, home };
        fs::create_dir_all(h.old().join("kit")).unwrap();
        fs::write(h.old().join("owner.json"), "{\"items\":[]}").unwrap();
        fs::write(h.old().join("kit").join("README.md"), "kit").unwrap();
        h.workers("[]");
        h
    }
    fn old(&self) -> PathBuf {
        self.home.join(".config").join("herdr-app").join("herdr-contr")
    }
    fn neutral(&self) -> PathBuf {
        self.home.join(".config").join("contr-backend")
    }
    /// The fake `herdr-contr-orch agents ls --json` gives `json`.
    fn workers(&self, json: &str) {
        let p = self.old().join("bin").join("herdr-contr-orch");
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, format!("#!/bin/sh\n[ \"$*\" = \"agents ls --json\" ] || exit 9\ncat <<'EOF'\n{json}\nEOF\n")).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_herdr-contr-feed"))
            .args(args)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", &self.home)
            .env("HERDR_CONTR_PHONE_SERVICE", "launchd")
            .output()
            .unwrap()
    }
    /// The files under HOME, with a mark for each link.
    fn tree(&self) -> Vec<String> {
        fn walk(d: &Path, base: &Path, out: &mut Vec<String>) {
            for e in fs::read_dir(d).unwrap() {
                let p = e.unwrap().path();
                let m = fs::symlink_metadata(&p).unwrap();
                let rel = p.strip_prefix(base).unwrap().display().to_string();
                if m.file_type().is_symlink() {
                    out.push(format!("{rel} -> {}", fs::read_link(&p).unwrap().display()));
                } else if m.is_dir() {
                    out.push(format!("{rel}/"));
                    walk(&p, base, out);
                } else {
                    out.push(format!("{rel} {}", fs::read_to_string(&p).unwrap_or_default().len()));
                }
            }
        }
        let mut out = vec![];
        walk(&self.home, &self.home, &mut out);
        out.sort();
        out
    }
    /// The backend root that the helper uses: the one `install --backend --dry-run` names.
    fn root(&self) -> String {
        let o = self.run(&["install", "--backend", "--dry-run"]);
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        let text = String::from_utf8_lossy(&o.stdout).into_owned();
        text.lines().next().unwrap().rsplit(" into ").next().unwrap().to_string()
    }
}

fn stopped(o: &Output, why: &str) {
    let err = String::from_utf8_lossy(&o.stderr);
    assert_eq!(o.status.code(), Some(1), "{err}");
    assert!(err.starts_with("STOP: ") && err.contains(why) && err.trim_end().ends_with("Nothing was written."), "{err}");
}

/// Runs `home move`, which must stop for `why` and leave HOME as it was.
fn stops(h: &Home, why: &str) {
    let before = h.tree();
    stopped(&h.run(&["home", "move"]), why);
    assert_eq!(h.tree(), before);
}

#[test]
fn move_leaves_a_link_and_the_root_follows() {
    let h = Home::new();
    assert_eq!(h.root(), h.old().display().to_string());
    let o = h.run(&["home", "move"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(out.contains(&format!("did move {} to {}", h.old().display(), h.neutral().display())), "{out}");
    assert!(fs::symlink_metadata(h.old()).unwrap().file_type().is_symlink());
    assert_eq!(fs::read_link(h.old()).unwrap(), h.neutral());
    assert_eq!(fs::read_to_string(h.neutral().join("owner.json")).unwrap(), "{\"items\":[]}");
    assert_eq!(fs::read_to_string(h.old().join("kit").join("README.md")).unwrap(), "kit");
    assert_eq!(h.root(), h.neutral().display().to_string());
    // A second move finds the link and stops.
    stops(&h, "is a link already");
}

#[test]
fn an_empty_neutral_folder_is_replaced() {
    let h = Home::new();
    fs::create_dir_all(h.neutral()).unwrap();
    let o = h.run(&["home", "move"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(h.neutral().join("owner.json").is_file() && fs::read_link(h.old()).unwrap() == h.neutral());
}

#[test]
fn dry_run_writes_nothing() {
    let h = Home::new();
    let before = h.tree();
    let o = h.run(&["home", "move", "--dry-run"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(out.contains(&format!("would move {} to {}", h.old().display(), h.neutral().display())), "{out}");
    assert!(out.contains("Dry run: nothing was written."), "{out}");
    assert_eq!(h.tree(), before);
}

#[test]
fn a_neutral_folder_with_files_stops() {
    let h = Home::new();
    fs::create_dir_all(h.neutral()).unwrap();
    fs::write(h.neutral().join("owner.json"), "{}").unwrap();
    stops(&h, "exists and is not empty");
}

#[test]
fn a_held_lock_stops() {
    for name in ["agents.json.lock", "owner.json.lock"] {
        let h = Home::new();
        let f = fs::OpenOptions::new().create(true).truncate(false).read(true).write(true).open(h.old().join(name)).unwrap();
        f.lock().unwrap();
        stops(&h, &format!("{name} is held"));
        f.unlock().unwrap();
        // A lock file that no one holds does not stop it.
        let o = h.run(&["home", "move", "--dry-run"]);
        assert!(o.status.success(), "{name}: {}", String::from_utf8_lossy(&o.stderr));
    }
}

#[test]
fn a_live_worker_stops() {
    let h = Home::new();
    h.workers(r#"[{"name":"b-1","live":true},{"name":"b-2","live":false}]"#);
    stops(&h, "live workers: b-1.");
    h.workers("not json");
    stops(&h, "cannot read the workers");
    fs::remove_file(h.old().join("bin").join("herdr-contr-orch")).unwrap();
    stops(&h, "cannot read the workers");
}

#[test]
fn a_phone_service_with_the_old_path_stops() {
    let h = Home::new();
    let agents = h.home.join("Library").join("LaunchAgents");
    fs::create_dir_all(&agents).unwrap();
    let file = agents.join("dev.herdr-contr.serve.plist");
    fs::write(&file, "<plist><string>/elsewhere/bin/herdr-contr-feed</string></plist>").unwrap();
    let o = h.run(&["home", "move", "--dry-run"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    fs::write(&file, format!("<plist><string>{}/bin/herdr-contr-feed</string></plist>", h.old().display())).unwrap();
    stops(&h, "the phone service file");
}

#[test]
fn no_root_to_move_and_bad_arguments() {
    let h = Home::new();
    fs::remove_dir_all(h.old()).unwrap();
    stops(&h, "there is no");
    assert_eq!(h.run(&["home"]).status.code(), Some(2));
    assert_eq!(h.run(&["home", "move", "--force"]).status.code(), Some(2));
    let help = h.run(&["home", "--help"]);
    assert!(help.status.success() && String::from_utf8_lossy(&help.stdout).contains("home move [--dry-run]"));
}
