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

