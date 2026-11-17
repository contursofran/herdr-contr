//! `herdr-contr-feed install --backend`, and `install` in a build without the feature herdr-mod: the backend
//! root only, with no mod file. Temp folders only; HOME is one too. A fake herdr-app on PATH records its calls.
#![cfg(unix)]

use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Sandbox {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
    app: PathBuf,
    helper: PathBuf,
}

fn write_exe(path: &Path, text: &str) {
    fs::write(path, text).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn sandbox() -> Sandbox {
    let tmp = tempfile::Builder::new().prefix("fb-install-backend-").tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let (home, app, bin) = (root.join("user"), root.join("app"), root.join("bin"));
    for d in [&home, &app, &bin] {
        fs::create_dir(d).unwrap();
    }
    write_exe(&bin.join("herdr-app"), &format!("#!/bin/sh\necho \"$HERDR_APP_HOME $*\" >> \"{}\"\necho ok\n", root.join("calls").display()));
    let helper = root.join("herdr-contr-feed");
    write_exe(&helper, "#!/bin/sh\necho helper\n");
    Sandbox { _tmp: tmp, root, home, app, helper }
}

impl Sandbox {
    fn path_env(&self) -> String {
        format!("{}:/usr/bin:/bin", self.root.join("bin").display())
    }
    fn calls(&self) -> Vec<String> {
        match fs::read_to_string(self.root.join("calls")) {
            Ok(t) => t.trim().lines().map(String::from).collect(),
            Err(_) => vec![],
        }
    }
    fn app_s(&self) -> &str {
        self.app.to_str().unwrap()
    }
}

fn read(p: impl AsRef<Path>) -> String {
    fs::read_to_string(p).unwrap()
}
fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}
fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}
fn both(o: &Output) -> String {
    out(o) + &err(o)
}
fn sha(text: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(text).iter().map(|b| format!("{b:02x}")).collect()
}

/// A remote/ folder as the release zip has it: <target>/herdr-contr-feed and SHA256SUMS.
fn remote_dir(s: &Sandbox, corrupt: Option<&str>) -> PathBuf {
    let dir = s.root.join("remote");
    let mut sums = String::new();
    for t in ["universal-apple-darwin", "x86_64-unknown-linux-musl", "aarch64-unknown-linux-musl"] {
        fs::create_dir_all(dir.join(t)).unwrap();
        let body = format!("#!/bin/sh\necho {t}\n");
        write_exe(&dir.join(t).join("herdr-contr-feed"), &body);
        let sum = if corrupt == Some(t) { sha(b"something else") } else { sha(body.as_bytes()) };
        sums.push_str(&format!("{sum}  {t}/herdr-contr-feed\n"));
    }
    fs::write(dir.join("SHA256SUMS"), sums).unwrap();
    dir
}

impl Sandbox {
    /// `install --backend` with these variables (and the sandbox's PATH and HOME), and no other.
    fn run_backend(&self, args: &[&str], vars: &[(&str, &Path)]) -> Output {
        let mut c = Command::new(env!("CARGO_BIN_EXE_herdr-contr-feed"));
        c.args(["install", "--backend"]).args(args).env_clear().env("PATH", self.path_env()).env("HOME", &self.home);
        for (k, v) in vars {
            c.env(k, v);
        }
        c.output().unwrap()
    }
    /// A fake orchestration tool.
    fn orch(&self) -> PathBuf {
        let p = self.root.join("herdr-contr-orch");
        write_exe(&p, "#!/bin/sh\necho orch\n");
        p
    }
}

/// The files and folders under `dir`, as paths relative to it.
fn all_paths(dir: &Path, base: &Path, out: &mut Vec<String>) {
    for e in fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        out.push(p.strip_prefix(base).unwrap().to_string_lossy().into_owned());
        if p.is_dir() && !fs::symlink_metadata(&p).unwrap().file_type().is_symlink() {
            all_paths(&p, base, out);
        }
    }
}

#[test]
fn backend_install_writes_no_mod_file() {
    let s = sandbox();
    let r_dir = s.root.join("r");
    let orch = s.orch();
    let r = s.run_backend(&["--helper", s.helper.to_str().unwrap(), "--orch", orch.to_str().unwrap()], &[("CONTR_HOME", &r_dir)]);
    assert_eq!(r.status.code(), Some(0), "{}", both(&r));
    for f in ["kit/.claude-plugin/plugin.json", ".claude-plugin/marketplace.json", "bin/herdr-contr-feed", "bin/herdr-contr-orch"] {
        assert!(r_dir.join(f).is_file(), "{f} is missing: {}", out(&r));
    }
    assert_eq!(read(r_dir.join("bin/herdr-contr-orch")), "#!/bin/sh\necho orch\n");
    let mut paths = Vec::new();
    all_paths(&s.root, &s.root, &mut paths);
    for p in &paths {
        let name = Path::new(p).file_name().unwrap().to_str().unwrap();
        assert!(!["layouts", "themes", "mods", "keys.json"].contains(&name), "{p} was written");
    }
    assert!(s.calls().is_empty(), "herdr-app check ran: {:?}", s.calls());
    let record: Value = serde_json::from_str(&read(r_dir.join("install.json"))).unwrap();
    assert_eq!(record, json!({ "build": env!("HERDR_CONTR_BUILD") }));
}

