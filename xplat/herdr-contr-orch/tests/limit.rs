//! The limits of spawn and of a message to a settled worker (#212), on a fake hc core and a fake gh.
mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use common::{call, path, Fake};
use serde_json::{json, Value};

fn worker(name: &str, parent: &str, state: &str, status: &str) -> Value {
    json!({"name": name, "address": name, "parent": parent, "state": state, "status": {"state": status}, "labels": {"contr": "agent"}})
}

/// A core with these workers, and a reply to terminal.run.
fn core(ws: Vec<Value>) -> Fake {
    Fake::default()
        .reply("terminal.list", json!({"terminals": ws}))
        .reply("terminal.run", json!({"address": "t9", "name": "w", "state": "live", "labels": {"contr": "agent"}}))
}

/// Two of t1's workers work; others do not count: idle, finished, or another coordinator's.
fn busy() -> Vec<Value> {
    vec![
        worker("b-1", "t1", "live", "working"),
        worker("r-2", "t1", "live", "working"),
        worker("b-3", "t1", "live", "idle"),
        worker("b-4", "t1", "finished", "working"),
        worker("b-5", "t7", "live", "working"),
    ]
}

fn config(dir: &Path, content: Value) -> String {
    let f = dir.join("agents.json");
    fs::write(&f, content.to_string()).unwrap();
    path(&f).to_string()
}

fn ran(core: &Fake) -> bool {
    core.methods().contains(&"terminal.run".to_string())
}

#[test]
fn spawn_refuses_past_the_flows_workers_and_names_them() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = config(tmp.path(), json!({"flow": {"workers": 2}}));
    let vars = [("CONTR_AGENTS_CONFIG", cfg.as_str()), ("HC_TERMINAL_ID", "t1")];
    let c = core(busy());
    let r = call(&["agents", "spawn", "--name", "w", "--profile", "builder", "go"], &vars, &c);
    assert_eq!(r.code, 1);
    assert!(r.err.contains("2 workers at once is the flow's limit; working now: b-1, r-2."), "{}", r.err);
    assert!(!ran(&c));

    // Under the limit (3), the idle, finished and other workers do not count.
    let cfg = config(tmp.path(), json!({"flow": {"workers": 3}}));
    let vars = [("CONTR_AGENTS_CONFIG", cfg.as_str()), ("HC_TERMINAL_ID", "t1")];
    let c = core(busy());
    assert_eq!(call(&["agents", "spawn", "--name", "w", "--profile", "builder", "go"], &vars, &c).code, 0);
    assert!(ran(&c));
}

#[test]
fn a_message_to_a_settled_worker_counts_as_a_start() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = config(tmp.path(), json!({"flow": {"workers": 2}}));
    let vars = [("CONTR_AGENTS_CONFIG", cfg.as_str()), ("HC_TERMINAL_ID", "t1")];
    let send = |w: Value| {
        let c = core(busy()).reply("terminal.get", w);
        let r = call(&["agents", "send", "b-3", "go on"], &vars, &c);
        (r, c.methods().contains(&"terminal.send".to_string()))
    };
    // b-3 is idle: the message would make 3 working.
    let (r, sent) = send(worker("b-3", "t1", "live", "idle"));
    assert!(r.code == 1 && r.err.contains("working now: b-1, r-2") && !sent, "{}", r.err);
    // A working worker, or another coordinator's worker, takes the message.
    assert!(send(worker("b-1", "t1", "live", "working")).1);
    assert!(send(worker("b-5", "t7", "live", "idle")).1);
    // Under the limit, a settled worker takes it.
    let cfg = config(tmp.path(), json!({"flow": {"workers": 3}}));
    let c = core(busy()).reply("terminal.get", worker("b-3", "t1", "live", "idle"));
    assert_eq!(call(&["agents", "send", "b-3", "go"], &[("CONTR_AGENTS_CONFIG", cfg.as_str()), ("HC_TERMINAL_ID", "t1")], &c).code, 0);
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git").arg("-C").arg(dir).args(["-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "commit.gpgsign=false"]).args(args).status().unwrap().success();
    assert!(ok, "git {args:?}");
}

