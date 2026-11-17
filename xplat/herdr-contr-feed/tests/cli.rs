//! The built binary end to end, on a fake herdr-core: the protocol's header and end, errors, and a card.
#![cfg(unix)]

mod common;
use common::{claude, FakeHc};
use serde_json::Value;
use std::process::Command;

fn run(sock: &str, args: &[&str]) -> Vec<Value> {
    let out = Command::new(env!("CARGO_BIN_EXE_herdr-contr-feed"))
        .args(args)
        .env_clear()
        .env("HC_SOCKET", sock)
        .env("HOME", "/nonexistent-home")
        .output()
        .unwrap();
    assert!(out.status.success(), "exit code 0 always");
    String::from_utf8(out.stdout).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect()
}

#[test]
fn protocol_lines() {
    let v = run("/nonexistent.sock", &["version"]);
    assert_eq!(v[0], serde_json::json!({"type":"header","protocol":1,"sidecar":"0.1.0","build":env!("HERDR_CONTR_BUILD"),"cmd":"version"}));
    assert_eq!(v[1]["commands"], serde_json::json!(["version", "feed", "status", "card", "approve", "answer", "send", "paste-image", "mermaid", "pictures", "helper", "queue", "phone", "commands", "working", "model", "usage", "favicon", "panel", "keys", "suggestion", "agent-images", "update", "health", "statusline"]));
    assert_eq!(v.last().unwrap()["type"], "end");
    assert_eq!(v.last().unwrap()["ok"], true);

    let u = run("/nonexistent.sock", &["nope"]);
    assert_eq!(u[1]["code"], "usage");
    assert_eq!(u.last().unwrap()["ok"], false);
    assert_eq!(run("/nonexistent.sock", &["feed"])[1]["message"], "feed needs --terminal <addr>");
    assert_eq!(run("/nonexistent.sock", &["feed", "--terminal", "t1"])[1]["code"], "no-core");

    // mermaid: the options before --text, the text as it is (#87).
    assert_eq!(run("/nonexistent.sock", &["mermaid", "--dir", "x"])[1]["message"], "mermaid needs --text <s>");
    assert_eq!(run("/nonexistent.sock", &["mermaid", "--text", "--dir"])[1]["message"], "mermaid needs --dir <folder>");
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path().to_str().unwrap();
    let m = run("/nonexistent.sock", &["mermaid", "--dir", d, "--dark", "--fg", "#eeeeee", "--text", "flowchart LR\n  A --> B"]);
    assert_eq!(m[1]["type"], "mermaid", "{m:?}");
    assert!(m[1]["path"].as_str().unwrap().starts_with(d) && m[1]["path"].as_str().unwrap().ends_with(".svg"));
    assert!(m[1]["width"].as_f64().unwrap() > 0.0 && m[1]["height"].as_f64().unwrap() > 0.0);
    assert!(std::fs::read_to_string(m[1]["path"].as_str().unwrap()).unwrap().contains("#eeeeee"));
    let bad = run("/nonexistent.sock", &["mermaid", "--dir", d, "--text", "not a diagram (("]);
    assert_eq!((bad[1]["code"].as_str(), bad.last().unwrap()["ok"].as_bool()), (Some("parse"), Some(false)));

    let fake = FakeHc::start();
    fake.set_terminals(vec![claude("t1", "blocked", "permission"), common::terminal("t2", &["zsh"], Some(9), None, None)]);
    fake.set_screen("t1", "Bash command\n  rm -rf build\nDo you want to proceed?\n❯ 1. Yes\n  2. No");
    assert_eq!(run(&fake.path, &["card", "--terminal", "t9"])[1]["code"], "no-terminal");
    assert_eq!(run(&fake.path, &["card", "--terminal", "t2"])[1]["code"], "not-agent");
    let c = run(&fake.path, &["card", "--terminal", "t1"]);
    assert_eq!(c[1]["type"], "card");
    assert_eq!(c[1]["kind"], "approval");
    assert_eq!(c[1]["tool"], "Bash");
    assert_eq!(c[1]["command"], "rm -rf build");
    assert!(fake.keys().is_empty(), "card sends no keys");
    assert_eq!(run(&fake.path, &["approve", "--terminal", "t1", "--request", "x"])[1]["code"], "usage");
    let a = run(&fake.path, &["approve", "--terminal", "hc:t1", "--request", c[1]["request"].as_str().unwrap(), "--deny"]);
    assert_eq!(a[1], serde_json::json!({"type":"done","result":"sent"}));
    assert_eq!(fake.keys(), vec![vec!["esc".to_string()]]);
    // No log found: path null, nothing to splice.
    let f = run(&fake.path, &["feed", "--terminal", "t1"]);
    assert_eq!(f[1], serde_json::json!({"type":"log","agent":"claude","path":null}));
    assert_eq!(f[2], serde_json::json!({"type":"splice","at":0,"count":0}));
}

