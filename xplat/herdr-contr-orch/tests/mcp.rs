//! The MCP server on a fake hc core, with the owner file and the profiles file in a temporary folder.
mod common;

use std::fs;

use common::{call_with, path, Fake};
use orch::mcp::{serve, Tool, PROTOCOL_VERSION};
use orch::owner::Store;
use serde_json::{json, Value};

/// The replies of the server to these requests, one for each line that it wrote.
fn talk(requests: &[Value], vars: &[(&str, &str)], core: &Fake) -> Vec<Value> {
    let input: String = requests.iter().map(|r| format!("{r}\n")).collect();
    let r = call_with(&["mcp"], vars, core, &input);
    assert_eq!((r.code, r.err.as_str()), (0, ""));
    r.out.lines().map(|l| serde_json::from_str(l).unwrap()).collect()
}

/// The paths of the owner file and of the profiles file in `dir`. The test writes them when it needs them.
fn files(dir: &std::path::Path) -> (String, String) {
    (path(&dir.join("owner.json")).to_string(), path(&dir.join("agents.json")).to_string())
}

fn tool(id: i64, name: &str, args: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": name, "arguments": args}})
}

/// The text of a tool result, and whether it is an error.
fn text(reply: &Value) -> (&str, bool) {
    (reply["result"]["content"][0]["text"].as_str().unwrap_or_else(|| panic!("no text: {reply}")), reply["result"]["isError"] == json!(true))
}

fn echo<'a>() -> Vec<Tool<'a>> {
    vec![Tool {
        name: "echo",
        description: "Says it again.",
        schema: json!({"type": "object"}),
        run: Box::new(|a| match a["say"].as_str() {
            Some(s) => Ok(s.to_string()),
            None => Err("nothing to say".into()),
        }),
    }]
}

fn serve_lines(input: &str) -> Vec<Value> {
    let mut out = Vec::new();
    serve("test", "1.2.3", &echo(), &mut input.as_bytes(), &mut out).unwrap();
    String::from_utf8(out).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect()
}

#[test]
fn the_server_answers_initialize_list_and_ping() {
    let got = serve_lines(concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}"#, "\n",
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#, "\n\n",
        r#"{"jsonrpc":"2.0","id":"a","method":"initialize","params":{"protocolVersion":"1999-01-01"}}"#, "\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#, "\n",
        r#"{"jsonrpc":"2.0","id":3,"method":"ping"}"#, "\n",
    ));
    assert_eq!(got.len(), 4, "{got:?}");
    assert_eq!(got[0], json!({"jsonrpc": "2.0", "id": 1, "result": {"protocolVersion": "2024-11-05", "capabilities": {"tools": {}}, "serverInfo": {"name": "test", "version": "1.2.3"}}}));
    assert_eq!((&got[1]["id"], &got[1]["result"]["protocolVersion"]), (&json!("a"), &json!(PROTOCOL_VERSION)));
    assert_eq!(got[2]["result"], json!({"tools": [{"name": "echo", "description": "Says it again.", "inputSchema": {"type": "object"}}]}));
    assert_eq!(got[3], json!({"jsonrpc": "2.0", "id": 3, "result": {}}));
}

#[test]
fn a_tool_call_gives_its_text_or_its_error() {
    let got = serve_lines(concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"echo","arguments":{"say":"hi"}}}"#, "\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"echo"}}"#, "\n",
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"nope"}}"#, "\n",
        r#"{"jsonrpc":"2.0","id":4,"method":"tools/call"}"#, "\n",
        r#"{"jsonrpc":"2.0","id":5,"method":"resources/list"}"#, "\n",
        "{not json\n",
    ));
    assert_eq!(got[0]["result"], json!({"content": [{"type": "text", "text": "hi"}]}));
    assert_eq!(got[1]["result"], json!({"content": [{"type": "text", "text": "echo: nothing to say"}], "isError": true}));
    assert_eq!(got[2]["error"], json!({"code": -32602, "message": "unknown tool: nope"}));
    assert_eq!(got[3]["error"]["code"], -32602);
    assert_eq!(got[4]["error"], json!({"code": -32601, "message": "method not found: resources/list"}));
    assert_eq!((&got[5]["id"], &got[5]["error"]["code"]), (&Value::Null, &json!(-32700)));
}