/// A main checkout with a worktree on branch b-1, and a fake gh that prints `prs` (or fails with "").
fn project(tmp: &Path, prs: &str) -> (String, String, String) {
    let main = fs::canonicalize(tmp).unwrap().join("main");
    fs::create_dir(&main).unwrap();
    git(&main, &["init", "--quiet"]);
    git(&main, &["commit", "--quiet", "--allow-empty", "-m", "first"]);
    let wt = main.with_file_name("main-wt");
    git(&main, &["worktree", "add", "--quiet", "-b", "b-1", path(&wt)]);
    let gh = tmp.join("gh");
    let body = if prs.is_empty() { "echo 'gh: no auth' >&2; exit 1".to_string() } else { format!("echo '{prs}'") };
    fs::write(&gh, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&gh, fs::Permissions::from_mode(0o755)).unwrap();
    (path(&main).to_string(), path(&wt).to_string(), path(&gh).to_string())
}

const TWO: &str = r#"[{"number":7,"headRefName":"b-7"},{"number":8,"headRefName":"b-8"}]"#;

fn spawn_in(tmp: &Path, prs: &str, limit: i64, extra: &[&str]) -> (common::Run, bool) {
    let (main, wt, gh) = project(tmp, prs);
    let cfg = config(tmp, json!({"open_prs": {main: limit}}));
    let vars = [("CONTR_AGENTS_CONFIG", cfg.as_str()), ("HC_TERMINAL_ID", "t1"), ("CONTR_GH", gh.as_str())];
    let c = core(vec![]);
    let mut args = vec!["agents", "spawn", "--name", "w", "--profile", "builder", "--worktree", &wt];
    args.extend_from_slice(extra);
    args.push("go");
    let r = call(&args, &vars, &c);
    (r, ran(&c))
}

#[test]
fn spawn_refuses_a_new_issue_at_the_projects_open_pr_limit() {
    let tmp = tempfile::tempdir().unwrap();
    let (r, ran) = spawn_in(tmp.path(), TWO, 2, &[]);
    assert!(r.code == 1 && !ran, "{}", r.err);
    assert!(r.err.contains("2 open PRs is the project's limit (#7, #8)"), "{}", r.err);
}

#[test]
fn spawn_allows_a_new_issue_under_the_open_pr_limit() {
    let tmp = tempfile::tempdir().unwrap();
    let (r, ran) = spawn_in(tmp.path(), TWO, 3, &[]);
    assert!(r.code == 0 && ran, "{}", r.err);
}

#[test]
fn spawn_allows_a_fix_round_at_the_open_pr_limit() {
    let tmp = tempfile::tempdir().unwrap();
    let (r, ran) = spawn_in(tmp.path(), TWO, 1, &["--fix-pr"]);
    assert!(r.code == 0 && ran, "{}", r.err);
    // The worktree's branch b-1 has an open PR: a fix round with no flag.
    let tmp = tempfile::tempdir().unwrap();
    let (r, ran) = spawn_in(tmp.path(), r#"[{"number":1,"headRefName":"b-1"}]"#, 1, &[]);
    assert!(r.code == 0 && ran, "{}", r.err);
}

#[test]
fn spawn_names_a_failed_pr_count_and_skips_a_project_with_no_limit() {
    let tmp = tempfile::tempdir().unwrap();
    let (r, started) = spawn_in(tmp.path(), "", 2, &[]);
    assert!(r.code == 1 && !started && r.err.contains("gh: no auth"), "{}", r.err);
    // No open_prs for this project: gh does not run (it would fail).
    let tmp = tempfile::tempdir().unwrap();
    let (_, wt, gh) = project(tmp.path(), "");
    let cfg = config(tmp.path(), json!({"open_prs": {"/elsewhere": 1}}));
    let c = core(vec![]);
    let r = call(&["agents", "spawn", "--name", "w", "--profile", "builder", "--worktree", &wt, "go"], &[("CONTR_AGENTS_CONFIG", cfg.as_str()), ("CONTR_GH", gh.as_str())], &c);
    assert!(r.code == 0 && ran(&c), "{}", r.err);
}

#[test]
fn an_open_pr_limit_under_one_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = config(tmp.path(), json!({"open_prs": {"/x": 0}}));
    let r = call(&["agents", "spawn", "--name", "w", "--profile", "builder", "go"], &[("CONTR_AGENTS_CONFIG", cfg.as_str())], &Fake::default());
    assert!(r.code == 1 && r.err.contains("open_prs \"/x\": 0 is less than 1"), "{}", r.err);
}
