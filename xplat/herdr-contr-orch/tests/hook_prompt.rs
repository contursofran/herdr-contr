//! The prompt hook and the flow pickup, on a fake hc core. The profiles file and the state folder are in
//! temporary folders.
mod common;

use std::fs;
use std::path::Path;

use common::{call_with, path, Fake, Run};
use orch::flowseen::{flow_state, state_dir, CHANGED_LEAD};
use orch::hook::YOU_CONTEXT;
use serde_json::{json, Value};

/// The profiles file and the state folder of a test.
struct Files {
    _dir: tempfile::TempDir,
    cfg: String,
    state: String,
}

fn files() -> Files {
    let dir = tempfile::tempdir().unwrap();
    let (cfg, state) = (path(&dir.path().join("agents.json")).to_string(), path(&dir.path().join("state")).to_string());
    fs::write(&cfg, "{}").unwrap();
    Files { _dir: dir, cfg, state }
}

impl Files {
    fn vars(&self) -> [(&str, &str); 3] {
        [("HC_TERMINAL_ID", "t1"), ("CONTR_AGENTS_CONFIG", &self.cfg), ("CONTR_STATE_DIR", &self.state)]
    }
    fn prompt(&self, text: &str, core: &Fake) -> Run {
        call_with(&["hook", "prompt"], &self.vars(), core, &json!({"prompt": text}).to_string())
    }
    fn seen(&self) -> String {
        fs::read_to_string(Path::new(&self.state).join("flow-seen/t1")).unwrap_or_default()
    }
}

/// The context that the hook printed, or "" when it printed nothing.
fn context(r: &Run) -> String {
    assert_eq!((r.code, r.err.as_str()), (0, ""));
    if r.out.is_empty() {
        return String::new();
    }
    let v: Value = serde_json::from_str(&r.out).unwrap();
    assert_eq!(v["hookSpecificOutput"]["hookEventName"], "UserPromptSubmit");
    v["hookSpecificOutput"]["additionalContext"].as_str().unwrap().to_string()
}

fn terminal(labels: Value) -> Fake {
    Fake::default().reply("terminal.get", json!({"address": "t1", "name": "lead", "state": "live", "labels": labels}))
}

#[test]
fn a_prompt_of_the_owner_gets_the_todo_line() {
    let f = files();
    let core = Fake::default();
    assert_eq!(context(&f.prompt("fix the test", &core)), YOU_CONTEXT);
    // A message from another terminal and an empty prompt get nothing.
    assert_eq!(context(&f.prompt("  <hc from=\"t4 b-1\">b-1: done</hc>", &core)), "");
    assert_eq!(context(&f.prompt(" ", &core)), "");
    // No flow change: hc is not asked.
    assert!(core.calls().is_empty());
}

#[test]
fn the_hook_does_nothing_outside_an_hc_terminal_and_in_a_worker() {
    let f = files();
    let core = Fake::default();
    let input = json!({"prompt": "fix the test"}).to_string();
    let r = call_with(&["hook", "prompt"], &[("CONTR_AGENTS_CONFIG", f.cfg.as_str()), ("CONTR_STATE_DIR", f.state.as_str())], &core, &input);
    assert_eq!((r.code, r.out.as_str()), (0, ""));
    let worker = [("HC_TERMINAL_ID", "t7"), ("CONTR_COORDINATOR", "t1"), ("CONTR_AGENTS_CONFIG", f.cfg.as_str()), ("CONTR_STATE_DIR", f.state.as_str())];
    let r = call_with(&["hook", "prompt"], &worker, &core, &input);
    assert_eq!((r.code, r.out.as_str()), (0, ""));
    assert!(core.calls().is_empty() && !Path::new(&f.state).exists());
}