/// `commands`: a Claude Code terminal's "/" commands, read from files in its folder and the user's home. No key goes
/// to the agent. Another agent: an empty list (the mod shows nothing).
#[test]
fn commands_lists_a_claude_terminals_slash_commands() {
    let t = tempfile::tempdir().unwrap();
    let (home, proj) = (t.path().join("home"), t.path().join("proj"));
    std::fs::create_dir_all(proj.join(".claude/commands")).unwrap();
    std::fs::write(proj.join(".claude/commands/deploy.md"), "---\ndescription: Deploy the app\n---\n").unwrap();
    std::fs::create_dir_all(home.join(".claude/commands")).unwrap();
    std::fs::write(home.join(".claude/commands/review.md"), "Review the diff.").unwrap();
    let fake = FakeHc::start();
    let mut c = claude("t1", "idle", "idle");
    c["cwd"] = serde_json::json!(proj);
    let mut x = common::terminal("t2", &["codex"], Some(77), Some("idle"), None);
    x["cwd"] = serde_json::json!(proj);
    fake.set_terminals(vec![c, x, common::terminal("t3", &["zsh"], Some(9), None, None)]);
    let run_home = |args: &[&str]| -> Vec<Value> {
        let out = Command::new(env!("CARGO_BIN_EXE_herdr-contr-feed")).args(args).env_clear().env("HC_SOCKET", &fake.path).env("HOME", &home).output().unwrap();
        String::from_utf8(out.stdout).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect()
    };
    let v = run_home(&["commands", "--terminal", "t1"]);
    assert_eq!(v[1]["type"], "commands");
    assert_eq!(v[1]["agent"], "claude");
    let cmds = v[1]["commands"].as_array().unwrap();
    let own: Vec<&Value> = cmds.iter().filter(|c| c["source"] != "builtin").collect();
    assert_eq!(own, vec![&serde_json::json!({"name": "deploy", "description": "Deploy the app", "source": "project"})], "the user's review is hidden by the built-in /review");
    let model = cmds.iter().find(|c| c["name"] == "model").unwrap();
    assert_eq!(model["args"][1], serde_json::json!({"value": "claude-opus-5-5", "label": "Opus 5.5"}), "full ids, with labels");
    assert!(model["args"].as_array().unwrap().contains(&serde_json::json!({"value": "opus", "label": "The latest Opus"})));
    let effort = cmds.iter().find(|c| c["name"] == "effort").unwrap();
    assert_eq!(effort["args"][0], serde_json::json!({"value": "low"}), "no label where the value says it all");
    assert_eq!(model["screen"], true);
    assert_eq!(v.last().unwrap()["ok"], true);
    let o = run_home(&["commands", "--terminal", "t2"]);
    assert_eq!(o[1]["agent"], "codex");
    let codex: Vec<&str> = o[1]["commands"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap()).collect();
    assert!(codex.contains(&"compact") && !codex.contains(&"deploy"), "Codex: its built-ins, not Claude's files: {codex:?}");
    assert!(!codex.contains(&"model"), "Codex's /model takes no value: it can never be sent from the chat");
    assert_eq!(o[1]["screens"], serde_json::json!(["config", "model", "permissions", "resume", "skills"]));
    assert!(v[1]["screens"].as_array().unwrap().contains(&serde_json::json!("status")));
    assert_eq!(run_home(&["commands", "--terminal", "t3"])[1]["code"], "not-agent");
    assert_eq!(run_home(&["commands"])[1]["message"], "commands needs --terminal <addr>");
    assert!(fake.keys().is_empty(), "commands sends no keys");
}

