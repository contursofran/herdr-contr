//! The settings in `<home>/herdr-contr/config.json` that the helper follows, and `version`. Temp homes only.
#![cfg(unix)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

struct T {
    _tmp: tempfile::TempDir,
    home: PathBuf,
}

fn t() -> T {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().canonicalize().unwrap().join("app");
    fs::create_dir_all(home.join("herdr-contr")).unwrap();
    T { _tmp: tmp, home }
}

impl T {
    fn config(&self, text: &str) {
        fs::write(self.home.join("herdr-contr/config.json"), text).unwrap();
    }
}

/// Never in config.json: a command for a terminal on another machine stops before anything (no herdr-core
/// needed to see it), and so does `helper install` / `remove`.
#[test]
fn remote_helper_never_runs_nothing_on_other_machines() {
    let t = t();
    t.config(r#"{"remote_helper": "never"}"#);
    let run = |args: &[&str]| -> String {
        let o = Command::new(env!("CARGO_BIN_EXE_herdr-contr-feed"))
            .args(args)
            .args(["--home", t.home.to_str().unwrap()])
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HC_SOCKET", t.home.join("no-core.sock"))
            .output()
            .unwrap();
        String::from_utf8(o.stdout).unwrap()
    };
    for args in [
        &["feed", "--terminal", "m1/t1"][..],
        &["send", "--terminal", "m1/t1", "--text", "hi"][..],
        // A text that says --home is not the home: the last --home (the mod's) counts.
        &["send", "--terminal", "m1/t1", "--text", "--home"][..],
        &["card", "--terminal", "hc:m1/t2"][..],
        &["helper", "install", "--machine", "m1", "--sha256", "00"][..],
        &["helper", "remove", "--machine", "m1"][..],
    ] {
        let out = run(args);
        assert!(out.contains(r#""code":"remote-off""#), "{args:?}: {out}");
    }
    // A terminal on this machine is not affected (here: no herdr-core, so that error instead).
    let out = run(&["feed", "--terminal", "t1"]);
    assert!(!out.contains("remote-off"), "{out}");
    // Ask (or no file) does not stop it.
    fs::remove_file(t.home.join("herdr-contr/config.json")).unwrap();
    assert!(!run(&["feed", "--terminal", "m1/t1"]).contains("remote-off"));
}

// The update check is gone: `version` does not offer it, and the command is unknown.
#[test]
fn there_is_no_update_check() {
    let o = Command::new(env!("CARGO_BIN_EXE_herdr-contr-feed")).arg("version").output().unwrap();
    assert!(!String::from_utf8_lossy(&o.stdout).contains("update-check"));
    let o = Command::new(env!("CARGO_BIN_EXE_herdr-contr-feed")).arg("update-check").output().unwrap();
    assert!(!String::from_utf8_lossy(&o.stdout).contains("\"type\":\"update\""));
}

// `version` names the release baked in at build time, which the release's package job checks against the tag.
#[test]
fn version_names_the_baked_release() {
    let o = Command::new(env!("CARGO_BIN_EXE_herdr-contr-feed")).arg("version").output().unwrap();
    let line = String::from_utf8_lossy(&o.stdout).lines().find(|l| l.contains("\"type\":\"version\"")).unwrap().to_string();
    let v: serde_json::Value = serde_json::from_str(&line).unwrap();
    let want = option_env!("HERDR_CONTR_VERSION").map(|s| s.trim_start_matches('v').to_string());
    assert_eq!(v["release"], serde_json::json!(want), "{line}");
}


/// Runs the helper with only these variables (no CONTR_HOME, HERDR_APP_HOME or HOME of this machine).
fn run_with(args: &[&str], vars: &[(&str, &std::path::Path)]) -> String {
    let o = Command::new(env!("CARGO_BIN_EXE_herdr-contr-feed"))
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", "/nonexistent-home")
        .envs(vars.iter().copied())
        .output()
        .unwrap();
    String::from_utf8(o.stdout).unwrap()
}

fn never_in(root: &std::path::Path) {
    fs::create_dir_all(root).unwrap();
    fs::write(root.join("config.json"), r#"{"remote_helper": "never"}"#).unwrap();
}

const REMOTE: [&str; 3] = ["feed", "--terminal", "m1/t1"];

/// `--home` names the root: its settings apply, and those in CONTR_HOME do not.
#[test]
fn home_flag_beats_contr_home() {
    let (flag, contr) = (t(), t());
    flag.config(r#"{"remote_helper": "never"}"#);
    let sock = flag.home.join("no-core.sock");
    let vars = [("CONTR_HOME", contr.home.join("herdr-contr")), ("HC_SOCKET", sock)];
    let vars: Vec<(&str, &std::path::Path)> = vars.iter().map(|(k, v)| (*k, v.as_path())).collect();
    let mut args = REMOTE.to_vec();
    args.extend(["--home", flag.home.to_str().unwrap()]);
    assert!(run_with(&args, &vars).contains(r#""code":"remote-off""#), "the --home settings apply");
    // Never only in CONTR_HOME: --home (no settings there) wins.
    fs::remove_file(flag.home.join("herdr-contr/config.json")).unwrap();
    contr.config(r#"{"remote_helper": "never"}"#);
    let out = run_with(&args, &vars);
    assert!(!out.contains("remote-off"), "{out}");
}

/// Without `--home`, CONTR_HOME is the root itself, before HERDR_APP_HOME.
#[test]
fn contr_home_gives_the_settings() {
    let (tmp, app) = (tempfile::tempdir().unwrap(), t());
    let root = tmp.path().join("backend");
    never_in(&root);
    let sock = tmp.path().join("no-core.sock");
    let vars = [("CONTR_HOME", root.as_path()), ("HERDR_APP_HOME", app.home.as_path()), ("HC_SOCKET", sock.as_path())];
    assert!(run_with(&REMOTE, &vars).contains(r#""code":"remote-off""#));
    // An empty CONTR_HOME counts as unset: HERDR_APP_HOME (no settings) gives the root.
    let empty = std::path::Path::new("");
    let vars = [("CONTR_HOME", empty), ("HERDR_APP_HOME", app.home.as_path()), ("HC_SOCKET", sock.as_path())];
    assert!(!run_with(&REMOTE, &vars).contains("remote-off"));
    // HERDR_APP_HOME alone: `<it>/herdr-contr`.
    app.config(r#"{"remote_helper": "never"}"#);
    assert!(run_with(&REMOTE, &vars).contains(r#""code":"remote-off""#));
}

/// `update install` writes a Herdr home: a CONTR_HOME that is not `<home>/herdr-contr` has none, so it stops first.
#[test]
fn update_install_needs_a_herdr_home() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("backend");
    fs::create_dir_all(&root).unwrap();
    let out = run_with(&["update", "install"], &[("CONTR_HOME", root.as_path())]);
    assert!(out.contains("update install needs --home <herdr-app home>"), "{out}");
}
