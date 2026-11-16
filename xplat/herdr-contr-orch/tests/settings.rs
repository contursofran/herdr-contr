//! `agents config --json`, `agents set` and `owner open --json` on files in a temporary folder.
mod common;

use std::fs;
use std::path::Path;

use common::{call, path, Fake};
use orch::owner::{Question, Store, Todo};
use serde_json::{json, Value};

fn config(cfg: &Path) -> Value {
    let r = call(&["agents", "config", "--json"], &[("CONTR_AGENTS_CONFIG", path(cfg))], &Fake::default());
    assert_eq!((r.code, r.err.as_str()), (0, ""));
    serde_json::from_str(&r.out).unwrap()
}

fn set(cfg: &Path, args: &[&str]) -> common::Run {
    let mut all = vec!["agents", "set"];
    all.extend_from_slice(args);
    call(&all, &[("CONTR_AGENTS_CONFIG", path(cfg))], &Fake::default())
}

fn file(cfg: &Path) -> Value {
    serde_json::from_slice(&fs::read(cfg).unwrap()).unwrap()
}

#[test]
fn config_of_a_missing_file_gives_the_defaults_and_the_valid_values() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("agents.json");
    let v = config(&cfg);
    assert_eq!(v["path"], path(&cfg));
    assert_eq!(v["bad"], false);
    assert_eq!(v["flow"], json!({"plan": "when-needed", "review": "code", "debugger_after": 2, "workers": 4, "merge": "ask"}));
    assert_eq!(v["fields"]["plan"], json!({"default": "when-needed", "values": ["never", "when-needed", "always"]}));
    assert_eq!(v["fields"]["review"]["values"], json!(["none", "code", "every"]));
    assert_eq!(v["fields"]["merge"]["values"], json!(["ask", "auto"]));
    assert_eq!(v["fields"]["workers"], json!({"default": 4, "min": 1}));
    assert_eq!(v["fields"]["debugger_after"], json!({"default": 2, "min": 1}));
    let names: Vec<&str> = v["roles"].as_array().unwrap().iter().map(|r| r["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["orchestrator", "planner", "builder", "reviewer", "debugger"]);
    assert_eq!(v["roles"][0], json!({"name": "orchestrator", "claude": true, "model": "opus", "effort": "medium",
        "argv": ["claude", "--model", "opus", "--effort", "medium", "--permission-mode", "auto"]}));
    assert!(!cfg.exists());
}

#[test]
fn config_uses_the_file_and_a_value_that_is_not_valid_gives_the_default() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("agents.json");
    fs::write(&cfg, r#"{"flow":{"plan":"never","review":"all","workers":0,"debugger_after":3},
        "profiles":{"builder":{"argv":["env","A=1","claude","--model=haiku"]},"reviewer":{"argv":["codex","--model","x"]},"planner":{"argv":[]}}}"#).unwrap();
    let v = config(&cfg);
    assert_eq!(v["flow"], json!({"plan": "never", "review": "code", "debugger_after": 3, "workers": 4, "merge": "ask"}));
    let role = |i: usize| v["roles"][i].clone();
    assert_eq!((role(2)["claude"].clone(), role(2)["model"].clone(), role(2)["effort"].clone()), (json!(true), json!("haiku"), json!("")));
    assert_eq!((role(3)["claude"].clone(), role(3)["model"].clone()), (json!(false), json!("")));
    assert_eq!(role(3)["argv"], json!(["codex", "--model", "x"]));
    assert_eq!(role(1)["model"], "opus", "an empty argv gives the built-in one");
}

#[test]
fn config_of_a_file_that_is_not_an_object_is_bad() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("agents.json");
    for text in ["not json", "[1]"] {
        fs::write(&cfg, text).unwrap();
        let v = config(&cfg);
        assert_eq!(v["bad"], true, "{text}");
        assert_eq!(v["flow"]["plan"], "when-needed");
    }
}

#[test]
fn config_needs_json_and_set_needs_pairs() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("agents.json");
    let r = call(&["agents", "config"], &[("CONTR_AGENTS_CONFIG", path(&cfg))], &Fake::default());
    assert_eq!(r.code, 2);
    assert!(r.err.starts_with("herdr-contr-orch agents config: give --json\n"));
    for args in [&[][..], &["flow.plan"][..]] {
        let r = set(&cfg, args);
        assert_eq!(r.code, 2, "{args:?}");
        assert!(r.err.starts_with("herdr-contr-orch agents set: give pairs"));
    }
    assert!(!cfg.exists());
}