#[test]
fn backend_dry_run_writes_nothing() {
    let s = sandbox();
    let r_dir = s.root.join("r");
    let r = s.run_backend(&["--dry-run", "--helper", s.helper.to_str().unwrap()], &[("CONTR_HOME", &r_dir)]);
    assert_eq!(r.status.code(), Some(0), "{}", both(&r));
    assert!(out(&r).contains("bin/herdr-contr-feed"), "{}", out(&r));
    assert!(!r_dir.exists());
    assert!(s.calls().is_empty());
}

#[test]
fn backend_install_keeps_the_mod_record() {
    let s = sandbox();
    // The record of an install of the mod, by an older build: the backend install gives it this build.
    let path = s.app.join("herdr-contr/install.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let before = json!({ "build": "old 2026-01-01", "files": { "mods/herdr-contr/mod.json": sha(b"{}") } });
    fs::write(&path, before.to_string()).unwrap();
    let r = s.run_backend(&["--home", s.app_s()], &[]);
    assert_eq!(r.status.code(), Some(0), "{}", both(&r));
    let after: Value = serde_json::from_str(&read(&path)).unwrap();
    assert_eq!(after["files"], before["files"]);
    assert_eq!(after["build"], env!("HERDR_CONTR_BUILD"));
}

#[test]
fn backend_install_removes_an_old_kit_file() {
    let s = sandbox();
    let r_dir = s.root.join("r");
    fs::create_dir_all(r_dir.join("kit")).unwrap();
    fs::write(r_dir.join("kit/old.md"), "old").unwrap();
    let r = s.run_backend(&[], &[("CONTR_HOME", &r_dir)]);
    assert_eq!(r.status.code(), Some(0), "{}", both(&r));
    assert!(out(&r).contains("kit/old.md: not in this version, removed"), "{}", out(&r));
    assert!(!r_dir.join("kit/old.md").exists());
    assert!(r_dir.join("kit/.claude-plugin/plugin.json").is_file());
}

#[test]
fn backend_install_stops_on_a_bad_remote() {
    let s = sandbox();
    let r_dir = s.root.join("r");
    let dir = remote_dir(&s, Some("x86_64-unknown-linux-musl"));
    let r = s.run_backend(&["--remote", dir.to_str().unwrap()], &[("CONTR_HOME", &r_dir)]);
    assert_eq!(r.status.code(), Some(1), "{}", both(&r));
    assert!(err(&r).contains("Nothing was written."), "{}", err(&r));
    assert!(!r_dir.join("kit").exists());
}

#[test]
fn backend_install_without_a_root_stops() {
    let s = sandbox();
    let r = Command::new(env!("CARGO_BIN_EXE_herdr-contr-feed"))
        .args(["install", "--backend"])
        .env_clear()
        .env("PATH", s.path_env())
        .current_dir(&s.root)
        .output()
        .unwrap();
    assert_eq!(r.status.code(), Some(1), "{}", both(&r));
    assert!(err(&r).contains("CONTR_HOME") && err(&r).contains("Nothing was written."), "{}", err(&r));
    assert!(!s.root.join(".config").exists());
}

/// Without the feature, `install` (no --backend) is the backend install.
#[cfg(not(feature = "herdr-mod"))]
#[test]
fn install_is_the_backend_install() {
    let s = sandbox();
    let r_dir = s.root.join("r");
    let r = Command::new(env!("CARGO_BIN_EXE_herdr-contr-feed"))
        .arg("install")
        .env_clear()
        .env("PATH", s.path_env())
        .env("HOME", &s.home)
        .env("CONTR_HOME", &r_dir)
        .output()
        .unwrap();
    assert_eq!(r.status.code(), Some(0), "{}", both(&r));
    assert!(out(&r).contains("Installing the backend into"), "{}", out(&r));
    assert!(r_dir.join("kit/.claude-plugin/plugin.json").is_file() && r_dir.join(".claude-plugin/marketplace.json").is_file());
    let mut paths = Vec::new();
    all_paths(&s.root, &s.root, &mut paths);
    for p in &paths {
        let name = Path::new(p).file_name().unwrap().to_str().unwrap();
        assert!(!["layouts", "themes", "mods", "keys.json"].contains(&name), "{p} was written");
    }
    assert!(s.calls().is_empty(), "herdr-app check ran: {:?}", s.calls());
}
