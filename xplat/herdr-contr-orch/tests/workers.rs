//! The worker commands on a fake hc core. No test talks to the owner's hc core.
mod common;

use common::{call, Fake};
use orch::workers::{state_text, Core, Worker};
use serde_json::{json, Value};

fn worker(addr: &str, name: &str, state: &str, status: Value) -> Value {
    json!({"address": addr, "name": name, "state": state, "status": status, "exit_code": 0,
        "labels": {"contr": "agent", "role": "builder", "profile": "builder", "worktree": "/w/a", "issue": "158"}})
}

#[test]
fn ls_prints_a_table_or_json() {
    let core = Fake::default().reply("terminal.list", json!({"terminals": [
        worker("t3", "r-158", "finished", Value::Null),
        worker("t2", "b-158", "live", json!({"state": "blocked", "kind": "permission"})),
    ]}));
    let r = call(&["agents", "ls", "--all"], &[], &core);
    assert_eq!((r.code, r.err.as_str()), (0, ""));
    assert_eq!(
        r.out,
        "NAME   ADDRESS  ROLE     ISSUE  STATE                WORKTREE\n\
         b-158  t2       builder  158    blocked: permission  /w/a\n\
         r-158  t3       builder  158    ended (0)            /w/a\n"
    );
    assert_eq!(core.calls()[0], ("terminal.list".to_string(), json!({"all": true, "labels": {"contr": "agent"}})));

    let r = call(&["agents", "ls", "--json"], &[], &core);
    let ws: Value = serde_json::from_str(&r.out).unwrap();
    assert_eq!(ws[0], json!({"name": "b-158", "address": "t2", "role": "builder", "issue": "158", "profile": "builder",
        "worktree": "/w/a", "live": true, "status": "blocked", "kind": "permission", "turn": 0, "exit_code": 0}));
    assert_eq!(core.calls()[1].1["all"], false);
    assert!(r.out.starts_with("[\n  {\n    \"name\": \"b-158\",\n    \"address\": \"t2\","), "{}", r.out);
    assert_eq!(call(&["agents", "ls", "--json"], &[], &Fake::default()).out, "[]\n");
}

#[test]
fn ls_shows_a_plain_terminal() {
    let core = Fake::default().reply("terminal.list", json!({"terminals": [
        {"address": "t5", "name": "term-0101", "state": "live", "labels": {"contr": "agent", "role": "terminal", "worktree": "/w/a"}},
    ]}));
    let r = call(&["agents", "ls"], &[], &core);
    assert_eq!(r.out.lines().nth(1).unwrap(), "term-0101  t5       terminal  -      -      /w/a");
}

#[test]
fn send_interrupt_and_read() {
    let core = Fake::default().reply("terminal.interrupt", json!({"outcome": "stopped"})).reply("terminal.read", json!({"text": "line\n\n"}));
    let vars = [("HC_TERMINAL_ID", "t1")];
    let r = call(&["agents", "send", "w1", "fix", "the", "test"], &vars, &core);
    assert_eq!((r.code, r.out.as_str()), (0, ""));
    let r = call(&["agents", "interrupt", "w1"], &[], &core);
    assert_eq!((r.code, r.out.as_str()), (0, "stopped\n"));
    let r = call(&["agents", "read", "--lines", "5", "w1"], &[], &core);
    assert_eq!((r.code, r.out.as_str()), (0, "line\n"));
    let r = call(&["agents", "read", "w1", "--lines=7"], &[], &core);
    assert_eq!((r.code, r.out.as_str()), (0, "line\n"));
    call(&["agents", "read", "w1"], &[], &core);
    assert_eq!(
        core.calls(),
        [
            ("terminal.send".to_string(), json!({"terminal": "w1", "caller": "t1", "text": "fix the test"})),
            ("terminal.interrupt".to_string(), json!({"terminal": "w1"})),
            ("terminal.read".to_string(), json!({"terminal": "w1", "lines": 5})),
            ("terminal.read".to_string(), json!({"terminal": "w1", "lines": 7})),
            ("terminal.read".to_string(), json!({"terminal": "w1"})),
        ]
    );
}