#[test]
fn set_creates_a_missing_file_with_the_flow_whole() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("sub/agents.json");
    let r = set(&cfg, &["flow.merge", "auto", "flow.workers", "2"]);
    assert_eq!((r.code, r.err.as_str()), (0, ""));
    assert_eq!(file(&cfg), json!({"flow": {"plan": "when-needed", "review": "code", "debugger_after": 2, "workers": 2, "merge": "auto"}}));
    assert!(fs::read_to_string(&cfg).unwrap().ends_with("}\n"));
}

#[test]
fn set_keeps_the_other_keys() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("agents.json");
    fs::write(&cfg, r#"{"other":[1],"kit":"/k","flow":{"plan":"never","profiles":{"builder":"b2"},"extra":true},
        "profiles":{"builder":{"argv":["claude","-p","--model","opus","x"],"agent":"builder"},"b2":{"argv":["claude"]}}}"#).unwrap();
    let r = set(&cfg, &["flow.review", "none", "profile.builder.effort", "low", "profile.planner.model", "haiku"]);
    assert_eq!((r.code, r.err.as_str()), (0, ""));
    let v = file(&cfg);
    assert_eq!((v["other"].clone(), v["kit"].clone()), (json!([1]), json!("/k")));
    assert_eq!(v["flow"], json!({"plan": "never", "review": "none", "debugger_after": 2, "workers": 4, "merge": "ask", "profiles": {"builder": "b2"}, "extra": true}));
    assert_eq!(v["profiles"]["builder"], json!({"argv": ["claude", "-p", "--model", "opus", "--effort", "low", "x"], "agent": "builder"}));
    assert_eq!(v["profiles"]["b2"], json!({"argv": ["claude"]}));
    assert_eq!(v["profiles"]["planner"]["argv"], json!(["claude", "--model", "haiku", "--effort", "low", "--permission-mode", "auto"]));
}

#[test]
fn set_leaves_a_role_that_is_not_claude() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("agents.json");
    let text = r#"{"profiles":{"reviewer":{"argv":["codex","--model","x"]}}}"#;
    fs::write(&cfg, text).unwrap();
    let r = set(&cfg, &["profile.reviewer.model", "opus", "profile.reviewer.effort", "low"]);
    assert_eq!((r.code, r.err.as_str()), (0, ""));
    assert_eq!(file(&cfg), serde_json::from_str::<Value>(text).unwrap());
}

#[test]
fn set_refuses_a_bad_file_an_unknown_key_and_a_value_that_is_not_valid() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("agents.json");
    fs::write(&cfg, "[1]").unwrap();
    let r = set(&cfg, &["flow.merge", "auto"]);
    assert_eq!(r.code, 1);
    assert!(r.err.contains("not a JSON object"), "{}", r.err);
    assert_eq!(fs::read_to_string(&cfg).unwrap(), "[1]");
    let text = r#"{"flow":{"plan":"sometimes"}}"#;
    fs::write(&cfg, text).unwrap();
    for (args, want) in [
        (&["flow.color", "red"][..], "unknown key flow.color"),
        (&["profile.explorer.model", "opus"][..], "unknown key profile.explorer.model"),
        (&["profile.builder.argv", "x"][..], "unknown key profile.builder.argv"),
        (&["flow.merge", "yes"][..], "flow merge: \"yes\" is not one of ask, auto"),
        (&["flow.workers", "0"][..], "flow workers: 0 is less than 1"),
        (&["flow.debugger_after", "two"][..], "flow.debugger_after: \"two\" is not a number"),
        (&["profile.builder.model", ""][..], "is not a valid value"),
        (&["flow.merge", "auto", "flow.review", "all"][..], "flow review"),
    ] {
        let r = set(&cfg, args);
        assert_eq!(r.code, 1, "{args:?}");
        assert!(r.err.starts_with("herdr-contr-orch agents set: agents: ") && r.err.contains(want), "{args:?}: {}", r.err);
        assert_eq!(fs::read_to_string(&cfg).unwrap(), text, "{args:?} wrote the file");
    }
    // Another key that is not valid does not block a valid change.
    assert_eq!(set(&cfg, &["flow.merge", "auto"]).code, 0);
    assert_eq!(file(&cfg)["flow"]["plan"], "sometimes");
}

