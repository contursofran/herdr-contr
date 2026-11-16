//! `pr check-boundary` (#215) on a fake gh that prints a fixture file list, and the boundary that spawn keeps.
mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use common::{call, path, Fake};
use serde_json::json;

const FILES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/data/boundary/pr-files.json"
);

/// A fake gh that prints the fixture, or fails when `ok` is false.
fn gh(dir: &Path, ok: bool) -> String {
    let f = dir.join("gh");
    let body = if ok {
        format!("cat '{FILES}'")
    } else {
        "echo 'gh: no pull request' >&2; exit 1".into()
    };
    fs::write(&f, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&f, fs::Permissions::from_mode(0o755)).unwrap();
    path(&f).to_string()
}

fn brief(dir: &Path, text: &str) -> String {
    let f = dir.join("brief.md");
    fs::write(&f, text).unwrap();
    path(&f).to_string()
}

#[test]
fn lists_the_files_outside_and_exits_1() {
    let tmp = tempfile::tempdir().unwrap();
    let (g, b) = (
        gh(tmp.path(), true),
        brief(
            tmp.path(),
            "Boundary: xplat/herdr-contr-orch, scripts/land.sh\n",
        ),
    );
    let r = call(
        &["pr", "check-boundary", "215", "--brief", &b],
        &[("CONTR_GH", &g)],
        &Fake::default(),
    );
    assert_eq!(r.code, 1, "{}", r.err);
    assert_eq!(r.out, "check-boundary: PR 215 changes files outside the boundary:\n  herdr-app/mods/herdr-contr/chat.js\n");
}

#[test]
fn passes_inside_and_no_docs_refuses_the_docs() {
    let tmp = tempfile::tempdir().unwrap();
    let g = gh(tmp.path(), true);
    let b = brief(
        tmp.path(),
        "Boundary:\n- xplat/**\n- scripts/*.sh\n- herdr-app/mods\n",
    );
    let r = call(
        &["pr", "check-boundary", "215", "--brief", &b],
        &[("CONTR_GH", &g)],
        &Fake::default(),
    );
    assert_eq!(r.code, 0, "{}{}", r.out, r.err);
    let b = brief(tmp.path(), "Boundary: xplat scripts herdr-app no-docs\n");
    let r = call(
        &["pr", "check-boundary", "215", "--brief", &b],
        &[("CONTR_GH", &g)],
        &Fake::default(),
    );
    assert_eq!(
        (r.code, r.out.contains("  AGENTS.md\n")),
        (1, true),
        "{}",
        r.out
    );
}

#[test]
fn errors_exit_2() {
    let tmp = tempfile::tempdir().unwrap();
    let b = brief(tmp.path(), "Boundary: kit\n");
    let r = call(
        &["pr", "check-boundary", "215", "--brief", &b],
        &[("CONTR_GH", &gh(tmp.path(), false))],
        &Fake::default(),
    );
    assert!(
        r.code == 2 && r.err.contains("gh: no pull request"),
        "{}",
        r.err
    );
    let none = brief(tmp.path(), "No part.\n");
    let r = call(
        &["pr", "check-boundary", "215", "--brief", &none],
        &[("CONTR_GH", &gh(tmp.path(), true))],
        &Fake::default(),
    );
    assert!(
        r.code == 2 && r.err.contains("has no Boundary: part"),
        "{}",
        r.err
    );
    assert_eq!(
        call(&["pr", "check-boundary"], &[], &Fake::default()).code,
        2
    );
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

#[test]
fn spawn_keeps_the_boundary_by_branch_and_the_check_finds_it() {
    let tmp = tempfile::tempdir().unwrap();
    let main = tmp.path().join("main");
    fs::create_dir(&main).unwrap();
    git(&main, &["init", "--quiet"]);
    git(
        &main,
        &["commit", "--quiet", "--allow-empty", "-m", "first"],
    );
    let wt = tmp.path().join("wt");
    git(
        &main,
        &[
            "worktree",
            "add",
            "--quiet",
            "-b",
            "agent/215-boundary-check",
            path(&wt),
        ],
    );
    let cfg = tmp.path().join("agents.json");
    fs::write(&cfg, json!({}).to_string()).unwrap();
    let kept = tmp.path().join("kept");
    let g = gh(tmp.path(), true);
    let vars = [
        ("CONTR_AGENTS_CONFIG", path(&cfg)),
        ("CONTR_BOUNDARY_DIR", path(&kept)),
        ("CONTR_GH", g.as_str()),
    ];
    let core = Fake::default().reply(
        "terminal.run",
        json!({"address": "t9", "name": "w", "state": "live"}),
    );

    // No kept boundary yet: the check passes and says so.
    let r = call(&["pr", "check-boundary", "215"], &vars, &core);
    assert_eq!(
        (r.code, r.out.as_str()),
        (0, "check-boundary: no boundary for PR 215\n")
    );

    let r = call(
        &[
            "agents",
            "spawn",
            "--no-parent",
            "--name",
            "w",
            "--profile",
            "builder",
            "--worktree",
            path(&wt),
            "Do it.\nBoundary: kit\n",
        ],
        &vars,
        &core,
    );
    assert_eq!(r.code, 0, "{}", r.err);
    assert_eq!(
        fs::read_to_string(kept.join("agent__215-boundary-check.txt")).unwrap(),
        "kit\n"
    );
    let r = call(&["pr", "check-boundary", "215"], &vars, &core);
    assert_eq!(r.code, 1);
    assert!(
        r.out.contains("  scripts/land.sh\n") && !r.out.contains("AGENTS.md"),
        "{}",
        r.out
    );

    // A new brief with no boundary removes the kept one.
    assert_eq!(
        call(
            &[
                "agents",
                "spawn",
                "--no-parent",
                "--name",
                "w",
                "--profile",
                "builder",
                "--worktree",
                path(&wt),
                "Fix it."
            ],
            &vars,
            &core
        )
        .code,
        0
    );
    assert!(!kept.join("agent__215-boundary-check.txt").exists());
}
