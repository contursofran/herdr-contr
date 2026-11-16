//! `agents resume` (#210) on a fake hc core, with the state folder in a temporary folder. No test starts an agent.
mod common;

use std::fs;

use common::{call, path, Fake};
use orch::resume::RESUME_TEXT;
use serde_json::{json, Value};

fn argv(params: &Value) -> Vec<String> {
    params["argv"].as_array().unwrap().iter().map(|a| a.as_str().unwrap().to_string()).collect()
}

fn run_params(core: &Fake) -> Value {
    core.calls().into_iter().find(|(m, _)| m == "terminal.run").expect("a run").1
}

fn after<'a>(full: &'a [String], flag: &str) -> Option<&'a str> {
    full.iter().position(|a| a == flag).map(|i| full[i + 1].as_str())
}

#[test]
fn a_gone_worker_resumes_from_its_recorded_session() {
    let tmp = tempfile::tempdir().unwrap();
    let state = tmp.path().join("state");
    let cfg = tmp.path().join("agents.json");
    fs::write(&cfg, "{}").unwrap();
    let wt = path(tmp.path());
    let vars = [("HC_TERMINAL_ID", "t1"), ("CONTR_AGENTS_CONFIG", path(&cfg)), ("CONTR_STATE_DIR", path(&state))];
    let live = json!({"address": "t4", "name": "b-210", "state": "live", "parent": "t1", "labels": {"contr": "agent", "role": "builder", "profile": "builder", "issue": "210"}});

    // Spawn gives a session id and keeps it with the worker.
    let core = Fake::default().reply("terminal.run", live.clone());
    let r = call(&["agents", "spawn", "--name", "b-210", "--profile", "builder", "--issue", "210", "--worktree", wt, "--title", "Resume", "go"], &vars, &core);
    assert_eq!(r.code, 0, "{}", r.err);
    let session = after(&argv(&run_params(&core)), "--session-id").unwrap().to_string();
    assert_eq!(session.len(), 36);
    let rec: Value = serde_json::from_slice(&fs::read(state.join("workers/b-210.json")).unwrap()).unwrap();
    assert_eq!((&rec["session"], &rec["worktree"], &rec["issue"], &rec["profile"], &rec["parent"]), (&json!(session), &json!(wt), &json!("210"), &json!("builder"), &json!("t1")));

    // A live worker is not gone, and cannot resume.
    let core = Fake::default().reply("terminal.list", json!({"terminals": [live]}));
    assert!(!call(&["agents", "ls"], &vars, &core).out.contains("gone"));
    let r = call(&["agents", "resume", "b-210"], &vars, &core);
    assert_eq!((r.code, r.err.contains("still live")), (1, true), "{}", r.err);

    // With no terminal, ls shows it as gone.
    let core = Fake::default().reply("terminal.run", json!({"address": "t9", "name": "b-210", "state": "live"}));
    let r = call(&["agents", "ls"], &vars, &core);
    assert!(r.out.contains("b-210") && r.out.contains("gone (agents resume)"), "{}", r.out);

    // Resume starts the same profile in the same worktree with the recorded session.
    let r = call(&["agents", "resume", "b-210"], &vars, &core);
    assert_eq!((r.code, r.out.as_str(), r.err.as_str()), (0, "b-210 t9\n", ""));
    let p = run_params(&core);
    let full = argv(&p);
    assert_eq!(after(&full, "--resume"), Some(session.as_str()));
    assert_eq!(after(&full, "--session-id"), None);
    assert_eq!(full.last().unwrap(), RESUME_TEXT);
    assert_eq!((&p["cwd"], &p["parent"], &p["labels"]["issue"], &p["labels"]["title"]), (&json!(wt), &json!("t1"), &json!("210"), &json!("Resume")));

    // Stop forgets the worker: it is not gone, and cannot resume.
    let core = Fake::default().reply("terminal.get", json!({"name": "b-210", "state": "finished"}));
    assert_eq!(call(&["agents", "stop", "b-210"], &vars, &core).code, 0);
    assert!(!state.join("workers/b-210.json").exists());
    assert!(!call(&["agents", "ls"], &vars, &Fake::default()).out.contains("b-210"));
}