#[test]
fn a_send_with_no_text_or_that_hc_refuses_is_an_error() {
    let core = Fake::default();
    let r = call(&["agents", "send", "w1", " "], &[], &core);
    assert_eq!((r.code, r.err.as_str()), (1, "herdr-contr-orch agents send: agents: no text to send\n"));
    assert!(core.calls().is_empty());
    let core = Fake::default().reply("terminal.send", json!({"sent": false, "reason": "the owner types"}));
    let r = call(&["agents", "send", "w1", "hi"], &[], &core);
    assert_eq!((r.code, r.err.as_str()), (1, "herdr-contr-orch agents send: agents: w1 did not take the text: the owner types\n"));
    let core = Fake::default().reply("terminal.send", json!({"sent": false}));
    assert!(call(&["agents", "send", "w1", "hi"], &[], &core).err.ends_with("no reason given\n"));
    let core = Fake::default().reply("terminal.send", json!({"sent": true}));
    assert_eq!(call(&["agents", "send", "w1", "hi"], &[], &core).code, 0);
}

#[test]
fn stop_removes_the_terminal_unless_kept() {
    for (args, state, want) in [
        (vec!["agents", "stop", "w1"], "live", vec!["terminal.get", "terminal.kill", "terminal.rm"]),
        (vec!["agents", "stop", "w1", "--keep"], "live", vec!["terminal.get", "terminal.kill"]),
        (vec!["agents", "stop", "w1"], "finished", vec!["terminal.get", "terminal.rm"]),
        (vec!["agents", "rm", "w1"], "finished", vec!["terminal.rm"]),
    ] {
        let core = Fake::default().reply("terminal.get", worker("t1", "w1", state, Value::Null));
        let r = call(&args, &[], &core);
        assert_eq!((r.code, r.err.as_str()), (0, ""), "{args:?}");
        assert_eq!(core.methods(), want, "{args:?} ({state})");
    }
}

#[test]
fn an_error_of_hc_is_printed_and_the_exit_code_is_1() {
    let core = Fake::default().err("terminal.get", "hc: no terminal \"zz\" (not_found)");
    let r = call(&["agents", "stop", "zz"], &[], &core);
    assert_eq!((r.code, r.err.as_str()), (1, "herdr-contr-orch agents stop: hc: no terminal \"zz\" (not_found)\n"));
    assert_eq!(core.methods(), ["terminal.get"]);
    let core = Fake::default().err("terminal.list", "hc: socket: connect failed");
    let r = call(&["agents", "ls"], &[], &core);
    assert_eq!((r.code, r.out.as_str()), (1, ""));
}

#[test]
fn rm_ended_removes_the_finished_workers_of_this_terminal() {
    let with_parent = |mut w: Value, parent: &str| {
        w["parent"] = json!(parent);
        w
    };
    let mut lead = with_parent(worker("t6", "lead", "finished", Value::Null), "t1");
    lead["labels"]["role"] = json!("orchestrator");
    let list = json!({"terminals": [
        with_parent(worker("t4", "done-1", "finished", Value::Null), "t1"),
        with_parent(worker("t5", "busy", "live", Value::Null), "t1"),
        with_parent(worker("t8", "theirs", "finished", Value::Null), "t2"),
        lead,
    ]});
    let core = Fake::default().reply("terminal.list", list.clone());
    let r = call(&["agents", "rm", "--ended"], &[("HC_TERMINAL_ID", "t1")], &core);
    assert_eq!((r.code, r.out.as_str(), r.err.as_str()), (0, "done-1\n", ""));
    assert_eq!(core.methods(), ["terminal.list", "terminal.rm"]);
    assert_eq!(core.calls()[1].1, json!({"terminal": "t4", "caller": "t1"}));
    // Outside hc nothing is removed.
    let core = Fake::default().reply("terminal.list", list.clone());
    assert_eq!(call(&["agents", "rm", "--ended"], &[], &core).code, 0);
    assert!(core.calls().is_empty());
    // A remove that fails names the worker.
    let core = Fake::default().reply("terminal.list", list).err("terminal.rm", "hc: busy");
    let r = call(&["agents", "rm", "--ended"], &[("HC_TERMINAL_ID", "t1")], &core);
    assert_eq!((r.code, r.err.as_str()), (1, "herdr-contr-orch agents rm: remove done-1: hc: busy\n"));
}

