//! A worker's report to a busy coordinator (#199), on a fake hc core: hc refuses the send, the tool holds the
//! report, and the coordinator's Stop hook or prompt hook gives it later. The state folder is temporary.
mod common;

use std::fs;
use std::path::Path;

use common::{call_with, path, Fake, Run};
use orch::held::{self, Report, HELD_LEAD};
use serde_json::{json, Value};

const PROMPT: &str = r#"{"type":"user","message":{"role":"user","content":"do the task"}}"#;
const WORDS: &str = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"PR #5 is open."}]}}"#;
const SEND: &str = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"s1","name":"Bash","input":{"command":"hc send \"$CONTR_COORDINATOR\" \"b1: done\""}}]}}"#;
const BLOCKED: &str = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"s1","is_error":true,"content":"Permission for this action has been denied. Reason: Unsanctioned Communication"}]}}"#;

/// A core where the coordinator shows a dialog: hc refuses each send. It knows the worker t7 as b1.
fn busy() -> Fake {
    Fake::default().reply("terminal.send", json!({"sent": false, "reason": "blocked"})).reply("terminal.get", json!({"name": "b1", "address": "t7"}))
}

fn transcript(dir: &Path, lines: &[&str]) -> String {
    let p = dir.join("t.jsonl");
    fs::write(&p, lines.join("\n")).unwrap();
    path(&p).to_string()
}

/// The coordinator's Stop hook in terminal t1.
fn coordinator_stop(state: &str, dir: &Path, core: &Fake) -> Run {
    let input = json!({"transcript_path": transcript(dir, &[PROMPT, WORDS])}).to_string();
    call_with(&["hook", "stop"], &[("HC_TERMINAL_ID", "t1"), ("CONTR_STATE_DIR", state)], core, &input)
}

fn sends(core: &Fake) -> Vec<Value> {
    core.calls().into_iter().filter(|(m, _)| m == "terminal.send").map(|(_, p)| p).collect()
}

#[test]
fn a_refused_note_is_held_and_the_coordinators_stop_gives_it_later() {
    let (dir, state) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let core = busy();
    let worker = [("CONTR_COORDINATOR", "t1"), ("HC_TERMINAL_ID", "t7"), ("CONTR_STATE_DIR", path(state.path()))];
    let input = json!({"transcript_path": transcript(dir.path(), &[PROMPT, WORDS])}).to_string();
    let r = call_with(&["hook", "stop"], &worker, &core, &input);
    assert_eq!((r.code, r.err.as_str()), (0, ""), "a held note is no error");
    assert_eq!(sends(&core).len(), 1);
    assert_eq!(fs::read_dir(state.path().join("held/t1")).unwrap().count(), 1);

    // The coordinator's turn ends: its Stop hook keeps the turn going with the held note, one time.
    let later = Fake::default();
    let r = coordinator_stop(path(state.path()), dir.path(), &later);
    assert_eq!((r.code, r.err.as_str()), (0, ""));
    let out: Value = serde_json::from_str(r.out.trim()).unwrap();
    let want = format!("{HELD_LEAD}\n<hc from=\"t7 b1\">ended its turn without a report. Last words: PR #5 is open.</hc>");
    assert_eq!(out, json!({"decision": "block", "reason": want}));
    assert!(later.calls().is_empty(), "the held note is all that this Stop does");
    let again = coordinator_stop(path(state.path()), dir.path(), &later);
    assert_eq!((again.code, again.out.as_str()), (0, ""), "a note is given one time");
}

#[test]
fn a_send_that_auto_mode_blocked_gets_the_note() {
    let dir = tempfile::tempdir().unwrap();
    let core = Fake::default();
    let input = json!({"transcript_path": transcript(dir.path(), &[PROMPT, SEND, BLOCKED, WORDS])}).to_string();
    let r = call_with(&["hook", "stop"], &[("CONTR_COORDINATOR", "t1"), ("HC_TERMINAL_ID", "t7")], &core, &input);
    assert_eq!((r.code, r.err.as_str()), (0, ""));
    assert_eq!(sends(&core)[0]["text"], "ended its turn without a report. Last words: PR #5 is open.");
}

