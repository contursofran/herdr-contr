//! `herdr-contr-feed install`, run against temp homes only (the port of herdr-app/test/install.test.mjs).
//! HOME is a temp folder too, so the default (~/.config/herdr-app) can never be the real one. A fake
//! herdr-app on PATH records its calls.
#![cfg(unix)]

use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn mod_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../herdr-app")
}

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
    let tmp = tempfile::Builder::new().prefix("fb-install-").tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let (home, app, bin) = (root.join("user"), root.join("app"), root.join("bin"));
    for d in [&home, &app, &bin] {
        fs::create_dir(d).unwrap();
    }
    let calls = root.join("calls");
    let fail = root.join("fail");
    write_exe(
        &bin.join("herdr-app"),
        &format!(
            "#!/bin/sh\necho \"$HERDR_APP_HOME $*\" >> \"{}\"\n[ -f \"{}\" ] && {{ echo \"bad mod\"; exit 1; }}\necho ok\n",
            calls.display(),
            fail.display()
        ),
    );
    let helper = root.join("herdr-contr-feed");
    write_exe(&helper, "#!/bin/sh\necho helper\n");
    Sandbox { _tmp: tmp, root, home, app, helper }
}

impl Sandbox {
    fn path_env(&self) -> String {
        format!("{}:/usr/bin:/bin", self.root.join("bin").display())
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_herdr-contr-feed"))
            .arg("install")
            .args(args)
            .env_clear()
            .env("PATH", self.path_env())
            .env("HOME", &self.home)
            .output()
            .unwrap()
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
fn keys_of(app: &Path) -> Value {
    serde_json::from_str(&read(app.join("keys.json"))).unwrap()
}
fn ours() -> Value {
    serde_json::from_str(&read(mod_dir().join("keys.json"))).unwrap()
}
fn sha(text: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(text).iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn installs_into_an_empty_home_puts_the_helper_in_place_and_runs_check() {
    let s = sandbox();
    let r = s.run(&["--home", s.app_s(), "--helper", s.helper.to_str().unwrap()]);
    assert_eq!(r.status.code(), Some(0), "{}", both(&r));
    assert!(s.app.join("layouts/herdr-contr-own.json").exists());
    assert!(s.app.join("mods/herdr-contr/mod.json").exists());
    // The layout to switch to is herdr-contr: Herdr's sidebar and tab bar (decision 6).
    assert!(out(&r).contains(r#"Installed. In herdr-app run: layout.switch {"name": "herdr-contr"}"#), "{}", out(&r));
    assert!(s.app.join("themes/herdr-contr-light.json").exists());
    assert_eq!(read(s.app.join("herdr-contr/bin/herdr-contr-feed")), read(&s.helper));
    let mode = fs::metadata(s.app.join("herdr-contr/bin/herdr-contr-feed")).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o755);
    assert_eq!(keys_of(&s.app), ours());
    assert!(!s.app.join("mods/herdr-contr-agents").exists(), "retired in #98");
    assert!(s.app.join("mods/herdr-contr-tabs/win-close.svg").exists());
    let checks: Vec<String> = ["herdr-contr", "herdr-contr-spaces", "herdr-contr-tabs"].iter().map(|m| format!("{0} check {0}/mods/{m}", s.app_s())).collect();
    assert_eq!(s.calls(), checks);
}

#[test]
fn installs_exactly_the_files_in_the_repo() {
    let s = sandbox();
    let r = s.run(&["--home", s.app_s()]);
    assert_eq!(r.status.code(), Some(0), "{}", both(&r));
    let marker: Value = serde_json::from_str(&read(s.app.join("herdr-contr/install.json"))).unwrap();
    assert_eq!(marker["build"], env!("HERDR_CONTR_BUILD"), "install.json carries the build stamp");
    let files = marker["files"].as_object().unwrap();
    let mut want = vec!["layouts/herdr-contr.json".to_string(), "layouts/herdr-contr-own.json".to_string()];
    for e in fs::read_dir(mod_dir().join("themes")).unwrap() {
        let n = e.unwrap().file_name().into_string().unwrap();
        if n.ends_with(".json") {
            want.push(format!("themes/{n}"));
        }
    }
    let git = Command::new("git").args(["ls-files", "mods/herdr-contr", "mods/herdr-contr-spaces", "mods/herdr-contr-tabs"]).current_dir(mod_dir()).output().unwrap();
    want.extend(String::from_utf8(git.stdout).unwrap().lines().map(String::from));
    want.sort();
    let have: Vec<String> = files.keys().cloned().collect();
    assert_eq!(have, want);
    for rel in &want {
        assert_eq!(fs::read(s.app.join(rel)).unwrap(), fs::read(mod_dir().join(rel)).unwrap(), "{rel}");
        assert_eq!(files[rel], json!(sha(&fs::read(mod_dir().join(rel)).unwrap())), "{rel}");
    }
}

#[test]
fn dry_run_writes_nothing_and_does_not_run_herdr_app() {
    let s = sandbox();
    let r = s.run(&["--home", s.app_s(), "--helper", s.helper.to_str().unwrap(), "--dry-run"]);
    assert_eq!(r.status.code(), Some(0), "{}", both(&r));
    assert_eq!(fs::read_dir(&s.app).unwrap().count(), 0);
    assert!(s.calls().is_empty());
    assert!(out(&r).contains("layouts/herdr-contr-own.json"));
    assert!(out(&r).to_lowercase().contains("dry run"));
}

#[test]
fn stops_without_writing_when_the_person_has_their_own_layout() {
    let s = sandbox();
    fs::create_dir(s.app.join("layouts")).unwrap();
    fs::write(s.app.join("layouts/herdr-contr-own.json"), "{\"mine\": true}\n").unwrap();
    let r = s.run(&["--home", s.app_s()]);
    assert_eq!(r.status.code(), Some(1));
    assert!(err(&r).contains("layouts/herdr-contr-own.json"));
    assert_eq!(read(s.app.join("layouts/herdr-contr-own.json")), "{\"mine\": true}\n");
    assert!(!s.app.join("mods").exists());
    assert!(!s.app.join("keys.json").exists());
}

#[test]
fn stops_when_the_person_has_their_own_mod_folder() {
    let s = sandbox();
    fs::create_dir_all(s.app.join("mods/herdr-contr")).unwrap();
    fs::write(s.app.join("mods/herdr-contr/main.js"), "mine\n").unwrap();
    let r = s.run(&["--home", s.app_s()]);
    assert_eq!(r.status.code(), Some(1));
    assert!(err(&r).contains("mods/herdr-contr"));
    assert_eq!(read(s.app.join("mods/herdr-contr/main.js")), "mine\n");
    assert!(!s.app.join("layouts").exists());
}

#[test]
fn stops_when_the_person_has_a_file_in_a_copied_herdr_mod() {
    let s = sandbox();
    fs::create_dir_all(s.app.join("mods/herdr-contr-tabs")).unwrap();
    fs::write(s.app.join("mods/herdr-contr-tabs/extra.js"), "mine\n").unwrap();
    let r = s.run(&["--home", s.app_s()]);
    assert_eq!(r.status.code(), Some(1));
    assert!(err(&r).contains("mods/herdr-contr-tabs/extra.js"), "{}", err(&r));
    assert!(!s.app.join("layouts").exists());
}

#[test]
fn removes_an_unchanged_file_an_earlier_install_put_in_a_copied_herdr_mod() {
    let s = sandbox();
    let r = s.run(&["--home", s.app_s()]);
    assert_eq!(r.status.code(), Some(0), "{}", both(&r));
    fs::write(s.app.join("mods/herdr-contr-tabs/old.js"), "gone\n").unwrap();
    let marker_path = s.app.join("herdr-contr/install.json");
    let mut marker: Value = serde_json::from_str(&read(&marker_path)).unwrap();
    marker["files"]["mods/herdr-contr-tabs/old.js"] = json!(sha(b"gone\n"));
    fs::write(&marker_path, marker.to_string()).unwrap();
    let r = s.run(&["--home", s.app_s()]);
    assert_eq!(r.status.code(), Some(0), "{}", both(&r));
    assert!(!s.app.join("mods/herdr-contr-tabs/old.js").exists());
}

// An earlier install put the agents copy there (#98 retired it): its unchanged files and the folder go,
// with the files herdr-app wrote in it.
fn retired_install(s: &Sandbox, own: Option<&str>) {
    let r = s.run(&["--home", s.app_s()]);
    assert_eq!(r.status.code(), Some(0), "{}", both(&r));
    let dir = s.app.join("mods/herdr-contr-agents");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("main.js"), "old agents\n").unwrap();
    fs::write(dir.join("mod.json"), "{}\n").unwrap();
    fs::write(dir.join("jsconfig.json"), "{}\n").unwrap();
    fs::write(dir.join("herdr.d.ts"), "x\n").unwrap();
    if let Some(n) = own {
        fs::write(dir.join(n), "mine\n").unwrap();
    }
    let marker_path = s.app.join("herdr-contr/install.json");
    let mut marker: Value = serde_json::from_str(&read(&marker_path)).unwrap();
    marker["files"]["mods/herdr-contr-agents/main.js"] = json!(sha(b"old agents\n"));
    marker["files"]["mods/herdr-contr-agents/mod.json"] = json!(sha(b"{}\n"));
    fs::write(&marker_path, marker.to_string()).unwrap();
    let r = s.run(&["--home", s.app_s()]);
    assert_eq!(r.status.code(), Some(0), "{}", both(&r));
}

#[test]
fn removes_the_retired_agents_copy() {
    let s = sandbox();
    retired_install(&s, None);
    assert!(!s.app.join("mods/herdr-contr-agents").exists());
}

#[test]
fn keeps_the_retired_agents_copy_folder_when_a_file_is_the_persons() {
    let s = sandbox();
    retired_install(&s, Some("notes.js"));
    let dir = s.app.join("mods/herdr-contr-agents");
    assert!(!dir.join("main.js").exists(), "ours, unchanged: removed");
    assert_eq!(read(dir.join("notes.js")), "mine\n");
}

#[test]
fn merges_keys_keeps_the_persons_sections_and_keys() {
    let s = sandbox();
    fs::write(
        s.app.join("keys.json"),
        r#"{"global":{"cmd-k":"app.palette"},"layout:herdr-contr-own":{"cmd-b":"app.something-else"}}"#,
    )
    .unwrap();
    let r = s.run(&["--home", s.app_s()]);
    assert_eq!(r.status.code(), Some(0), "{}", both(&r));
    let k = keys_of(&s.app);
    assert_eq!(k["global"], json!({"cmd-k": "app.palette"}));
    assert_eq!(k["layout:herdr-contr-own"]["cmd-b"], "app.something-else");
    assert_eq!(k["layout:herdr-contr-own"]["cmd-t"], "tab.new");
    assert!(out(&r).contains("cmd-b"));
    // The person's order stays: their sections first, then ours.
    let text = read(s.app.join("keys.json"));
    assert!(text.find("\"global\"").unwrap() < text.find("\"layout:herdr-contr-own\"").unwrap());
}

#[test]
fn a_keys_file_that_is_not_json_stops_everything() {
    let s = sandbox();
    fs::write(s.app.join("keys.json"), "{ not json").unwrap();
    let r = s.run(&["--home", s.app_s()]);
    assert_eq!(r.status.code(), Some(1));
    assert!(err(&r).contains("is not valid JSON"));
    assert!(!s.app.join("layouts").exists());
}

#[test]
fn a_symlinked_keys_file_is_merged_where_it_points_and_the_link_stays() {
    let s = sandbox();
    let dots = s.root.join("dotfiles");
    fs::create_dir(&dots).unwrap();
    fs::write(dots.join("keys.json"), r#"{"global":{"cmd-k":"app.palette"}}"#).unwrap();
    std::os::unix::fs::symlink(dots.join("keys.json"), s.app.join("keys.json")).unwrap();
    let r = s.run(&["--home", s.app_s()]);
    assert_eq!(r.status.code(), Some(0), "{}", both(&r));
    assert!(fs::symlink_metadata(s.app.join("keys.json")).unwrap().file_type().is_symlink());
    assert_eq!(fs::read_link(s.app.join("keys.json")).unwrap(), dots.join("keys.json"));
    let k: Value = serde_json::from_str(&read(dots.join("keys.json"))).unwrap();
    assert_eq!(k["global"], json!({"cmd-k": "app.palette"}));
    assert_eq!(k["layout:herdr-contr-own"]["cmd-t"], "tab.new");
    // The temp file was in the target's folder and is gone; nothing was left beside the link.
    let left: Vec<_> = fs::read_dir(&dots).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(left, vec![std::ffi::OsString::from("keys.json")]);
    assert!(!s.app.join("keys.json.herdr-contr-new").exists());

    // A symlinked layout that already matches ours is written through too, and stays a link.
    let lay = s.app.join("layouts/herdr-contr-own.json");
    fs::copy(&lay, dots.join("herdr-contr.json")).unwrap();
    fs::remove_file(&lay).unwrap();
    std::os::unix::fs::symlink(dots.join("herdr-contr.json"), &lay).unwrap();
    let r = s.run(&["--home", s.app_s()]);
    assert_eq!(r.status.code(), Some(0), "{}", both(&r));
    assert!(fs::symlink_metadata(&lay).unwrap().file_type().is_symlink());

    // A link to a keys.json that is not there yet: it is created where the link points.
    let s = sandbox();
    let dots = s.root.join("dotfiles");
    fs::create_dir(&dots).unwrap();
    std::os::unix::fs::symlink(dots.join("keys.json"), s.app.join("keys.json")).unwrap();
    let r = s.run(&["--home", s.app_s()]);
    assert_eq!(r.status.code(), Some(0), "{}", both(&r));
    assert!(fs::symlink_metadata(s.app.join("keys.json")).unwrap().file_type().is_symlink());
    assert_eq!(keys_of(&s.app)["layout:herdr-contr-own"]["cmd-t"], "tab.new");
}

#[test]
fn without_home_or_dash_dash_home_it_stops_and_writes_nothing() {
    let s = sandbox();
    let r = Command::new(env!("CARGO_BIN_EXE_herdr-contr-feed"))
        .args(["install", "--helper", s.helper.to_str().unwrap()])
        .env_clear()
        .env("PATH", s.path_env())
        .current_dir(&s.root)
        .output()
        .unwrap();
    assert_eq!(r.status.code(), Some(1), "{}", both(&r));
    assert!(err(&r).contains("--home"), "{}", err(&r));
    assert!(!s.root.join(".config").exists());
    assert!(s.calls().is_empty());
    let e = Command::new(env!("CARGO_BIN_EXE_herdr-contr-feed"))
        .args(["install", "--dry-run"])
        .env_clear()
        .env("PATH", s.path_env())
        .env("HOME", "")
        .current_dir(&s.root)
        .output()
        .unwrap();
    assert_eq!(e.status.code(), Some(1), "{}", both(&e));
}

#[test]
fn installing_again_updates_its_own_files_keeps_edits() {
    let s = sandbox();
    assert_eq!(s.run(&["--home", s.app_s()]).status.code(), Some(0));
    let stale = "stale copy from an older release\n";
    fs::write(s.app.join("mods/herdr-contr/main.js"), stale).unwrap();
    let mp = s.app.join("herdr-contr/install.json");
    let mut marker: Value = serde_json::from_str(&read(&mp)).unwrap();
    marker["files"]["mods/herdr-contr/main.js"] = json!(sha(stale.as_bytes()));
    fs::write(&mp, marker.to_string()).unwrap();
    let again = s.run(&["--home", s.app_s()]);
    assert_eq!(again.status.code(), Some(0), "{}", both(&again));
    assert_eq!(read(s.app.join("mods/herdr-contr/main.js")), read(mod_dir().join("mods/herdr-contr/main.js")));

    fs::write(s.app.join("layouts/herdr-contr-own.json"), "{\"edited\": true}\n").unwrap();
    let third = s.run(&["--home", s.app_s()]);
    assert_eq!(third.status.code(), Some(1));
    assert!(err(&third).contains("layouts/herdr-contr-own.json"));
    assert_eq!(read(s.app.join("layouts/herdr-contr-own.json")), "{\"edited\": true}\n");
}

#[test]
fn installing_again_removes_its_own_dropped_files_nothing_else() {
    let s = sandbox();
    assert_eq!(s.run(&["--home", s.app_s()]).status.code(), Some(0));
    fs::write(s.app.join("mods/herdr-contr/old.js"), "gone\n").unwrap();
    fs::write(s.app.join("notes.txt"), "mine\n").unwrap();
    let mp = s.app.join("herdr-contr/install.json");
    let mut marker: Value = serde_json::from_str(&read(&mp)).unwrap();
    marker["files"]["mods/herdr-contr/old.js"] = json!(sha(b"gone\n"));
    marker["files"]["notes.txt"] = json!(sha(b"mine\n"));
    fs::write(&mp, marker.to_string()).unwrap();
    assert_eq!(s.run(&["--home", s.app_s()]).status.code(), Some(0));
    assert!(!s.app.join("mods/herdr-contr/old.js").exists());
    assert_eq!(read(s.app.join("notes.txt")), "mine\n");
}

#[test]
fn files_herdr_app_writes_into_the_mod_folder_do_not_block() {
    let s = sandbox();
    assert_eq!(s.run(&["--home", s.app_s()]).status.code(), Some(0));
    fs::write(s.app.join("mods/herdr-contr/jsconfig.json"), "{}\n").unwrap();
    fs::write(s.app.join("mods/herdr-contr/gpui-kit.d.ts"), "// types\n").unwrap();
    let r = s.run(&["--home", s.app_s()]);
    assert_eq!(r.status.code(), Some(0), "{}", both(&r));
    assert_eq!(read(s.app.join("mods/herdr-contr/jsconfig.json")), "{}\n");
}

#[test]
fn a_failed_herdr_app_check_fails_the_install() {
    let s = sandbox();
    fs::write(s.root.join("fail"), "").unwrap();
    let r = s.run(&["--home", s.app_s()]);
    assert_eq!(r.status.code(), Some(1));
    assert!(both(&r).contains("herdr-app check"));
}

#[test]
fn without_herdr_app_on_path_it_installs_and_says_check_did_not_run() {
    let s = sandbox();
    fs::remove_file(s.root.join("bin/herdr-app")).unwrap();
    let r = s.run(&["--home", s.app_s()]);
    assert_eq!(r.status.code(), Some(0), "{}", both(&r));
    assert!(out(&r).contains("herdr-app is not on PATH"));
    assert!(s.app.join("mods/herdr-contr/mod.json").exists());
}

#[test]
fn without_helper_and_none_in_place_it_says_where_to_get_one() {
    let s = sandbox();
    let r = s.run(&["--home", s.app_s()]);
    assert_eq!(r.status.code(), Some(0), "{}", both(&r));
    assert!(out(&r).contains("cargo build --release --manifest-path xplat/Cargo.toml -p herdr-contr-feed"));
    assert!(out(&r).contains("install --helper xplat/target/release/herdr-contr-feed"));
    assert!(!out(&r).to_lowercase().contains("flockbook"));
}

#[test]
fn a_missing_helper_file_stops_before_anything() {
    let s = sandbox();
    let r = s.run(&["--home", s.app_s(), "--helper", "/nonexistent/herdr-contr-feed"]);
    assert_eq!(r.status.code(), Some(1));
    assert!(err(&r).contains("no helper at /nonexistent/herdr-contr-feed"));
    assert_eq!(fs::read_dir(&s.app).unwrap().count(), 0);
}

#[test]
fn bad_arguments_exit_2() {
    let s = sandbox();
    assert_eq!(s.run(&["--nope"]).status.code(), Some(2));
    assert_eq!(s.run(&["--home"]).status.code(), Some(2));
    let h = s.run(&["--help"]);
    assert_eq!(h.status.code(), Some(0));
    assert!(out(&h).contains("--dry-run"));
}

#[test]
fn the_home_defaults_to_herdr_app_home_then_dot_config() {
    let s = sandbox();
    let exe = env!("CARGO_BIN_EXE_herdr-contr-feed");
    let r = Command::new(exe).args(["install", "--dry-run"]).env_clear().env("PATH", s.path_env()).env("HOME", &s.home).output().unwrap();
    assert_eq!(r.status.code(), Some(0), "{}", both(&r));
    assert!(out(&r).contains(s.home.join(".config/herdr-app").to_str().unwrap()), "{}", out(&r));
    let e = Command::new(exe)
        .args(["install", "--dry-run"])
        .env_clear()
        .env("PATH", s.path_env())
        .env("HOME", &s.home)
        .env("HERDR_APP_HOME", &s.app)
        .output()
        .unwrap();
    assert!(out(&e).contains(s.app_s()), "{}", out(&e));
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

#[test]
fn remote_helpers_are_checked_and_copied_with_their_sums() {
    let s = sandbox();
    let dir = remote_dir(&s, None);
    let dry = s.run(&["--home", s.app_s(), "--remote", dir.to_str().unwrap(), "--dry-run"]);
    assert_eq!(dry.status.code(), Some(0), "{}", both(&dry));
    assert!(out(&dry).contains("herdr-contr/remote/aarch64-unknown-linux-musl/herdr-contr-feed"));
    assert_eq!(fs::read_dir(&s.app).unwrap().count(), 0);

    let r = s.run(&["--home", s.app_s(), "--remote", dir.to_str().unwrap()]);
    assert_eq!(r.status.code(), Some(0), "{}", both(&r));
    for t in ["universal-apple-darwin", "x86_64-unknown-linux-musl", "aarch64-unknown-linux-musl"] {
        let p = s.app.join("herdr-contr/remote").join(t).join("herdr-contr-feed");
        assert_eq!(read(&p), format!("#!/bin/sh\necho {t}\n"));
        assert_eq!(fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o755);
    }
    assert_eq!(read(s.app.join("herdr-contr/remote/SHA256SUMS")), read(dir.join("SHA256SUMS")));
}

#[test]
fn a_remote_helper_with_the_wrong_sum_stops_before_anything() {
    let s = sandbox();
    let dir = remote_dir(&s, Some("x86_64-unknown-linux-musl"));
    let r = s.run(&["--home", s.app_s(), "--remote", dir.to_str().unwrap()]);
    assert_eq!(r.status.code(), Some(1));
    assert!(err(&r).contains("x86_64-unknown-linux-musl"), "{}", err(&r));
    assert_eq!(fs::read_dir(&s.app).unwrap().count(), 0);
}

#[test]
fn a_remote_sums_line_that_leaves_the_folder_is_refused() {
    let s = sandbox();
    let dir = remote_dir(&s, None);
    fs::write(dir.join("SHA256SUMS"), format!("{}  ../evil/herdr-contr-feed\n", sha(b"x"))).unwrap();
    let r = s.run(&["--home", s.app_s(), "--remote", dir.to_str().unwrap()]);
    assert_eq!(r.status.code(), Some(1));
    assert_eq!(fs::read_dir(&s.app).unwrap().count(), 0);
}

#[test]
fn a_remote_sums_file_naming_a_target_twice_is_refused() {
    let s = sandbox();
    let dir = remote_dir(&s, None);
    let sums = read(dir.join("SHA256SUMS"));
    let first = sums.lines().next().unwrap().to_string();
    fs::write(dir.join("SHA256SUMS"), format!("{sums}{first}\n")).unwrap();
    let r = s.run(&["--home", s.app_s(), "--remote", dir.to_str().unwrap()]);
    assert_eq!(r.status.code(), Some(1));
    assert!(err(&r).contains("twice"), "{}", err(&r));
    assert_eq!(fs::read_dir(&s.app).unwrap().count(), 0);
}

// The phone page runs the installed helper as a user service (herdr-contr settings, `phone on`): an install that
// replaces the helper restarts it, only when the service file is the settings' own and names this helper.
#[test]
fn an_install_restarts_the_phone_page_that_runs_this_helper() {
    let s = sandbox();
    let bin = s.root.join("bin");
    write_exe(&bin.join("systemctl"), &format!("#!/bin/sh\necho \"systemctl $*\" >> \"{}\"\n", s.root.join("calls").display()));
    write_exe(&bin.join("id"), "#!/bin/sh\necho 501\n");
    let unit = s.home.join(".config/systemd/user/herdr-contr-serve.service");
    fs::create_dir_all(unit.parent().unwrap()).unwrap();
    let helper = s.app.join("herdr-contr/bin/herdr-contr-feed");
    let install = |s: &Sandbox| {
        Command::new(env!("CARGO_BIN_EXE_herdr-contr-feed"))
            .args(["install", "--home", s.app_s(), "--helper", s.helper.to_str().unwrap()])
            .env_clear()
            .env("PATH", s.path_env())
            .env("HOME", &s.home)
            .env("HERDR_CONTR_PHONE_BIN", &bin)
            .env("HERDR_CONTR_PHONE_SERVICE", "systemd")
            .output()
            .unwrap()
    };
    // Set up by hand (no marker): left alone.
    fs::write(&unit, format!("[Service]\nExecStart=\"{}\" \"serve\"\n", helper.display())).unwrap();
    let r = install(&s);
    assert_eq!(r.status.code(), Some(0), "{}", both(&r));
    assert!(!s.calls().iter().any(|c| c.starts_with("systemctl")), "{:?}", s.calls());
    // The settings' own, for another home's helper: left alone.
    fs::write(&unit, format!("# {}\n[Service]\nExecStart=\"/elsewhere/herdr-contr-feed\" \"serve\"\n", modfeedkit::phone::MARKER)).unwrap();
    install(&s);
    assert!(!s.calls().iter().any(|c| c.starts_with("systemctl")), "{:?}", s.calls());
    // The settings' own, running this helper: restarted.
    fs::write(&unit, format!("# {}\n[Service]\nExecStart=\"{}\" \"serve\"\n", modfeedkit::phone::MARKER, helper.display())).unwrap();
    let r = install(&s);
    assert!(s.calls().iter().any(|c| c == "systemctl --user try-restart herdr-contr-serve"), "{:?}", s.calls());
    assert!(out(&r).contains("the phone page (herdr-contr settings): restarted"), "{}", out(&r));
}

/// Every file of the repository's kit, as paths relative to it.
fn kit_files() -> Vec<String> {
    fn walk(dir: &Path, base: &Path, out: &mut Vec<String>) {
        for e in fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(&p, base, out);
            } else if p.file_name().unwrap() != ".DS_Store" {
                out.push(p.strip_prefix(base).unwrap().to_str().unwrap().to_string());
            }
        }
    }
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../kit");
    let mut out = Vec::new();
    walk(&base, &base, &mut out);
    out.sort();
    out
}

#[test]
fn installs_the_kit_and_the_orchestration_tool() {
    let s = sandbox();
    let orch = s.root.join("herdr-contr-orch");
    write_exe(&orch, "#!/bin/sh\necho orch\n");
    let r = s.run(&["--home", s.app_s(), "--orch", orch.to_str().unwrap()]);
    assert_eq!(r.status.code(), Some(0), "{}", both(&r));
    let kit = s.app.join("herdr-contr/kit");
    let files = kit_files();
    assert!(files.contains(&"hooks/hooks.json".to_string()) && files.contains(&".claude-plugin/plugin.json".to_string()));
    for rel in &files {
        assert_eq!(fs::read(kit.join(rel)).unwrap(), fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../kit").join(rel)).unwrap(), "{rel}");
    }
    // The marketplace file names the kit folder next to it, and is the repository's.
    let market: Value = serde_json::from_str(&read(s.app.join("herdr-contr/.claude-plugin/marketplace.json"))).unwrap();
    assert_eq!(market["plugins"][0]["source"], "./kit");
    let plugin: Value = serde_json::from_str(&read(kit.join(".claude-plugin/plugin.json"))).unwrap();
    assert_eq!(market["plugins"][0]["name"], plugin["name"]);
    assert_eq!(read(s.app.join("herdr-contr/.claude-plugin/marketplace.json")), read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.claude-plugin/marketplace.json")));
    let dest = s.app.join("herdr-contr/bin/herdr-contr-orch");
    assert_eq!(read(&dest), read(&orch));
    assert_eq!(fs::metadata(&dest).unwrap().permissions().mode() & 0o777, 0o755);
    assert!(out(&r).contains(&format!("herdr-contr/kit/... ({} files)", files.len())), "{}", out(&r));
    assert!(out(&r).contains("herdr-contr/bin/herdr-contr-orch (from "), "{}", out(&r));
}

#[test]
fn a_second_install_removes_a_kit_file_that_this_version_does_not_have() {
    let s = sandbox();
    assert_eq!(s.run(&["--home", s.app_s()]).status.code(), Some(0));
    let old = s.app.join("herdr-contr/kit/agents/explorer.md");
    fs::write(&old, "old").unwrap();
    let dry = s.run(&["--home", s.app_s(), "--dry-run"]);
    assert!(out(&dry).contains("herdr-contr/kit/agents/explorer.md: not in this version, removed"), "{}", out(&dry));
    assert!(old.exists(), "a dry run removes nothing");
    let r = s.run(&["--home", s.app_s()]);
    assert_eq!(r.status.code(), Some(0), "{}", both(&r));
    assert!(!old.exists());
    assert!(s.app.join("herdr-contr/kit/agents/builder.md").exists());
    // Without --orch, no tool is copied.
    assert!(!s.app.join("herdr-contr/bin/herdr-contr-orch").exists());
}

#[test]
fn stops_when_the_orchestration_tool_is_not_a_file() {
    let s = sandbox();
    let r = s.run(&["--home", s.app_s(), "--orch", s.root.join("none").to_str().unwrap()]);
    assert_eq!(r.status.code(), Some(1), "{}", both(&r));
    assert!(both(&r).contains("STOP: no orchestration tool at "), "{}", both(&r));
    assert_eq!(fs::read_dir(&s.app).unwrap().count(), 0);
}

/// The kit's hooks find the tool that install put next to the kit, with no tool on the PATH.
#[test]
fn the_installed_kit_runs_the_tool_next_to_it() {
    let s = sandbox();
    let orch = s.root.join("herdr-contr-orch");
    write_exe(&orch, "#!/bin/sh\necho \"orch $*\"\n");
    assert_eq!(s.run(&["--home", s.app_s(), "--orch", orch.to_str().unwrap()]).status.code(), Some(0));
    let kit = s.app.join("herdr-contr/kit");
    let run = |command: &str, vars: &[(&str, &str)]| {
        let o = Command::new("sh").arg("-c").arg(command).env_clear().env("PATH", "/usr/bin:/bin").envs(vars.iter().copied()).output().unwrap();
        out(&o)
    };
    let hooks: Value = serde_json::from_str(&read(kit.join("hooks/hooks.json"))).unwrap();
    // Claude Code puts the plugin folder in place of ${CLAUDE_PLUGIN_ROOT}.
    let stop = hooks["hooks"]["Stop"][0]["hooks"][0]["command"].as_str().unwrap().replace("${CLAUDE_PLUGIN_ROOT}", kit.to_str().unwrap());
    assert_eq!(run(&stop, &[("HC_TERMINAL_ID", "t1")]), "orch hook stop\n");
    let mcp: Value = serde_json::from_str(&read(kit.join(".mcp.json"))).unwrap();
    let server = &mcp["mcpServers"]["contr"];
    assert_eq!((server["command"].as_str(), server["args"][0].as_str()), (Some("sh"), Some("-c")));
    assert_eq!(server["env"]["CONTR_KIT"], "${CLAUDE_PLUGIN_ROOT}");
    assert_eq!(run(server["args"][1].as_str().unwrap(), &[("CONTR_KIT", kit.to_str().unwrap())]), "orch mcp\n");
}

/// The contr app's folder in the sandbox's user config folder.
fn old_contr(s: &Sandbox) -> PathBuf {
    s.home.join(if cfg!(target_os = "macos") { "Library/Application Support/contr" } else { ".config/contr" })
}

#[test]
fn copies_the_contr_apps_profiles_and_owner_files_once() {
    let s = sandbox();
    let old = old_contr(&s);
    fs::create_dir_all(&old).unwrap();
    fs::write(old.join("agents.json"), r#"{"flow": {"workers": 2}}"#).unwrap();
    fs::write(old.join("owner.json"), r#"{"questions": []}"#).unwrap();
    let dry = s.run(&["--home", s.app_s(), "--dry-run"]);
    assert!(out(&dry).contains("herdr-contr/agents.json (a copy of ") && out(&dry).contains("herdr-contr/owner.json (a copy of "), "{}", out(&dry));
    assert_eq!(fs::read_dir(&s.app).unwrap().count(), 0);
    let r = s.run(&["--home", s.app_s()]);
    assert_eq!(r.status.code(), Some(0), "{}", both(&r));
    assert_eq!(read(s.app.join("herdr-contr/agents.json")), r#"{"flow": {"workers": 2}}"#);
    assert_eq!(read(s.app.join("herdr-contr/owner.json")), r#"{"questions": []}"#);
    assert!(old.join("agents.json").exists() && old.join("owner.json").exists(), "the old files stay");
    // A file that is in the Herdr home is never replaced.
    fs::write(s.app.join("herdr-contr/agents.json"), "{}").unwrap();
    fs::write(old.join("agents.json"), r#"{"flow": {"workers": 3}}"#).unwrap();
    let r = s.run(&["--home", s.app_s()]);
    assert_eq!(r.status.code(), Some(0), "{}", both(&r));
    assert!(!out(&r).contains("a copy of"), "{}", out(&r));
    assert_eq!(read(s.app.join("herdr-contr/agents.json")), "{}");
}

#[test]
fn with_no_contr_app_files_none_is_made() {
    let s = sandbox();
    let r = s.run(&["--home", s.app_s()]);
    assert_eq!(r.status.code(), Some(0), "{}", both(&r));
    assert!(!s.app.join("herdr-contr/agents.json").exists() && !s.app.join("herdr-contr/owner.json").exists());
}