#[test]
fn the_server_lists_the_worker_tools_and_the_owner_tools() {
    let dir = tempfile::tempdir().unwrap();
    let (own, ag) = files(dir.path());
    let vars = [("CONTR_OWNER_FILE", own.as_str()), ("CONTR_AGENTS_CONFIG", ag.as_str())];
    let got = talk(&[json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}), json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})], &vars, &Fake::default());
    assert_eq!(got[0]["result"]["serverInfo"], json!({"name": "herdr-contr-orch", "version": env!("CARGO_PKG_VERSION")}));
    let tools = got[1]["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["spawn", "message", "interrupt", "stop", "read", "prompt", "list", "clear_ended", "ask_owner", "decided", "pin_for_owner", "unpin_for_owner", "owner_items"]);
    let spawn = &tools[0]["inputSchema"];
    assert_eq!(spawn["required"], json!(["name", "profile"]));
    assert_eq!(spawn["properties"]["review_pr"]["type"], "integer");
    assert_eq!(tools[8]["inputSchema"]["properties"]["options"], json!({"type": "array", "items": {"type": "string"}, "description": "Short answers to pick from (2-4)."}));
    assert!(tools[6]["inputSchema"].get("required").is_none());
    assert!(tools.iter().all(|t| !t["description"].as_str().unwrap().is_empty()));
}

#[test]
fn the_worker_tools_call_hc() {
    let dir = tempfile::tempdir().unwrap();
    let (own, _) = files(dir.path());
    let cfg = dir.path().join("agents.json");
    fs::write(&cfg, "{}").unwrap();
    let wt = path(dir.path());
    let vars = [("CONTR_OWNER_FILE", own.as_str()), ("CONTR_AGENTS_CONFIG", path(&cfg)), ("HC_TERMINAL_ID", "t1"), ("HOME", "/home/o")];
    let core = Fake::default()
        .reply("terminal.run", json!({"address": "t4", "name": "b-158", "state": "live", "labels": {"contr": "agent", "role": "builder", "issue": "158"}}))
        .reply("terminal.list", json!({"terminals": [{"address": "t4", "name": "b-158", "state": "live", "labels": {"contr": "agent", "role": "builder"}}]}))
        .reply("terminal.interrupt", json!({"outcome": "stopped"}))
        .reply("terminal.read", json!({"text": "a screen\n"}));
    let got = talk(
        &[
            tool(1, "spawn", json!({"name": "b-158", "profile": "builder", "issue": "158", "worktree": wt, "task": "read TASK.md"})),
            tool(2, "message", json!({"name": "b-158", "text": "fix the test"})),
            tool(3, "interrupt", json!({"name": "b-158"})),
            tool(4, "read", json!({"name": "b-158", "lines": 5})),
            tool(5, "list", json!({})),
            tool(6, "prompt", json!({"name": "b-158"})),
            tool(7, "message", json!({"name": "b-158", "message": "a wrong field"})),
            tool(8, "spawn", json!({"name": "b-159", "profile": "builder", "worktree": wt})),
        ],
        &vars,
        &core,
    );
    let spawned: Value = serde_json::from_str(text(&got[0]).0).unwrap();
    assert_eq!((&spawned["name"], &spawned["address"], &spawned["issue"]), (&json!("b-158"), &json!("t4"), &json!("158")));
    assert_eq!(text(&got[1]), ("sent", false));
    assert_eq!(text(&got[2]), ("stopped", false));
    assert_eq!(text(&got[3]), ("a screen\n", false));
    let list: Value = serde_json::from_str(text(&got[4]).0).unwrap();
    assert_eq!(list[0]["name"], "b-158");
    assert_eq!(text(&got[5]), ("no prompt", false));
    let (bad, is_error) = text(&got[6]);
    assert!(is_error && bad.starts_with("message: bad arguments: unknown field `message`"), "{bad}");
    assert_eq!(text(&got[7]), ("spawn: give the brief as task or task_file", true));

    let calls = core.calls();
    let (method, run) = &calls[0];
    assert_eq!(method, "terminal.run");
    assert_eq!((&run["parent"], &run["name"], &run["cwd"]), (&json!("t1"), &json!("b-158"), &json!(wt)));
    assert_eq!(run["argv"].as_array().unwrap().last().unwrap(), "read TASK.md");
    assert_eq!(calls[1], ("terminal.send".to_string(), json!({"terminal": "b-158", "caller": "t1", "text": "fix the test"})));
    assert_eq!(calls[3], ("terminal.read".to_string(), json!({"terminal": "b-158", "caller": "t1", "lines": 5})));
    // The wrong field and the spawn with no brief reach no terminal.
    assert_eq!(core.methods().iter().filter(|m| *m == "terminal.send" || *m == "terminal.run").count(), 2);
}