/// `send` of a screen command sent bare ("/model", "/status"): refused with `opens_screen`, nothing goes to the
/// agent. With a value, or for an agent whose command prints in the chat, it goes out.
#[test]
fn send_refuses_a_bare_screen_command() {
    let fake = FakeHc::start();
    fake.set_terminals(vec![claude("t1", "idle", "idle"), common::terminal("t2", &["codex"], Some(77), Some("idle"), None)]);
    let send = |t: &str, text: &str| -> Value {
        let v = run(&fake.path, &["send", "--terminal", t, "--text", text]);
        v.into_iter().find(|l| l["type"] == "done").unwrap()
    };
    let d = send("t1", " /model ");
    assert_eq!(d["result"], "opens_screen");
    assert_eq!(d["message"], "/model opens a menu in the terminal. Add a value, like /model claude-fable-5-2.");
    assert_eq!(send("t1", "/status")["message"], "/status opens a screen in the terminal. Use it there.");
    assert_eq!(send("t2", "/model")["result"], "opens_screen");
    assert!(fake.calls("terminal.send").is_empty(), "nothing was typed");
    assert_ne!(send("t1", "/model sonnet")["result"], "opens_screen");
    assert_ne!(send("t2", "/status")["result"], "opens_screen", "Codex's /status prints in the chat");
}

/// `send` reads the agent's input box first: text in it (a half-typed "/") and nothing is typed (`box_not_empty`).
/// An empty box, its placeholder, or no screen to read: it goes as before.
#[test]
fn send_refuses_when_the_agents_box_has_text() {
    let fake = FakeHc::start();
    fake.set_terminals(vec![claude("t1", "idle", "idle")]);
    let rule = "─".repeat(40);
    let screen = |row: &str| format!("● Done.\n\n{rule}\n{row}\n{rule}\n  ~/proj\n");
    let send = || -> Value { run(&fake.path, &["send", "--terminal", "t1", "--text", "/compact"]).into_iter().find(|l| l["type"] == "done").unwrap() };
    fake.set_screen("t1", &screen("❯\u{a0}/"));
    let d = send();
    assert_eq!(d["result"], "box_not_empty");
    assert_eq!(d["message"], "The agent's box has text in it. Clear it in the terminal first. Nothing was sent.");
    assert!(fake.calls("terminal.send").is_empty(), "nothing was typed");
    fake.set_screen("t1", &screen("❯\u{a0}Try \"fix typecheck errors\""));
    assert_ne!(send()["result"], "box_not_empty", "only its placeholder: empty");
    let typed = fake.calls("terminal.send").len();
    assert!(typed >= 1);
    fake.clear_screen("t1");
    assert_ne!(send()["result"], "box_not_empty", "no screen: it goes as before");
    assert!(fake.calls("terminal.send").len() > typed);
}