#[test]
fn usage_errors_make_no_call() {
    let core = Fake::default();
    for (args, want) in [
        (vec!["agents", "rm"], "give one name, or --ended"),
        (vec!["agents", "rm", "a", "--ended"], "give one name, or --ended"),
        (vec!["agents", "rm", "a", "b"], "give one name, or --ended"),
        (vec!["agents", "stop", "a", "--ended"], "flag --ended does not apply"),
        (vec!["agents", "rm", "a", "--keep"], "flag --keep does not apply"),
        (vec!["agents", "ls", "x"], "wrong number of arguments"),
        (vec!["agents", "send", "w1"], "wrong number of arguments"),
        (vec!["agents", "read"], "wrong number of arguments"),
        (vec!["agents", "interrupt", "a", "b"], "wrong number of arguments"),
        (vec!["agents", "read", "w1", "--lines"], "flag needs an argument: -lines"),
        (vec!["agents", "read", "w1", "--lines", "x"], "--lines \"x\" is not a number"),
        (vec!["agents", "ls", "--wide"], "flag provided but not defined: -wide"),
        (vec!["agents", "ls", "--all=maybe"], "is not true or false"),
    ] {
        let r = call(&args, &[], &core);
        assert_eq!(r.code, 2, "{args:?}");
        assert!(r.err.contains(want), "{args:?}: {}", r.err);
    }
    assert!(core.calls().is_empty());
    // After --, a word that starts with - is text.
    let r = call(&["agents", "send", "w1", "--", "--all", "of", "it"], &[], &core);
    assert_eq!(r.code, 0);
    assert_eq!(core.calls()[0].1["text"], "--all of it");
}

#[test]
fn state_text_of_each_state() {
    let w = |live: bool, status: &str, kind: &str, exit_code: Option<i64>| Worker { live, status: status.into(), kind: kind.into(), exit_code, ..Worker::default() };
    assert_eq!(state_text(&w(false, "", "", Some(1))), "ended (1)");
    assert_eq!(state_text(&w(false, "working", "", None)), "ended");
    assert_eq!(state_text(&w(true, "blocked", "auth", None)), "blocked: auth");
    assert_eq!(state_text(&w(true, "blocked", "", None)), "blocked");
    assert_eq!(state_text(&w(true, "working", "", None)), "working");
    assert_eq!(state_text(&w(true, "", "", None)), "-");
}