#[test]
fn spawn_reads_the_profiles_again_each_time_and_follows_the_flow() {
    let dir = tempfile::tempdir().unwrap();
    let (own, _) = files(dir.path());
    let cfg = dir.path().join("agents.json");
    fs::write(&cfg, "{}").unwrap();
    let wt = path(dir.path());
    let vars = [("CONTR_OWNER_FILE", own.as_str()), ("CONTR_AGENTS_CONFIG", path(&cfg)), ("HC_TERMINAL_ID", "t1"), ("HOME", "/home/o")];
    let core = Fake::default().reply("terminal.run", json!({"address": "t4", "name": "w", "state": "live", "labels": {"contr": "agent"}}));
    let spawn = |profile: &str, any: bool| talk(&[tool(1, "spawn", json!({"name": "w", "profile": profile, "worktree": wt, "task": "go", "any_profile": any}))], &vars, &core).remove(0);

    let (e, is_error) = (spawn("fast", false), true);
    assert!(text(&e).1 == is_error && text(&e).0.contains("no profile \"fast\""), "{e}");
    // The server reads a profile that the file gets later. One server, two calls: the file changes between them.
    fs::write(&cfg, json!({"profiles": {"fast": {"argv": ["codex"]}}, "flow": {"profiles": {"builder": "fast"}}}).to_string()).unwrap();
    assert!(!text(&spawn("fast", false)).1);
    // The flow now names fast for the builder, so the profile builder has no role.
    let e = spawn("builder", false);
    assert!(text(&e).1 && text(&e).0.contains("no role with profile \"builder\""), "{e}");
    assert!(!text(&spawn("builder", true)).1);
    assert_eq!(core.methods(), ["terminal.run", "terminal.run"]);
}

#[test]
fn stop_keeps_the_terminal_when_asked_and_clear_ended_names_what_it_removed() {
    let dir = tempfile::tempdir().unwrap();
    let (own, ag) = files(dir.path());
    let vars = [("CONTR_OWNER_FILE", own.as_str()), ("CONTR_AGENTS_CONFIG", ag.as_str()), ("HC_TERMINAL_ID", "t1")];
    let core = Fake::default().reply("terminal.list", json!({"terminals": [
        {"address": "t5", "name": "b-1", "state": "finished", "parent": "t1", "labels": {"contr": "agent", "role": "builder"}},
    ]}));
    let got = talk(&[tool(1, "stop", json!({"name": "b-2", "keep": true})), tool(2, "stop", json!({"name": "b-3"})), tool(3, "clear_ended", Value::Null)], &vars, &core);
    assert_eq!(text(&got[0]), ("stopped", false));
    assert_eq!(text(&got[1]), ("stopped", false));
    assert_eq!(text(&got[2]), ("[\n  \"b-1\"\n]", false));
    // The kept worker is only killed. The other one and the finished one are removed.
    let rm: Vec<Value> = core.calls().into_iter().filter(|(m, _)| m == "terminal.rm").map(|(_, p)| p["terminal"].clone()).collect();
    assert_eq!(rm, [json!("b-3"), json!("t5")]);

    let none = talk(&[tool(1, "clear_ended", json!({}))], &vars, &Fake::default());
    assert_eq!(text(&none[0]), ("[]", false));
    let failed = talk(&[tool(1, "list", json!({}))], &vars, &Fake::default().err("terminal.list", "hc is down"));
    assert_eq!(text(&failed[0]), ("list: hc is down", true));
}

