//! The command line on files in a temporary folder. No test reads the owner's real files.
mod common;

use std::fs;

use common::{call, path, Fake};
use orch::cli::{AGENTS_USAGE, OWNER_USAGE, USAGE};
use orch::owner::{Store, Todo};
use serde_json::json;

#[test]
fn help_and_unknown_arguments() {
    let r = call(&["--help"], &[], &Fake::default());
    assert_eq!((r.code, r.out.as_str(), r.err.as_str()), (0, USAGE, ""));
    let r = call(&[], &[], &Fake::default());
    assert_eq!((r.code, r.out.as_str(), r.err.as_str()), (2, "", USAGE));
    let r = call(&["window"], &[], &Fake::default());
    assert_eq!(r.code, 2);
    assert!(r.err.starts_with("herdr-contr-orch: unknown argument window\n"));
}

#[test]
fn agents_flow_prints_the_flow_of_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("agents.json");
    fs::write(&cfg, r#"{"flow":{"plan":"never","review":"none","merge":"auto","profiles":{"builder":"builder-medium"}}}"#).unwrap();
    let r = call(&["agents", "flow"], &[("CONTR_AGENTS_CONFIG", path(&cfg))], &Fake::default());
    assert_eq!((r.code, r.err.as_str()), (0, ""));
    assert!(r.out.starts_with("The flow:\n- Plan first: never."), "{}", r.out);
    assert!(r.out.contains("- Merge: merge yourself when CI and review pass.\n"));
    assert!(r.out.contains("- Profiles: builder: builder-medium, planner: planner,"));
}

#[test]
fn agents_flow_with_no_file_prints_the_default_flow() {
    let dir = tempfile::tempdir().unwrap();
    let r = call(&["agents", "flow"], &[("CONTR_AGENTS_CONFIG", path(&dir.path().join("none.json")))], &Fake::default());
    assert_eq!(r.code, 0);
    assert!(r.out.contains("Plan first: when needed.") && r.out.contains("Merge: ask the owner each time."));
}

#[test]
fn agents_flow_with_a_bad_file_is_an_error_and_help_still_gives_the_default_flow() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("agents.json");
    fs::write(&cfg, r#"{"flow":{"workers":0}}"#).unwrap();
    let vars = [("CONTR_AGENTS_CONFIG", path(&cfg))];
    let r = call(&["agents", "flow"], &vars, &Fake::default());
    assert_eq!((r.code, r.out.as_str()), (1, ""));
    assert!(r.err.contains("flow workers: 0 is less than 1"), "{}", r.err);
    let r = call(&["agents", "help"], &vars, &Fake::default());
    assert_eq!(r.code, 0);
    assert!(r.out.starts_with(AGENTS_USAGE));
    assert!(r.out.contains("The flow could not be read (") && r.out.contains("the default flow follows.\nThe flow:\n- Plan first: when needed."));
}

#[test]
fn agents_help_says_when_the_file_is_in_the_old_format() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("agents.json");
    fs::write(&cfg, r#"{"presets":[]}"#).unwrap();
    let r = call(&["agents", "--help"], &[("CONTR_AGENTS_CONFIG", path(&cfg))], &Fake::default());
    assert!(r.out.ends_with("\nNote: agents.json is in the old format (presets); its presets are ignored and the default profiles apply.\n"), "{}", r.out);
}

#[test]
fn agents_usage_errors() {
    let r = call(&["agents"], &[], &Fake::default());
    assert_eq!((r.code, r.err.as_str()), (2, AGENTS_USAGE));
    let r = call(&["agents", "flow", "x"], &[], &Fake::default());
    assert_eq!((r.code, r.err.as_str()), (2, "herdr-contr-orch agents flow: takes no arguments\n"));
    let r = call(&["agents", "watch"], &[], &Fake::default()); // not ported
    assert_eq!(r.code, 2);
    assert!(r.err.starts_with("herdr-contr-orch agents: unknown command watch\n"));
    let r = call(&["--help"], &[], &Fake::default());
    assert!(r.out.contains("agents <command>"));
}

#[test]
fn owner_ask_saves_the_question_for_the_project_and_the_terminal() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("owner.json");
    let vars = [("CONTR_OWNER_FILE", path(&file)), ("CONTR_PROJECT", "app"), ("HC_TERMINAL_ID", "t2nw")];
    let r = call(&["owner", "ask", "--title", "Merge order", "--from=PR 12", "--option", "now", "-option", "later", "--impact", "risk",
        "--recommended", "1", "--ref", "#12", "--ref", "docs/a.md", "--source", "a line", "Merge", "it", "--now?"], &vars, &Fake::default());
    assert_eq!((r.code, r.out.as_str(), r.err.as_str()), (0, "D1\n", ""));
    let q = Store::new(&file).read().unwrap().questions.remove(0);
    assert_eq!((q.project.as_str(), q.coordinator.as_str(), q.title.as_str(), q.from.as_str()), ("app", "t2nw", "Merge order", "PR 12"));
    assert_eq!(q.question, "Merge it --now?");
    assert_eq!((q.options, q.impacts, q.refs), (vec!["now".to_string(), "later".into()], vec!["risk".to_string()], vec!["#12".to_string(), "docs/a.md".into()]));
    assert_eq!((q.recommended, q.source.as_str()), (Some(1), "a line"));
    let r = call(&["owner", "ask", "--", "--is", "this", "ok?"], &vars, &Fake::default());
    assert_eq!(r.out, "D2\n");
    assert_eq!(Store::new(&file).read().unwrap().questions[1].question, "--is this ok?");
}