#[test]
fn a_changed_flow_reaches_the_coordinator_one_time() {
    let f = files();
    let core = terminal(json!({"contr": "agent", "role": "orchestrator"}));
    // The session start saves the flow of now.
    let r = call_with(&["hook", "session-start"], &f.vars(), &Fake::default(), &json!({"source": "resume"}).to_string());
    assert_eq!((r.code, r.err.as_str()), (0, ""));
    let before = f.seen();
    assert!(before.starts_with("The flow") && before.contains("\nProfiles:\n- builder: model sonnet, effort medium\n"), "{before}");
    assert_eq!(context(&f.prompt("go on", &core)), YOU_CONTEXT);

    // The owner changes a model.
    fs::write(&f.cfg, json!({"profiles": {"builder": {"argv": ["claude", "--model", "opus"]}}}).to_string()).unwrap();
    let got = context(&f.prompt("go on", &core));
    let want = flow_state(&|k| if k == "CONTR_AGENTS_CONFIG" { f.cfg.clone() } else { String::new() }).unwrap();
    assert!(want.contains("- builder: model opus, effort default\n"), "{want}");
    assert_eq!(got, format!("{YOU_CONTEXT}\n\n{CHANGED_LEAD}{want}"));
    assert_eq!(f.seen(), want);
    // A message from a worker gets the flow with no todo line. The next prompt gets no flow again.
    fs::write(&f.cfg, json!({"flow": {"review": "none"}}).to_string()).unwrap();
    let got = context(&f.prompt("<hc from=\"t4 b-1\">b-1: done</hc>", &core));
    assert!(got.starts_with(CHANGED_LEAD) && !got.contains("[you]"), "{got}");
    assert_eq!(context(&f.prompt("go on", &core)), YOU_CONTEXT);
    assert_eq!(core.methods(), ["terminal.get", "terminal.get"]);
}

#[test]
fn a_changed_flow_goes_only_to_a_coordinator() {
    let f = files();
    // With no saved flow, the hook only saves one.
    let core = terminal(json!({"contr": "agent", "role": "builder", "contr.orchestrating": "1"}));
    assert_eq!(context(&f.prompt("go on", &core)), YOU_CONTEXT);
    assert!(core.calls().is_empty() && !f.seen().is_empty());
    // The label of a session that orchestrates counts as the role does.
    fs::write(&f.cfg, json!({"flow": {"review": "none"}}).to_string()).unwrap();
    assert!(context(&f.prompt("go on", &core)).contains(CHANGED_LEAD));
    // A builder's terminal gets no flow, and the change is saved, so hc is asked one time only.
    let builder = terminal(json!({"contr": "agent", "role": "builder"}));
    fs::write(&f.cfg, "{}").unwrap();
    assert_eq!(context(&f.prompt("go on", &builder)), YOU_CONTEXT);
    assert_eq!(context(&f.prompt("go on", &builder)), YOU_CONTEXT);
    assert_eq!(builder.methods(), ["terminal.get"]);
}

#[test]
fn an_error_is_printed_and_the_exit_code_stays_0() {
    let f = files();
    f.prompt("go on", &Fake::default());
    // A broken profiles file: the todo line still comes.
    fs::write(&f.cfg, "{not json").unwrap();
    let r = f.prompt("go on", &Fake::default());
    assert!(r.code == 0 && r.err.starts_with("herdr-contr-orch hook prompt: flow: ") && r.out.contains("[you]"), "{} {}", r.err, r.out);
    // hc does not answer after a change.
    fs::write(&f.cfg, json!({"flow": {"review": "none"}}).to_string()).unwrap();
    let r = f.prompt("go on", &Fake::default().err("terminal.get", "hc is down"));
    assert!(r.code == 0 && r.err == "herdr-contr-orch hook prompt: flow: hc is down\n" && r.out.contains("[you]"), "{} {}", r.err, r.out);
    let r = call_with(&["hook", "prompt"], &f.vars(), &Fake::default(), "not json");
    assert!(r.code == 0 && r.out.is_empty() && r.err.starts_with("herdr-contr-orch hook prompt: bad hook input: "), "{}", r.err);
    // A broken profiles file at the session start is printed too.
    fs::write(&f.cfg, "{not json").unwrap();
    let r = call_with(&["hook", "session-start"], &f.vars(), &Fake::default(), &json!({"source": "resume"}).to_string());
    assert!(r.code == 0 && r.err.starts_with("herdr-contr-orch hook: record flow: "), "{}", r.err);
}

#[test]
fn the_state_folder_comes_from_the_environment() {
    let env = |vars: &'static [(&'static str, &'static str)]| move |k: &str| vars.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string()).unwrap_or_default();
    assert_eq!(state_dir(&env(&[("CONTR_STATE_DIR", "/s"), ("HOME", "/h")])).unwrap(), Path::new("/s"));
    assert_eq!(state_dir(&env(&[])), None);
    let home = state_dir(&env(&[("HOME", "/h"), ("XDG_STATE_HOME", "/x")])).unwrap();
    if cfg!(target_os = "macos") {
        assert_eq!(home, Path::new("/h/Library/Application Support/contr"));
    } else {
        assert_eq!(home, Path::new("/x/contr"));
        assert_eq!(state_dir(&env(&[("HOME", "/h")])).unwrap(), Path::new("/h/.local/state/contr"));
    }
}