#[test]
fn the_prompt_hook_gives_held_reports() {
    let state = tempfile::tempdir().unwrap();
    let env = |k: &str| if k == "CONTR_STATE_DIR" { path(state.path()).to_string() } else { String::new() };
    held::hold(&env, "t1", &Report { from: "t7 b1".into(), text: "b1: done, PR #5".into() }).unwrap();
    held::hold(&env, "t1", &Report { from: "t8 b2".into(), text: "b2: blocked".into() }).unwrap();
    let vars = [("HC_TERMINAL_ID", "t1"), ("CONTR_STATE_DIR", path(state.path())), ("CONTR_AGENTS_CONFIG", "/none/agents.json")];
    let r = call_with(&["hook", "prompt"], &vars, &Fake::default(), r#"{"prompt":"<hc from=\"t9 b3\">b3: done</hc>"}"#);
    assert_eq!(r.code, 0);
    let out: Value = serde_json::from_str(r.out.trim()).unwrap();
    let want = format!("{HELD_LEAD}\n<hc from=\"t7 b1\">b1: done, PR #5</hc>\n<hc from=\"t8 b2\">b2: blocked</hc>");
    assert_eq!(out["hookSpecificOutput"]["additionalContext"], want, "oldest first");
    assert!(held::take(&env, "t1").unwrap().is_empty());
    // A bad hook input takes nothing, so nothing is lost.
    held::hold(&env, "t1", &Report { from: "t7".into(), text: "x".into() }).unwrap();
    assert_eq!(call_with(&["hook", "prompt"], &vars, &Fake::default(), "not json").code, 0);
    assert_eq!(held::take(&env, "t1").unwrap().len(), 1);
}

#[test]
fn a_note_that_cannot_be_held_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let input = json!({"transcript_path": transcript(dir.path(), &[PROMPT, WORDS])}).to_string();
    // No state folder: no CONTR_STATE_DIR, HERDR_APP_HOME or HOME.
    let r = call_with(&["hook", "stop"], &[("CONTR_COORDINATOR", "t1"), ("HC_TERMINAL_ID", "t7")], &busy(), &input);
    assert!(r.code == 1 && r.err.contains("t1 did not take the report (blocked), and it was not held: held: no state folder"), "{}", r.err);
}

#[test]
fn take_skips_what_is_not_a_report() {
    let state = tempfile::tempdir().unwrap();
    let env = |k: &str| if k == "CONTR_STATE_DIR" { path(state.path()).to_string() } else { String::new() };
    assert!(held::take(&env, "t1").unwrap().is_empty(), "no folder: no reports");
    assert_eq!(held::text(&[]), "");
    let dir = state.path().join("held/t1");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("0-bad.json"), "not json").unwrap();
    fs::write(dir.join(".1.tmp"), "{}").unwrap();
    held::hold(&env, "t1", &Report { from: "t7".into(), text: "ok".into() }).unwrap();
    assert_eq!(held::take(&env, "t1").unwrap(), [Report { from: "t7".into(), text: "ok".into() }]);
    assert!(!dir.join("0-bad.json").exists() && dir.join(".1.tmp").exists(), "a bad report is removed; a file being written stays");
    assert!(held::take(&env, "..").is_err());
}

/// The replies of the MCP server to these tool calls.
fn mcp(calls: &[Value], vars: &[(&str, &str)], core: &Fake) -> Vec<(String, bool)> {
    let dir = tempfile::tempdir().unwrap();
    let (cfg, owner) = (dir.path().join("agents.json"), dir.path().join("owner.json"));
    let mut all = vec![("CONTR_AGENTS_CONFIG", path(&cfg)), ("CONTR_OWNER_FILE", path(&owner))];
    all.extend_from_slice(vars);
    let input: String = calls.iter().map(|c| format!("{c}\n")).collect();
    let r = call_with(&["mcp"], &all, core, &input);
    assert_eq!((r.code, r.err.as_str()), (0, ""));
    r.out.lines().map(|l| serde_json::from_str::<Value>(l).unwrap()).map(|v| (v["result"]["content"][0]["text"].as_str().unwrap().to_string(), v["result"]["isError"] == true)).collect()
}

fn report(text: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "report", "arguments": {"text": text}}})
}

#[test]
fn the_report_tool_sends_or_holds() {
    let state = tempfile::tempdir().unwrap();
    let worker = [("CONTR_COORDINATOR", "t1"), ("HC_TERMINAL_ID", "t7"), ("CONTR_STATE_DIR", path(state.path()))];
    let free = Fake::default();
    assert_eq!(mcp(&[report("b1: done")], &worker, &free), [("sent".to_string(), false)]);
    assert_eq!(sends(&free), [json!({"terminal": "t1", "caller": "t7", "text": "b1: done"})]);
    assert!(!state.path().join("held").exists());

    let got = mcp(&[report("b1: done")], &worker, &busy());
    assert_eq!(got, [("held: t1 did not take it now (blocked); contr gives it to t1 when its prompt is free".to_string(), false)]);
    let r = coordinator_stop(path(state.path()), tempfile::tempdir().unwrap().path(), &Fake::default());
    assert!(r.out.contains(r#"<hc from=\"t7 b1\">b1: done</hc>"#), "{}", r.out);
}

#[test]
fn the_report_tool_errors() {
    let got = mcp(&[report("b1: done")], &[("HC_TERMINAL_ID", "t7")], &Fake::default());
    assert_eq!(got, [("report: no coordinator: CONTR_COORDINATOR is not set".to_string(), true)]);
    let worker = [("CONTR_COORDINATOR", "t1"), ("HC_TERMINAL_ID", "t7")];
    let got = mcp(&[report("b1: done")], &worker, &Fake::default().err("terminal.send", "hc: not_found"));
    assert_eq!(got, [("report: hc: not_found".to_string(), true)]);
    assert_eq!(mcp(&[report(" ")], &worker, &Fake::default()), [("report: agents: no text to send".to_string(), true)]);
}