#[test]
fn owner_ask_usage_errors_save_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("owner.json");
    let vars = [("CONTR_OWNER_FILE", path(&file))];
    for (args, want) in [
        (vec!["owner", "ask"], "give the question"),
        (vec!["owner", "ask", "--title", "t"], "give the question"),
        (vec!["owner", "ask", "--recommended", "0", "q"], "--recommended and --impact need a matching --option"),
        (vec!["owner", "ask", "--option", "a", "--impact", "x", "--impact", "y", "q"], "need a matching --option"),
        (vec!["owner", "ask", "--recommended", "one", "--option", "a", "q"], "is not a number"),
        (vec!["owner", "ask", "--color", "red", "q"], "flag provided but not defined: -color"),
        (vec!["owner", "ask", "--title"], "flag needs an argument: -title"),
    ] {
        let r = call(&args, &vars, &Fake::default());
        assert_eq!(r.code, 2, "{args:?}");
        assert!(r.err.contains(want), "{args:?}: {}", r.err);
    }
    assert!(!file.exists());
}

#[test]
fn owner_answer_saves_then_tells_the_coordinator() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("owner.json");
    let vars = [("CONTR_OWNER_FILE", path(&file)), ("HC_TERMINAL_ID", "t9")];
    call(&["owner", "ask", "Which", "way?"], &vars, &Fake::default());
    let core = Fake::default();
    let r = call(&["owner", "answer", "D1", "the", "short", "way"], &vars, &core);
    assert_eq!((r.code, r.err.as_str()), (0, ""));
    assert_eq!(core.calls(), [("terminal.send".to_string(), json!({"terminal": "t9", "caller": "t9", "text": "Owner answered D1: the short way\n"}))]);
    let q = Store::new(&file).read().unwrap().questions.remove(0);
    assert_eq!((q.answer.as_str(), q.answered_by.as_str()), ("the short way", "owner"));
    let r = call(&["owner", "answer", "D1", "again"], &vars, &core);
    assert_eq!((r.code, r.err.as_str()), (1, "herdr-contr-orch owner answer: owner: no open question D1\n"));
    assert_eq!(core.calls().len(), 1);
}

#[test]
fn owner_done_marks_the_todo_and_tells_only_a_coordinator_that_asked() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("owner.json");
    let vars = [("CONTR_OWNER_FILE", path(&file))];
    let store = Store::new(&file);
    store.pin(Todo { text: "Test the build".into(), coordinator: "t4".into(), ..Todo::default() }).unwrap();
    store.pin(Todo { text: "No one asked".into(), ..Todo::default() }).unwrap();
    let core = Fake::default();
    assert_eq!(call(&["owner", "done", "T1"], &vars, &core).code, 0);
    assert_eq!(call(&["owner", "done", "T2"], &vars, &core).code, 0);
    assert_eq!(core.calls(), [("terminal.send".to_string(), json!({"terminal": "t4", "text": "Owner did T1: Test the build\n"}))]);
    assert_eq!(store.read().unwrap().open("").1.len(), 0);
    let r = call(&["owner", "done", "T3"], &vars, &core);
    assert_eq!((r.code, r.err.as_str()), (1, "herdr-contr-orch owner done: owner: no open to-do T3\n"));
}

#[test]
fn a_coordinator_that_cannot_be_reached_keeps_the_change_and_exits_1() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("owner.json");
    let vars = [("CONTR_OWNER_FILE", path(&file)), ("HC_TERMINAL_ID", "t9")];
    call(&["owner", "ask", "Go?"], &vars, &Fake::default());
    let r = call(&["owner", "answer", "D1", "yes"], &vars, &Fake::default().err("terminal.send", "no core"));
    assert_eq!((r.code, r.err.as_str()), (1, "herdr-contr-orch owner answer: saved; coordinator not reached: no core\n"));
    assert_eq!(Store::new(&file).read().unwrap().questions[0].answer, "yes");
}

#[test]
fn owner_usage_errors() {
    let r = call(&["owner"], &[], &Fake::default());
    assert_eq!((r.code, r.err.as_str()), (2, OWNER_USAGE));
    let r = call(&["owner", "help"], &[], &Fake::default());
    assert_eq!((r.code, r.out.as_str()), (0, OWNER_USAGE));
    for args in [vec!["owner", "answer", "D1"], vec!["owner", "answer", "D1", " "], vec!["owner", "done"], vec!["owner", "done", "T1", "T2"]] {
        let r = call(&args, &[], &Fake::default());
        assert_eq!(r.code, 2, "{args:?}");
        assert!(r.err.contains("wrong number of arguments"), "{args:?}: {}", r.err);
    }
    let r = call(&["owner", "pin", "x"], &[], &Fake::default());
    assert_eq!(r.code, 2);
    assert!(r.err.starts_with("herdr-contr-orch owner: unknown command pin\n"));
}

#[test]
fn the_default_files_are_in_the_user_config_folder() {
    let home = tempfile::tempdir().unwrap();
    let vars = [("HOME", path(home.path())), ("XDG_CONFIG_HOME", ""), ("CONTR_PROJECT", "app")];
    assert_eq!(call(&["owner", "ask", "Go?"], &vars, &Fake::default()).out, "D1\n");
    let base = if cfg!(target_os = "macos") { "Library/Application Support" } else { ".config" };
    assert!(home.path().join(base).join("contr/owner.json").exists());
    let r = call(&["agents", "flow"], &vars, &Fake::default());
    assert_eq!(r.code, 0);
    let r = call(&["owner", "ask", "Go?"], &[("CONTR_PROJECT", "app")], &Fake::default());
    assert_eq!(r.code, 1);
    assert!(r.err.contains("HOME"), "{}", r.err);
}
