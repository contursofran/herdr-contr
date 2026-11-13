//! The menu of a blocked worker, read from screen fixtures in `tests/data/prompt`.
mod common;

use common::{call, Fake};
use orch::prompt::{parse, Prompt};
use serde_json::json;

fn read(name: &str) -> String {
    std::fs::read_to_string(format!("{}/tests/data/prompt/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn check(p: Option<Prompt>, question: &str, choices: &[&str]) {
    let p = p.expect("not parsed");
    assert_eq!((p.question.as_str(), p.choices.iter().map(String::as_str).collect::<Vec<_>>()), (question, choices.to_vec()));
}

#[test]
fn parses_the_menus_of_claude_code_and_codex() {
    check(
        parse("claude --model opus", "permission", &read("claude_permission.txt")),
        "Do you want to proceed?",
        &["Yes", "Yes, and don't ask again for npm test commands in /work", "No, and tell Claude what to do differently (esc)"],
    );
    check(parse("/usr/local/bin/claude", "question", &read("claude_question.txt")), "Which library should the parser use?", &["regexp", "strings only", "Type something.", "Chat about this"]);
    check(
        parse("codex", "permission", &read("codex_approval.txt")),
        "$ touch /work/out.txt",
        &["Yes, proceed (y)", "Yes, and don't ask again for this command (a)", "No, and tell Codex what to do differently (esc)"],
    );
    // Claude Code can name itself by its version as the foreground program.
    assert!(parse("2.1.30", "permission", &read("claude_permission.txt")).is_some());
    // The last menu on the screen, not an answered one above it.
    check(parse("claude", "permission", &read("old_prompt.txt")), "Do you want to run make?", &["Yes", "Yes, and don't ask again", "No"]);
}

#[test]
fn gives_nothing_for_what_it_cannot_read() {
    let screen = read("claude_permission.txt");
    assert_eq!(parse("claude", "permission", &read("unknown.txt")), None);
    assert_eq!(parse("bash", "permission", &screen), None);
    assert_eq!(parse("", "permission", &screen), None);
    assert_eq!(parse("claude", "auth", &screen), None);
    assert_eq!(parse("claude", "question", "Pick\n1. a\n2. b\n4. d\n"), None, "a gap in the numbers");
    assert_eq!(parse("claude", "question", "Pick\n1. only\n"), None, "one choice");
    let ten: String = (1..=10).map(|n| format!("{n}. item\n")).collect();
    assert_eq!(parse("claude", "question", &format!("Pick\n{ten}")), None, "ten choices");
    assert_eq!(parse("claude", "question", "Pick\n1.a\n2.b\n"), None, "no space after the number");
}

#[test]
fn the_box_above_the_question_is_the_detail() {
    let p = parse("claude", "permission", &read("claude_permission.txt")).unwrap();
    assert_eq!(p.detail, ["Bash command", "npm test", "Run the unit tests"]);
    let sep = "● Done.\n────────────\n Bash command\n\n   go test ./...\n\n Do you want to proceed?\n ❯ 1. Yes\n   2. No\n";
    assert_eq!(parse("claude", "permission", sep).unwrap().detail, ["Bash command", "go test ./..."]);
    assert!(parse("claude", "permission", &read("old_prompt.txt")).unwrap().detail.is_empty());
    assert!(parse("claude", "question", &read("claude_question.txt")).unwrap().detail.is_empty());
    assert_eq!(parse("claude", "question", "1. a\n2. b\n").unwrap(), Prompt { choices: vec!["a".into(), "b".into()], ..Prompt::default() });
}

fn worker(state: &str, status: &str) -> serde_json::Value {
    json!({"address": "t2", "name": "b-1", "state": state, "status": {"state": status, "kind": "permission"}, "foreground": {"name": "claude"},
        "labels": {"contr": "agent", "role": "builder"}})
}

#[test]
fn agents_prompt_prints_the_menu_of_a_blocked_worker() {
    let core = Fake::default().reply("terminal.get", worker("live", "blocked")).reply("terminal.read", json!({"text": read("claude_permission.txt")}));
    let r = call(&["agents", "prompt", "b-1"], &[("HC_TERMINAL_ID", "t1")], &core);
    assert_eq!((r.code, r.err.as_str()), (0, ""));
    assert_eq!(
        r.out,
        "Bash command\nnpm test\nRun the unit tests\nDo you want to proceed?\n1. Yes\n2. Yes, and don't ask again for npm test commands in /work\n3. No, and tell Claude what to do differently (esc)\n"
    );
    assert_eq!(core.calls()[1], ("terminal.read".to_string(), json!({"terminal": "b-1", "caller": "t1", "lines": 40})));

    // A screen with no menu, a worker that is not blocked and one that ended: "no prompt".
    let core = Fake::default().reply("terminal.get", worker("live", "blocked")).reply("terminal.read", json!({"text": read("unknown.txt")}));
    assert_eq!(call(&["agents", "prompt", "b-1"], &[], &core).out, "no prompt\n");
    for w in [worker("live", "working"), worker("finished", "blocked")] {
        let core = Fake::default().reply("terminal.get", w);
        assert_eq!(call(&["agents", "prompt", "b-1"], &[], &core).out, "no prompt\n");
        assert_eq!(core.methods(), ["terminal.get"]);
    }
}

#[test]
fn agents_prompt_errors() {
    let r = call(&["agents", "prompt", "zz"], &[], &Fake::default().err("terminal.get", "hc: no terminal \"zz\" (not_found)"));
    assert_eq!((r.code, r.err.as_str()), (1, "herdr-contr-orch agents prompt: hc: no terminal \"zz\" (not_found)\n"));
    let core = Fake::default().reply("terminal.get", worker("live", "blocked")).err("terminal.read", "hc: gone");
    assert_eq!(call(&["agents", "prompt", "b-1"], &[], &core).code, 1);
    for args in [&["agents", "prompt"][..], &["agents", "prompt", "a", "b"], &["agents", "prompt", "a", "--json"]] {
        assert_eq!(call(args, &[], &Fake::default()).code, 2, "{args:?}");
    }
}
