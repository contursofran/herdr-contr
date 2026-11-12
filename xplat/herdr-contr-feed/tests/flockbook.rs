//! herdr-contr and an installed Flockbook in one Herdr home: neither install touches the other's files
//! or keys. The Flockbook install here is a fake with Flockbook's names and rules. Temp homes only.
#![cfg(unix)]

use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The files a Flockbook install puts in a Herdr home (its names at the commit this repository copied).
const FLOCKBOOK: &[(&str, &str)] = &[
    ("layouts/flockbook.json", "{\"flockbook\": 1}\n"),
    ("layouts/flockbook-herdr.json", "{\"flockbook\": 2}\n"),
    ("themes/flockbook.json", "{\"name\": \"flockbook\"}\n"),
    ("themes/flockbook-light.json", "{\"name\": \"flockbook-light\"}\n"),
    ("themes/flockbook-mac.json", "{\"name\": \"flockbook-mac\"}\n"),
    ("themes/flockbook-mac-light.json", "{\"name\": \"flockbook-mac-light\"}\n"),
    ("mods/flockbook/mod.json", "{\"id\": \"flockbook\"}\n"),
    ("mods/flockbook/main.js", "export default class Flockbook {}\n"),
    ("flockbook/bin/flockbook-modfeed", "#!/bin/sh\necho flockbook\n"),
    ("flockbook/install.json", "{\"layouts/flockbook.json\": \"x\"}\n"),
    ("flockbook/config.json", "{\"check_updates\": true}\n"),
];

fn flockbook_keys() -> Value {
    json!({
        "layout:flockbook": {"cmd-b": "flockbook.sidebar", "cmd-t": "tab.new"},
        "layout:flockbook-herdr": {"cmd-b": "flockbook.sidebar", "cmd-t": "tab.new"},
    })
}

/// What the Flockbook install does: write its own files, and add its own key sections to keys.json.
fn install_flockbook(app: &Path) {
    for (rel, text) in FLOCKBOOK {
        let p = app.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }
    let path = app.join("keys.json");
    let mut keys: serde_json::Map<String, Value> =
        fs::read(&path).ok().map(|b| serde_json::from_slice(&b).unwrap()).unwrap_or_default();
    for (section, map) in flockbook_keys().as_object().unwrap() {
        keys.insert(section.clone(), map.clone());
    }
    fs::write(path, serde_json::to_string_pretty(&keys).unwrap()).unwrap();
}

/// A path Flockbook's install owns: only these does it ever write, replace or remove.
fn flockbook_owns(rel: &str) -> bool {
    rel == "layouts/flockbook.json"
        || rel == "layouts/flockbook-herdr.json"
        || rel.starts_with("mods/flockbook/")
        || rel.starts_with("themes/flockbook")
        || rel.starts_with("flockbook/")
}

fn home() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let tmp = tempfile::Builder::new().prefix("hc-flockbook-").tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let (user, app) = (root.join("user"), root.join("app"));
    fs::create_dir(&user).unwrap();
    fs::create_dir(&app).unwrap();
    (tmp, user, app)
}

fn install_ours(user: &Path, app: &Path) {
    let o = Command::new(env!("CARGO_BIN_EXE_herdr-contr-feed"))
        .args(["install", "--home", app.to_str().unwrap()])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", user)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
}

/// Every file in the home, path -> bytes.
fn snapshot(app: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(dir: &Path, base: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for e in fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(&p, base, out);
            } else {
                out.insert(p.strip_prefix(base).unwrap().to_string_lossy().into_owned(), fs::read(&p).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(app, app, &mut out);
    out
}

fn keys(app: &Path) -> Value {
    serde_json::from_slice(&fs::read(app.join("keys.json")).unwrap()).unwrap()
}

#[test]
fn our_install_does_not_touch_an_installed_flockbook() {
    let (_tmp, user, app) = home();
    install_flockbook(&app);
    let before = snapshot(&app);
    install_ours(&user, &app);
    let after = snapshot(&app);
    for (rel, bytes) in before.iter().filter(|(r, _)| *r != "keys.json") {
        assert_eq!(after.get(rel), Some(bytes), "{rel} changed");
    }
    for rel in after.keys().filter(|r| !before.contains_key(*r)) {
        assert!(!flockbook_owns(rel), "{rel} is a Flockbook path");
    }
    let k = keys(&app);
    for (section, map) in flockbook_keys().as_object().unwrap() {
        assert_eq!(&k[section], map, "{section} changed");
    }
    assert!(app.join("mods/herdr-contr/mod.json").is_file());
    assert!(app.join("layouts/herdr-contr.json").is_file() && app.join("layouts/herdr-contr-own.json").is_file());
    assert!(app.join("herdr-contr/install.json").is_file());
}

#[test]
fn an_installed_flockbook_does_not_touch_our_install() {
    let (_tmp, user, app) = home();
    install_ours(&user, &app);
    let ours = snapshot(&app);
    let our_keys = keys(&app);
    for rel in ours.keys() {
        assert!(!flockbook_owns(rel), "{rel} is a path that a Flockbook install owns");
    }
    for (section, map) in our_keys.as_object().unwrap() {
        assert!(!flockbook_keys().as_object().unwrap().contains_key(section), "{section} is a Flockbook section");
        for cmd in map.as_object().unwrap().values() {
            assert!(!cmd.as_str().unwrap().starts_with("flockbook."), "{section}: {cmd}");
        }
    }
    install_flockbook(&app);
    let after = snapshot(&app);
    for (rel, bytes) in ours.iter().filter(|(r, _)| *r != "keys.json") {
        assert_eq!(after.get(rel), Some(bytes), "{rel} changed");
    }
    let k = keys(&app);
    for (section, map) in our_keys.as_object().unwrap() {
        assert_eq!(&k[section], map, "{section} changed");
    }
    // Our install again: it still finds its own files and leaves Flockbook's alone.
    install_ours(&user, &app);
    for (rel, text) in FLOCKBOOK {
        assert_eq!(fs::read_to_string(app.join(rel)).unwrap(), *text, "{rel} changed");
    }
}