#[test]
fn a_worker_from_a_terminal_of_this_tool() {
    let clean = ["/bin/sh", "-c", "script", "contr-clean-env", "/opt/bin/claude", "--model", "opus", "--effort=low", "the task"];
    let setup: Vec<&str> = ["/bin/sh", "-c", "wrapper", "contr-setup", "npm ci"].into_iter().chain(clean).collect();
    let w = Worker::from_terminal(&json!({"address": "t2", "name": "b-1", "parent": "t1", "state": "live", "turn": 3, "title": "Fix it",
        "argv": setup, "foreground": null, "status": {"state": "working", "kind": null, "msg": "m"},
        "labels": {"contr": "agent", "role": "builder", "profile": "builder-high", "worktree": "/w", "project": "app",
            "contr.model": "sonnet", "contr.orchestrating": "1", "contr.context": "1200", "contr.context-limit": "200000"}}));
    assert_eq!((w.command.as_str(), w.model.as_str(), w.effort.as_str()), ("claude", "sonnet", "low"));
    assert_eq!((w.parent.as_str(), w.project.as_str(), w.turn, w.title.as_str(), w.msg.as_str()), ("t1", "app", 3, "Fix it", "m"));
    assert_eq!((w.orchestrating, w.context_tokens, w.context_limit, w.foreign, w.exit_code), (true, 1200, 200000, false, None));
    // The foreground program's name comes first; one context label that is not a positive integer gives no data.
    let w = Worker::from_terminal(&json!({"address": "t2", "argv": ["claude"], "foreground": {"name": "node"},
        "labels": {"contr": "agent", "role": "builder", "contr.context": "0", "contr.context-limit": "200000"}}));
    assert_eq!((w.command.as_str(), w.context_tokens, w.context_limit), ("node", 0, 0));
    // A plain terminal with Claude Code in front is a session, by name or by version.
    for (front, session) in [("claude", true), ("/x/2.1.30", true), ("2.1", false), ("zsh", false)] {
        let w = Worker::from_terminal(&json!({"address": "t5", "foreground": {"name": front}, "labels": {"contr": "agent", "role": "terminal"}}));
        assert_eq!((w.session, w.role.as_str()), (session, if session { "orchestrator" } else { "terminal" }), "{front}");
    }
}

#[test]
fn a_worker_from_a_terminal_that_this_tool_did_not_start() {
    let w = Worker::from_terminal(&json!({"address": "t7", "cwd": "/code/app", "state": "live", "argv": ["/bin/zsh", "-l"], "labels": {}}));
    assert_eq!((w.foreign, w.name.as_str(), w.role.as_str(), w.worktree.as_str(), w.command.as_str()), (true, "t7", "terminal", "/code/app", "zsh"));
    for t in [
        json!({"address": "t7", "name": "mine", "argv": ["claude", "--model", "opus"]}),
        json!({"address": "t7", "name": "mine", "argv": ["zsh"], "foreground": {"name": "2.1.30"}}),
        json!({"address": "t7", "name": "mine", "argv": ["zsh"], "status": {"state": "idle"}}),
    ] {
        let w = Worker::from_terminal(&t);
        assert_eq!((w.name.as_str(), w.role.as_str(), w.foreign), ("mine", "orchestrator", true), "{t}");
    }
}

/// The real client on a socket: a result comes back, and an error of hc comes back as text with its kind.
#[cfg(unix)]
#[test]
fn the_hc_client_is_a_core() {
    use std::io::{BufRead, BufReader, Write};
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("hc.sock");
    let listener = std::os::unix::net::UnixListener::bind(&sock).unwrap();
    std::thread::spawn(move || {
        for mut s in listener.incoming().flatten() {
            let mut line = String::new();
            BufReader::new(s.try_clone().unwrap()).read_line(&mut line).unwrap();
            let req: Value = serde_json::from_str(&line).unwrap();
            let reply = if req["method"] == "terminal.read" {
                json!({"jsonrpc": "2.0", "id": req["id"], "result": {"text": req["params"]["terminal"]}})
            } else {
                json!({"jsonrpc": "2.0", "id": req["id"], "error": {"code": -32000, "message": "no terminal \"zz\"", "data": {"kind": "not_found"}}})
            };
            s.write_all(format!("{reply}\n").as_bytes()).unwrap();
        }
    });
    let client = herdr_core::hc::HcClient::new(herdr_core::hc::HcSession::new(sock.to_str().unwrap()), 2, 2);
    let core: &dyn Core = &client;
    assert_eq!(core.call("terminal.read", json!({"terminal": "w1"})), Ok(json!({"text": "w1"})));
    assert_eq!(core.call("terminal.get", json!({"terminal": "zz"})), Err("hc: no terminal \"zz\" (not_found)".to_string()));
    let none = herdr_core::hc::HcClient::new(herdr_core::hc::HcSession::new("/nonexistent/hc.sock"), 1, 1);
    assert!(Core::call(&none, "terminal.list", json!({})).unwrap_err().starts_with("hc: socket: connect"));
}