#[test]
fn the_owner_tools_write_the_owner_file() {
    let dir = tempfile::tempdir().unwrap();
    let (_, ag) = files(dir.path());
    let file = dir.path().join("owner.json");
    let vars = [("CONTR_OWNER_FILE", path(&file)), ("CONTR_AGENTS_CONFIG", ag.as_str()), ("CONTR_PROJECT", "app"), ("HC_TERMINAL_ID", "t2")];
    let core = Fake::default();
    let got = talk(
        &[
            tool(1, "ask_owner", json!({"question": "Merge now?", "options": ["now", "later"], "recommended": 0, "title": "Merge", "from": "PR 12", "impacts": ["risk", "wait"], "refs": ["#12"], "source": "a line", "extra": 1})),
            tool(2, "pin_for_owner", json!({"text": "Test the build", "kind": "do", "from": "b-1"})),
            tool(3, "ask_owner", json!({"question": "Which name?"})),
            tool(4, "decided", json!({"ids": ["D2"], "answer": "the short one", "by": "coordinator"})),
            tool(5, "owner_items", json!({})),
            tool(6, "unpin_for_owner", json!({"ids": ["T1"]})),
            tool(7, "owner_items", json!({"all": true})),
            tool(8, "decided", json!({"ids": ["D9"], "answer": "x", "by": "owner"})),
            tool(9, "ask_owner", json!({"question": 5})),
        ],
        &vars,
        &core,
    );
    assert_eq!([text(&got[0]), text(&got[1]), text(&got[2]), text(&got[3]), text(&got[5])], [("D1", false), ("T1", false), ("D2", false), ("decided", false), ("unpinned", false)]);
    let open: Value = serde_json::from_str(text(&got[4]).0).unwrap();
    assert_eq!((open["questions"].as_array().unwrap().len(), &open["questions"][0]["id"], &open["todos"][0]["text"]), (1, &json!("D1"), &json!("Test the build")));
    let after: Value = serde_json::from_str(text(&got[6]).0).unwrap();
    assert_eq!((after["questions"].as_array().unwrap().len(), after["todos"].as_array().unwrap().len()), (1, 0));
    assert!(text(&got[7]).1 && text(&got[7]).0.starts_with("decided: "), "{}", got[7]);
    assert!(text(&got[8]).1 && text(&got[8]).0.starts_with("ask_owner: bad arguments: "), "{}", got[8]);

    let state = Store::new(&file).read().unwrap();
    let q = &state.questions[0];
    assert_eq!((q.project.as_str(), q.coordinator.as_str(), q.title.as_str(), q.from.as_str()), ("app", "t2", "Merge", "PR 12"));
    assert_eq!((q.recommended, q.impacts.len(), q.refs[0].as_str(), q.source.as_str()), (Some(0), 2, "#12", "a line"));
    assert_eq!((state.questions[1].answer.as_str(), state.questions[1].answered_by.as_str()), ("the short one", "coordinator"));
    // The owner tools need no hc core.
    assert!(core.calls().is_empty());
}

#[test]
fn a_broken_profiles_file_stops_the_server_at_its_start() {
    let dir = tempfile::tempdir().unwrap();
    let (own, _) = files(dir.path());
    let cfg = dir.path().join("agents.json");
    fs::write(&cfg, "{not json").unwrap();
    let vars = [("CONTR_OWNER_FILE", own.as_str()), ("CONTR_AGENTS_CONFIG", path(&cfg))];
    let r = call_with(&["mcp"], &vars, &Fake::default(), "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n");
    assert_eq!((r.code, r.out.as_str()), (1, ""));
    assert!(r.err.starts_with("herdr-contr-orch mcp: "), "{}", r.err);
    let r = call_with(&["mcp", "more"], &vars, &Fake::default(), "");
    assert_eq!(r.code, 2, "{}", r.err);
}