#[test]
fn a_finished_terminal_without_a_stop_is_gone() {
    let tmp = tempfile::tempdir().unwrap();
    let state = tmp.path().join("state");
    fs::create_dir_all(state.join("workers")).unwrap();
    fs::write(state.join("workers/b-1.json"), json!({"name": "b-1", "session": "s", "profile": "builder", "parent": "t1"}).to_string()).unwrap();
    fs::write(state.join("workers/b-2.json"), json!({"name": "b-2", "session": "s", "profile": "builder", "parent": "t7"}).to_string()).unwrap();
    let core = Fake::default().reply("terminal.list", json!({"terminals": [{"address": "t4", "name": "b-1", "state": "finished", "exit_code": 1, "labels": {"contr": "agent"}}]}));
    let r = call(&["agents", "ls", "--all", "--json"], &[("HC_TERMINAL_ID", "t1"), ("CONTR_STATE_DIR", path(&state))], &core);
    let ws: Value = serde_json::from_str(&r.out).unwrap();
    // Only the caller's own records count.
    assert_eq!(ws.as_array().unwrap().len(), 1);
    assert_eq!((&ws[0]["name"], &ws[0]["status"], &ws[0]["address"]), (&json!("b-1"), &json!("gone"), &json!("t4")));
}

#[test]
fn an_unknown_name_or_the_limit_refuses_a_resume() {
    let tmp = tempfile::tempdir().unwrap();
    let state = tmp.path().join("state");
    let cfg = tmp.path().join("agents.json");
    fs::write(&cfg, json!({"flow": {"workers": 1}}).to_string()).unwrap();
    let vars = [("HC_TERMINAL_ID", "t1"), ("CONTR_AGENTS_CONFIG", path(&cfg)), ("CONTR_STATE_DIR", path(&state))];
    let core = Fake::default();
    let r = call(&["agents", "resume", "nobody"], &vars, &core);
    assert_eq!(r.code, 1);
    assert!(r.err.contains("no record of a worker \"nobody\""), "{}", r.err);
    assert!(!core.methods().contains(&"terminal.run".to_string()));

    // A resumed worker counts against the workers at once.
    fs::create_dir_all(state.join("workers")).unwrap();
    fs::write(state.join("workers/b-1.json"), json!({"name": "b-1", "session": "s", "profile": "builder", "parent": "t1"}).to_string()).unwrap();
    let core = Fake::default().reply("terminal.list", json!({"terminals": [{"address": "t5", "name": "b-9", "state": "live", "parent": "t1", "status": {"state": "working"}, "labels": {"contr": "agent"}}]}));
    let r = call(&["agents", "resume", "b-1"], &vars, &core);
    assert_eq!(r.code, 1);
    assert!(r.err.contains("workers at once"), "{}", r.err);
    assert!(!core.methods().contains(&"terminal.run".to_string()));
}

#[test]
fn the_mcp_resume_tool_resumes_a_gone_worker() {
    let tmp = tempfile::tempdir().unwrap();
    let state = tmp.path().join("state");
    let cfg = tmp.path().join("agents.json");
    fs::write(&cfg, "{}").unwrap();
    fs::create_dir_all(state.join("workers")).unwrap();
    fs::write(state.join("workers/b-1.json"), json!({"name": "b-1", "session": "abc", "profile": "builder", "parent": "t1"}).to_string()).unwrap();
    let own = tmp.path().join("owner.json");
    let vars = [("HC_TERMINAL_ID", "t1"), ("CONTR_AGENTS_CONFIG", path(&cfg)), ("CONTR_STATE_DIR", path(&state)), ("CONTR_OWNER_FILE", path(&own))];
    let core = Fake::default().reply("terminal.run", json!({"address": "t9", "name": "b-1", "state": "live"}));
    let input = [
        json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "resume", "arguments": {"name": "b-1"}}}),
    ]
    .iter()
    .map(|m| m.to_string() + "\n")
    .collect::<String>();
    let r = common::call_with(&["mcp"], &vars, &core, &input);
    assert!(r.out.contains("t9") && !r.out.contains("isError\":true"), "{} {}", r.out, r.err);
    assert_eq!(after(&argv(&run_params(&core)), "--resume"), Some("abc"));
}