#[test]
fn owner_open_lists_the_questions_with_no_answer_oldest_first() {
    let dir = tempfile::tempdir().unwrap();
    let owner = dir.path().join("owner.json");
    let vars = [("CONTR_OWNER_FILE", path(&owner))];
    let r = call(&["owner", "open", "--json"], &vars, &Fake::default());
    assert_eq!((r.code, r.out.as_str(), r.err.as_str()), (0, "[]\n", ""));
    let store = Store::new(&owner);
    let ask = |q: &str| store.ask(Question { question: q.into(), coordinator: "hc:t1".into(), ..Question::default() }).unwrap();
    let first = Question {
        question: "Which?".into(), coordinator: "hc:t2".into(), title: "Pick one".into(), from: "#12".into(),
        options: vec!["A".into(), "B".into()], impacts: vec!["a".into(), "b".into()], recommended: Some(1), refs: vec!["PR 3".into()],
        ..Question::default()
    };
    store.ask(first).unwrap();
    ask("Answered?");
    ask("Third?");
    store.decide(&["D2".into()], "yes", "owner").unwrap();
    let r = call(&["owner", "open", "--json"], &vars, &Fake::default());
    assert_eq!((r.code, r.err.as_str()), (0, ""));
    let v: Value = serde_json::from_str(&r.out).unwrap();
    assert_eq!(v[0], json!({"kind": "question", "id": "D1", "coordinator": "hc:t2", "title": "Pick one", "from": "#12", "question": "Which?",
        "options": ["A", "B"], "impacts": ["a", "b"], "recommended": 1, "refs": ["PR 3"]}));
    assert_eq!(v.as_array().unwrap().len(), 2);
    assert_eq!((v[1]["id"].clone(), v[1]["recommended"].clone()), (json!("D3"), Value::Null));
}

#[test]
fn owner_open_lists_the_open_todos_after_the_questions_oldest_first() {
    let dir = tempfile::tempdir().unwrap();
    let owner = dir.path().join("owner.json");
    let vars = [("CONTR_OWNER_FILE", path(&owner))];
    let store = Store::new(&owner);
    let pin = |text: &str, kind: &str| {
        store.pin(Todo { project: "app".into(), coordinator: "hc:t1".into(), from: "builder #9".into(), kind: kind.into(), text: text.into(), ..Todo::default() }).unwrap()
    };
    pin("Test `make app`", "");
    pin("Done already", "");
    pin("CI is slow", "know");
    store.ask(Question { question: "Which?".into(), coordinator: "hc:t1".into(), ..Question::default() }).unwrap();
    store.unpin(&["T2".into()]).unwrap();
    let r = call(&["owner", "open", "--json"], &vars, &Fake::default());
    assert_eq!((r.code, r.err.as_str()), (0, ""));
    let v: Value = serde_json::from_str(&r.out).unwrap();
    let kinds: Vec<_> = v.as_array().unwrap().iter().map(|i| (i["kind"].clone(), i["id"].clone())).collect();
    assert_eq!(kinds, [(json!("question"), json!("D1")), (json!("todo"), json!("T1")), (json!("todo"), json!("T3"))]);
    assert_eq!(v[1], json!({"kind": "todo", "id": "T1", "coordinator": "hc:t1", "from": "builder #9", "todo_kind": "do", "text": "Test `make app`"}));
    assert_eq!(v[2]["todo_kind"], "know");
    // A to-do marked done leaves the list.
    assert_eq!(call(&["owner", "done", "T1"], &vars, &Fake::default()).code, 0);
    let v: Value = serde_json::from_str(&call(&["owner", "open", "--json"], &vars, &Fake::default()).out).unwrap();
    assert_eq!(v.as_array().unwrap().iter().map(|i| i["id"].clone()).collect::<Vec<_>>(), [json!("D1"), json!("T3")]);
    let r = call(&["owner", "done", "T1"], &vars, &Fake::default());
    assert_eq!((r.code, r.err.as_str()), (1, "herdr-contr-orch owner done: owner: no open to-do T1\n"), "not an open to-do");
}

#[test]
fn owner_open_needs_json_and_a_readable_file() {
    let dir = tempfile::tempdir().unwrap();
    let owner = dir.path().join("owner.json");
    let vars = [("CONTR_OWNER_FILE", path(&owner))];
    let r = call(&["owner", "open"], &vars, &Fake::default());
    assert_eq!(r.code, 2);
    assert!(r.err.starts_with("herdr-contr-orch owner open: give --json\n"));
    fs::write(&owner, "not json").unwrap();
    let r = call(&["owner", "open", "--json"], &vars, &Fake::default());
    assert_eq!(r.code, 1);
    assert!(r.err.starts_with("herdr-contr-orch owner open: owner: "), "{}", r.err);
}
