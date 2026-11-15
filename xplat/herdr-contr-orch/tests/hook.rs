//! The Stop hook of a worker, on a fake hc core. The transcripts are in temporary folders.
mod common;

use std::fs;
use std::path::Path;

use common::{call_with, path, Fake, Run};
use orch::hook::{missed_report, SAID_MAX};
use serde_json::json;

const PROMPT: &str = r#"{"type":"user","message":{"role":"user","content":"do the task"}}"#;
const SEND: &str = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"s1","name":"Bash","input":{"command":"hc send t1 \"b1: done\""}}]}}"#;
const SENT: &str = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"s1","content":"sent"}]}}"#;
const WORDS: &str = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"PR #5 is open.\nHead abc."}]}}"#;
const WORKER: [(&str, &str); 2] = [("CONTR_COORDINATOR", "t1"), ("HC_TERMINAL_ID", "t7")];

fn transcript(dir: &Path, lines: &[&str]) -> String {
    let p = dir.join("t.jsonl");
    fs::write(&p, lines.join("\n")).unwrap();
    path(&p).to_string()
}

fn stop(vars: &[(&str, &str)], input: &str, core: &Fake) -> Run {
    call_with(&["hook", "stop"], vars, core, input)
}

#[test]
fn a_turn_with_no_report_gets_one() {
    let dir = tempfile::tempdir().unwrap();
    let (cwd, core) = (path(dir.path()), Fake::default());
    let input = json!({"transcript_path": transcript(dir.path(), &[PROMPT, WORDS]), "cwd": cwd}).to_string();
    let r = stop(&WORKER, &input, &core);
    assert_eq!((r.code, r.out.as_str(), r.err.as_str()), (0, "", ""));
    let want = "ended its turn without a report. Last words: PR #5 is open. Head abc.";
    assert_eq!(core.calls(), [("terminal.send".to_string(), json!({"terminal": "t1", "caller": "t7", "text": want}))]);
    // With RESULT.md in the folder, the report names it.
    fs::write(dir.path().join("RESULT.md"), "x").unwrap();
    assert_eq!(missed_report("done", cwd), format!("ended its turn without a report. Last words: done; RESULT.md in {cwd}"));
}

#[test]
fn the_last_assistant_message_of_the_input_comes_first() {
    let dir = tempfile::tempdir().unwrap();
    let core = Fake::default();
    let input = json!({"transcript_path": transcript(dir.path(), &[PROMPT, WORDS]), "last_assistant_message": "All done."}).to_string();
    stop(&WORKER, &input, &core);
    assert_eq!(core.calls()[0].1["text"], "ended its turn without a report. Last words: All done.");
}

#[test]
fn with_no_transcript_the_report_says_that_it_can_repeat_one() {
    let dir = tempfile::tempdir().unwrap();
    let (cwd, core) = (path(dir.path()), Fake::default());
    fs::write(dir.path().join("RESULT.md"), "x").unwrap();
    let input = json!({"transcript_path": path(&dir.path().join("gone.jsonl")), "cwd": cwd, "last_assistant_message": "PR #5 is open."}).to_string();
    let r = stop(&WORKER, &input, &core);
    assert_eq!((r.code, r.err.as_str()), (0, ""));
    let want = format!("ended its turn; it may have sent a report already (no transcript, so contr cannot tell). Last words: PR #5 is open.; RESULT.md in {cwd}");
    assert_eq!((&core.calls()[0].1["terminal"], &core.calls()[0].1["text"]), (&json!("t1"), &json!(want)));
}

#[test]
fn the_hook_is_quiet_when_it_has_no_work() {
    let dir = tempfile::tempdir().unwrap();
    let core = Fake::default();
    // A turn that reported sends nothing.
    let input = json!({"transcript_path": transcript(dir.path(), &[PROMPT, SEND, SENT, WORDS])}).to_string();
    assert_eq!(stop(&WORKER, &input, &core).code, 0);
    // Outside hc nothing is read or sent, even with bad input.
    assert_eq!(stop(&[], "not json", &core).code, 0);
    assert!(core.calls().is_empty());
    // A session in hc that is not a worker reports to nobody.
    let input = json!({"transcript_path": transcript(dir.path(), &[PROMPT, WORDS])}).to_string();
    assert_eq!(stop(&[("HC_TERMINAL_ID", "t7")], &input, &core).code, 0);
    assert!(!core.methods().contains(&"terminal.send".to_string()));
}

#[test]
fn an_error_gives_exit_code_1_and_never_2() {
    let dir = tempfile::tempdir().unwrap();
    let core = Fake::default();
    let r = stop(&WORKER, "not json", &core);
    assert!(r.code == 1 && r.err.contains("bad hook input"), "{}", r.err);
    // A transcript that is there but cannot be read is an error.
    let r = stop(&WORKER, &json!({"transcript_path": path(dir.path())}).to_string(), &core);
    assert!(r.code == 1 && r.err.starts_with("herdr-contr-orch hook: "), "{}", r.err);
    let r = stop(&WORKER, "{}", &core);
    assert!(r.code == 1 && r.err.contains("no transcript_path"), "{}", r.err);
    assert!(core.calls().is_empty());

    let down = Fake::default().err("terminal.send", "not_found");
    let r = stop(&WORKER, &json!({"transcript_path": transcript(dir.path(), &[PROMPT])}).to_string(), &down);
    assert!(r.code == 1 && r.err.contains("report to t1: not_found"), "{}", r.err);
    assert_eq!(down.calls().len(), 1);

    for args in [vec!["hook"], vec!["hook", "start"], vec!["hook", "stop", "more"]] {
        let r = call_with(&args, &WORKER, &core, "");
        assert!(r.code == 1 && r.err.starts_with("Usage:"), "{args:?}: {}", r.err);
    }
}

#[test]
fn the_last_words_are_one_line_and_not_too_long() {
    let got = missed_report(&"é".repeat(SAID_MAX + 10), "");
    assert!(got.ends_with(&("é".repeat(SAID_MAX) + "…")) && !got.ends_with(&("é".repeat(SAID_MAX + 1) + "…")), "{got}");
    assert_eq!(missed_report("  ", ""), "ended its turn without a report. Last words: (none)");
}