#[test]
fn usage_reads_the_fake_homes_only() {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(d.path().join("state/usage")).unwrap();
    std::fs::write(d.path().join("state/usage/claude.json"), r#"{"schema":1,"updated_at":7,"windows":[{"kind":"five_hour","used_percent":38,"resets_at":100}]}"#).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_herdr-contr-feed"))
        .arg("usage")
        .env_clear()
        .env("HOME", d.path().join("home"))
        .env("CONTR_STATE_DIR", d.path().join("state"))
        .env("CODEX_HOME", d.path().join("codex"))
        .env("SHELL", "/usr/bin/false")
        .output()
        .unwrap();
    let v: Vec<Value> = String::from_utf8(out.stdout).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(v[1], serde_json::json!({"type": "usage", "plans": [{"plan": "claude", "windows": [{"kind": "five_hour", "used_percent": 38.0, "resets_at": 100}], "read_at": 7}]}));
    assert_eq!(v.last().unwrap()["ok"], true);
}

#[test]
fn usage_reads_the_status_line_records_in_contr_home() {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(d.path().join("state/usage")).unwrap();
    std::fs::write(d.path().join("state/usage/claude.json"), r#"{"schema":1,"updated_at":7,"windows":[{"kind":"seven_day","used_percent":0,"resets_at":100}]}"#).unwrap();
    std::fs::create_dir_all(d.path().join("root/statusline")).unwrap();
    std::fs::write(d.path().join("root/statusline/sess-1.json"), r#"{"session_id":"sess-1","rate_limits":{"seven_day":{"used_percentage":17,"resets_at":1795453200}}}"#).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_herdr-contr-feed"))
        .arg("usage")
        .env_clear()
        .env("HOME", d.path().join("home"))
        .env("CONTR_HOME", d.path().join("root"))
        .env("CONTR_STATE_DIR", d.path().join("state"))
        .env("CODEX_HOME", d.path().join("codex"))
        .env("SHELL", "/usr/bin/false")
        .output()
        .unwrap();
    let v: Vec<Value> = String::from_utf8(out.stdout).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    let plan = &v[1]["plans"][0];
    assert_eq!(plan["windows"], serde_json::json!([{"kind": "seven_day", "used_percent": 17.0, "resets_at": 1795453200}]));
    assert!(plan["read_at"].as_u64().unwrap() > 7, "the record's file time");
}

#[test]
fn favicon_copies_under_the_dir() {
    let d = std::env::temp_dir().join(format!("hcf-cli-favicon-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("public")).unwrap();
    let src = d.join("public/apple-touch-icon.png");
    std::fs::write(&src, b"png").unwrap();
    let cache = d.join("home/herdr-contr/favicons");
    let v = run("/nonexistent.sock", &["favicon", "--path", src.to_str().unwrap(), "--dir", cache.to_str().unwrap()]);
    let p = v[1]["path"].as_str().unwrap();
    assert!(p.starts_with(cache.to_str().unwrap()) && p.ends_with(".png"));
    assert_eq!(std::fs::read(p).unwrap(), b"png");
    let none = run("/nonexistent.sock", &["favicon", "--path", d.join("gone.png").to_str().unwrap(), "--dir", cache.to_str().unwrap()]);
    assert_eq!(none[1], serde_json::json!({"type": "favicon", "path": null}));
    let usage = run("/nonexistent.sock", &["favicon", "--path", src.to_str().unwrap()]);
    assert_eq!(usage[1]["type"], "error");
    assert_eq!(usage[1]["code"], "usage");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn agent_images_copies_under_the_dir() {
    let d = std::env::temp_dir().join(format!("hcf-cli-agentimg-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("proj")).unwrap();
    std::fs::write(d.join("proj/chart.svg"), r#"<svg width="120" height="60"></svg>"#).unwrap();
    let log = d.join("s.jsonl");
    std::fs::write(&log, format!("{}\n", serde_json::json!({"type": "user", "cwd": d.join("proj")}))).unwrap();
    let cache = d.join("home/herdr-contr/images");
    let (l, c) = (log.to_str().unwrap(), cache.to_str().unwrap());
    let v = run("/nonexistent.sock", &["agent-images", "--log", l, "--dir", c, "--srcs", r#"["chart.svg","https://x.dev/a.png"]"#]);
    let p = v[1]["path"].as_str().unwrap();
    assert!(p.starts_with(c) && p.ends_with(".svg"));
    assert_eq!((v[1]["width"].as_f64(), v[1]["height"].as_f64()), (Some(120.0), Some(60.0)));
    assert_eq!(v[2], serde_json::json!({"type": "agent-image", "index": 1, "path": null}));
    assert_eq!(v.last().unwrap()["ok"], true);
    for bad in [vec!["agent-images", "--log", l, "--dir", c], vec!["agent-images", "--log", l, "--dir", c, "--srcs", "[1]"]] {
        let e = run("/nonexistent.sock", &bad);
        assert_eq!((e[1]["type"].as_str(), e[1]["code"].as_str()), (Some("error"), Some("usage")));
    }
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn version_flag_prints_the_build_stamp() {
    let out = Command::new(env!("CARGO_BIN_EXE_herdr-contr-feed")).arg("--version").env_clear().output().unwrap();
    assert!(out.status.success());
    let stamp = env!("HERDR_CONTR_BUILD");
    assert_ne!(stamp, "", "build.rs sets the stamp");
    assert_eq!(String::from_utf8(out.stdout).unwrap(), format!("herdr-contr-feed 0.1.0 ({stamp})\n"));
}

#[test]
#[cfg(feature = "herdr-mod")]
fn health_gives_a_check_line_for_each_check() {
    let home = tempfile::tempdir().unwrap();
    let h = home.path().to_str().unwrap();
    let v = run("/nonexistent.sock", &["health", "--layout", "main", "--herdr", "0.1.16", "--home", h]);
    let ids: Vec<&str> = v.iter().filter(|l| l["type"] == "check").map(|l| l["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["mods", "layout", "collisions", "orch", "hc", "kit", "herdr"]);
    let state = |id: &str| v.iter().find(|l| l["id"] == id).unwrap()["state"].as_str().unwrap().to_string();
    assert_eq!(state("mods"), "fail", "an empty home has no install");
    assert_eq!(state("hc"), "fail", "no socket");
    assert_eq!(state("layout"), "warn");
    assert_eq!(state("herdr"), "ok");
    assert_eq!(v.last().unwrap()["ok"], true);
}

/// `health` with no --home reads the checks orch and kit in CONTR_HOME, not in HERDR_APP_HOME's herdr-contr.
#[test]
fn health_reads_orch_and_kit_in_contr_home() {
    use std::os::unix::fs::PermissionsExt;
    let t = tempfile::tempdir().unwrap();
    let (r, h, claude) = (t.path().join("r"), t.path().join("h"), t.path().join("claude"));
    for d in [r.join("kit/.claude-plugin"), r.join(".claude-plugin"), r.join("bin"), h.clone(), claude.join("plugins")] {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::write(r.join("kit/.claude-plugin/plugin.json"), "{}").unwrap();
    std::fs::write(r.join(".claude-plugin/marketplace.json"), "{}").unwrap();
    let orch = r.join("bin/herdr-contr-orch");
    std::fs::write(&orch, format!("#!/bin/sh\necho 'herdr-contr-orch 0.1.0 ({})'\n", env!("HERDR_CONTR_BUILD"))).unwrap();
    std::fs::set_permissions(&orch, std::fs::Permissions::from_mode(0o755)).unwrap();
    let known = serde_json::json!({"contr": {"source": {"path": r.display().to_string()}}});
    std::fs::write(claude.join("plugins/known_marketplaces.json"), known.to_string()).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_herdr-contr-feed"))
        .arg("health")
        .env_clear()
        .env("HC_SOCKET", "/nonexistent.sock")
        .env("HOME", "/nonexistent-home")
        .env("CONTR_HOME", &r)
        .env("HERDR_APP_HOME", &h)
        .env("CLAUDE_CONFIG_DIR", &claude)
        .output()
        .unwrap();
    let v: Vec<Value> = String::from_utf8(out.stdout).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    let check = |id: &str| v.iter().find(|l| l["id"] == id).unwrap_or_else(|| panic!("no {id}: {v:?}")).clone();
    assert_eq!(check("orch")["state"], "ok", "{:?}", check("orch"));
    assert_eq!(check("kit")["state"], "ok", "{:?}", check("kit"));
}

#[test]
fn statusline_switch_and_wrapper_end_to_end() {
    let t = tempfile::tempdir().unwrap();
    let s = t.path().join("settings.json");
    std::fs::write(&s, "{\"statusLine\": {\"type\": \"command\", \"command\": \"echo mine\"}}\n").unwrap();
    let (sp, home) = (s.to_str().unwrap(), t.path().join("home"));
    let v = run("/nonexistent.sock", &["statusline", "on", "--settings", sp, "--home", home.to_str().unwrap()]);
    assert_eq!(v[1]["type"], "statusline");
    assert_eq!((v[1]["on"].clone(), v[1]["own"].clone()), (serde_json::json!(true), serde_json::json!("echo mine")));
    let cfg: Value = serde_json::from_str(&std::fs::read_to_string(&s).unwrap()).unwrap();
    let cmd = cfg["statusLine"]["command"].as_str().unwrap();
    let out = Command::new("/bin/sh").arg("-c").arg(cmd).stdin(std::fs::File::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../fixtures/statusline-fable.json")).unwrap()).output().unwrap();
    assert_eq!(out.stdout, b"mine\n");
    assert!(home.join("herdr-contr/statusline/fable-1.json").exists());
    let v = run("/nonexistent.sock", &["statusline", "off", "--settings", sp]);
    assert_eq!(v[1]["on"], false);
    assert_eq!(std::fs::read_to_string(&s).unwrap(), "{\"statusLine\": {\"type\": \"command\", \"command\": \"echo mine\"}}\n");
    std::fs::write(&s, "nope").unwrap();
    let v = run("/nonexistent.sock", &["statusline", "status", "--settings", sp]);
    assert_eq!((v[1]["type"].clone(), v[1]["code"].clone()), (serde_json::json!("error"), serde_json::json!("settings")));
    let v = run("/nonexistent.sock", &["statusline", "toggle", "--settings", sp]);
    assert_eq!(v[1]["code"], "usage");
}
