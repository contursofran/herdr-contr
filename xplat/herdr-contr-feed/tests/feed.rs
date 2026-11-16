//! Swift: `ChecksModFeed.swift`. Rows, footers, the tail reader, the incremental feed, status tags, locate.

use chrono::{TimeZone, Utc};
use herdr_core::util::Date;
use herdr_core::*;
use herdr_core::statusline::StatusLineFolder;
use modfeedkit::protocol::JsonLines;
use modfeedkit::{feed, locate, paste, rows, status, tail};
use std::io::Write;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn ts(s: u32) -> String {
    format!("2026-11-03T01:{:02}:{:02}.000Z", 40 + s / 60, s % 60)
}
fn user(id: &str, t: u32, text: &str) -> String {
    format!(r#"{{"type":"user","uuid":"{id}","timestamp":"{}","message":{{"role":"user","content":"{text}"}}}}"#, ts(t))
}
fn say(id: &str, t: u32, text: &str) -> String {
    format!(r#"{{"type":"assistant","uuid":"{id}","timestamp":"{}","message":{{"role":"assistant","model":"claude-test-1","content":[{{"type":"text","text":"{text}"}}]}}}}"#, ts(t))
}
fn tool(id: &str, t: u32, call: &str, cmd: &str) -> String {
    format!(r#"{{"type":"assistant","uuid":"{id}","timestamp":"{}","message":{{"role":"assistant","content":[{{"type":"tool_use","id":"{call}","name":"Bash","input":{{"command":"{cmd}"}}}}]}}}}"#, ts(t))
}
fn result(id: &str, t: u32, call: &str) -> String {
    format!(r#"{{"type":"user","uuid":"{id}","timestamp":"{}","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"{call}","content":"ok"}}]}}}}"#, ts(t))
}
fn turn() -> Vec<String> {
    vec![
        user("u1", 0, "Fix it"),
        tool("a1", 5, "c1", "ls"),
        result("r1", 8, "c1"),
        tool("a2", 10, "c2", "swift build"),
        result("r2", 20, "c2"),
        tool("a3", 21, "c3", "git diff"),
        result("r3", 25, "c3"),
        say("a4", 36, "Done."),
    ]
}
fn now() -> Date {
    Utc.timestamp_opt(1_793_670_000, 0).unwrap()
}

#[test]
fn rows_footer_separator() {
    let rs = rows::rows(&tail::items(&turn()), &now(), &Utc);
    let ks: Vec<&str> = rs.iter().map(|r| r.k.as_str()).collect();
    assert_eq!(ks, ["user", "tools", "assistant", "footer"], "modfeed rows: user, one tools row, reply, footer");
    let tools = rs.iter().find(|r| r.k == "tools").unwrap();
    let calls: Vec<_> = tools.chips.as_ref().unwrap().iter().flat_map(|c| c.calls.iter()).collect();
    assert_eq!(calls.len(), 3, "modfeed rows: the tools row holds the 3 calls");
    assert!(calls.iter().all(|c| c.done), "modfeed rows: results mark calls done");
    assert_eq!(rs.last().unwrap().text.as_deref(), Some("01:40 · Worked 36s · 3 tools"), "modfeed footer");
    assert_eq!(rs.last().unwrap().id, format!("f-{}", rs[rs.len() - 2].id), "modfeed footer: keyed by the turn's last row");

    let gap = vec![user("u1", 0, "a"), say("a1", 2, "b"), user("u2", 1000, "c")];
    let g = rows::rows(&tail::items(&gap), &now(), &Utc);
    let ks: Vec<&str> = g.iter().map(|r| r.k.as_str()).collect();
    assert_eq!(ks, ["user", "assistant", "footer", "sep", "user"], "modfeed separator after 15 min");
}

#[test]
fn hc_note_rows_carry_the_sender() {
    let item = |id: &str, t: &str| ConversationItem::new(id, ItemKind::UserText(t.into()), Some(1_793_670_000.0));
    let json = |it: &ConversationItem| serde_json::to_value(rows::row(it)).unwrap();
    let note = json(&item("u1", "<hc from=\"t2 docs-worker\">Done: notes updated.\nSee notes.md</hc>"));
    assert_eq!(note["k"], "user", "hc note: still a user row (older mods draw it as a bubble)");
    assert_eq!(note["from"], "t2 docs-worker", "hc note: the sender as hc wrote it");
    assert_eq!(note["text"], "Done: notes updated.\nSee notes.md", "hc note: the text without its tag");
    let reach = json(&item("u2", "<hc from=\"t4 review\" reach=\"hc send t1 when done or stuck\">Review x.rs</hc>"));
    assert_eq!((reach["from"].as_str(), reach["text"].as_str()), (Some("t4 review"), Some("Review x.rs")), "hc note: other attributes");
    let bare = json(&item("u3", "<hc>A note</hc>"));
    assert_eq!((bare["from"].as_str(), bare["text"].as_str()), (Some(""), Some("A note")), "hc note: no sender");
    for typed in ["Fix it", "<hc from=\"t2\">a</hc> and more I typed", "see <hc from=\"t2\">a</hc>", "<hcx>a</hcx>"] {
        let r = json(&item("u4", typed));
        assert!(r.get("from").is_none() && r["text"] == typed, "hc note: typed text stays as it is: {typed}");
    }
}

#[test]
fn tail_older_paste() {
    let dir = tempfile::tempdir().unwrap();
    let t = turn();
    let f = dir.path().join("s.jsonl");
    std::fs::write(&f, t.join("\n") + "\n" + r#"{"type":"user","par"#).unwrap();
    let all = tail::read(&f, None, tail::BYTES).unwrap();
    assert!(all.lines.len() == t.len() && all.start == 0, "modfeed tail: whole lines, unfinished one left out");
    let cut = tail::read(&f, None, (t.last().unwrap().len() + 30) as u64).unwrap();
    assert_eq!(cut.lines, vec![t.last().unwrap().clone()], "modfeed tail: a cut first line is dropped");
    assert_eq!(cut.start, (t[..t.len() - 1].join("\n").len() + 1) as u64, "modfeed tail: start is the first whole line");
    assert_eq!(tail::model(&t).as_deref(), Some("claude-test-1"), "modfeed tail: model from the newest assistant line");

    // Paste: a new PNG file in the folder; pastes older than a day go.
    let pdir = dir.path().join("paste");
    let t0 = UNIX_EPOCH + Duration::from_secs(1_000_000);
    let p1 = paste::save(&[1, 2, 3], &pdir, t0).unwrap();
    std::fs::File::options().write(true).open(&p1).unwrap().set_modified(t0).unwrap();
    let p2 = paste::save(&[4], &pdir, t0 + Duration::from_secs(2 * 86400)).unwrap();
    assert!(!std::path::Path::new(&p1).exists() && std::fs::read(&p2).unwrap() == vec![4] && p2.ends_with(".png"), "modfeed paste");

    // Older rows.
    let before = tail::read_before(&f, cut.start, (t[6].len() + t[5].len() + 5) as u64).unwrap();
    assert_eq!(before.lines, vec![t[5].clone(), t[6].clone()], "modfeed older: lines before the tail, cut first one dropped");
    let first = tail::read_before(&f, cut.start, tail::BYTES).unwrap();
    assert!(first.start == 0 && first.lines == t[..t.len() - 1].to_vec(), "modfeed older: reaches the file's start");
    let mut oo = JsonLines::new();
    feed::older(&f, cut.start, false, &mut oo, &now(), &Utc).unwrap();
    assert!(oo.lines[0].contains(r#""type":"older""#) && oo.lines[0].contains(r#""start":0"#), "modfeed older: {}", oo.lines[0]);
    assert!(oo.lines.len() > 1 && oo.lines[1..].iter().all(|l| l.contains(r#""type":"row""#)), "modfeed older: then rows");
}

#[test]
fn older_moves_back_past_a_line_longer_than_the_span() {
    // A tool result with a screenshot can be one line longer than the 2 MB read before `before`. Its bytes hold no
    // newline: the range must still move back (to that line's start) instead of staying at `before`.
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("big.jsonl");
    let small = r#"{"type":"user","uuid":"u1","message":{"content":"first"}}"#;
    let big = format!(r#"{{"type":"user","uuid":"r1","message":{{"content":"{}"}}}}"#, "x".repeat(5000));
    let last = r#"{"type":"user","uuid":"u2","message":{"content":"last"}}"#;
    std::fs::write(&f, format!("{small}\n{big}\n{last}\n")).unwrap();
    let before = (small.len() + 1 + big.len() + 1) as u64;   // the start of `last`
    let r = tail::read_before(&f, before, 1000).unwrap();
    assert_eq!(r.start, (small.len() + 1) as u64, "modfeed older: start is the long line's start");
    assert_eq!(r.lines, vec![big.clone()], "modfeed older: the long line is read whole");
    let r2 = tail::read_before(&f, r.start, 1000).unwrap();
    assert!(r2.start == 0 && r2.lines == vec![small.to_string()], "modfeed older: then on back to the start");
    // Longer than the cap: not read, but start still moves to its start.
    let skip = tail::read_before_max(&f, before, 1000, 2000).unwrap();
    assert!(skip.lines.is_empty() && skip.start == (small.len() + 1) as u64, "modfeed older: a line over the cap is skipped: {skip:?}");
}

#[test]
fn feed_splices() {
    let dir = tempfile::tempdir().unwrap();
    let t = turn();
    let f = dir.path().join("s.jsonl");
    std::fs::write(&f, t[..5].join("\n") + "\n").unwrap();
    let mut o1 = JsonLines::new();
    let s1 = feed::run(Some(&f), "claude", None, None, false, &mut o1, &now(), &Utc).unwrap();
    assert!(o1.lines.iter().any(|l| l.contains(r#""at":0"#) && l.contains(r#""type":"splice""#)), "first answer splices at 0");
    assert_eq!(o1.lines.iter().filter(|l| l.contains(r#""type":"row""#)).count(), 2, "user + tools row");
    let c1 = s1.as_ref().unwrap().cursor();
    let mut o2 = JsonLines::new();
    let s2 = feed::run(Some(&f), "claude", Some(&c1), s1.as_ref(), false, &mut o2, &now(), &Utc).unwrap();
    assert!(o2.lines.contains(&r#"{"type":"unchanged"}"#.to_string()) && s2 == s1, "same size, unchanged");
    let mut h = std::fs::OpenOptions::new().append(true).open(&f).unwrap();
    h.write_all((t[5..].join("\n") + "\n").as_bytes()).unwrap();
    drop(h);
    let mut o3 = JsonLines::new();
    let s3 = feed::run(Some(&f), "claude", Some(&c1), s1.as_ref(), false, &mut o3, &now(), &Utc).unwrap();
    let splice3 = o3.lines.iter().find(|l| l.contains(r#""type":"splice""#)).unwrap();
    assert!(splice3.contains(r#""at":1"#) && splice3.contains(r#""count":4"#), "growth splices at the changed tools row ({splice3})");
    assert_eq!(o3.lines.iter().filter(|l| l.contains(r#""type":"row""#)).count(), 3, "prints rows from the splice on");
    assert_eq!(s3.as_ref().unwrap().start, s1.as_ref().unwrap().start, "keeps the range start while the log grows");
    let mut o4 = JsonLines::new();
    feed::run(Some(&f), "claude", Some("1.bad.0.0.x"), s3.as_ref(), false, &mut o4, &now(), &Utc).unwrap();
    assert!(o4.lines.iter().any(|l| l.contains(r#""at":0"#) && l.contains(r#""type":"splice""#)), "an unknown cursor gets everything again");
    let mut o5 = JsonLines::new();
    feed::run(None, "codex", None, None, false, &mut o5, &now(), &Utc).unwrap();
    assert!(o5.lines[0].contains(r#""path":null"#), "no log, path null");

    // Locate: cache round trip (with the feed state).
    let pane = Pane {
        id: "hc:t7".into(),
        terminal_id: "t7".into(),
        cwd: "/w".into(),
        agent: Some(Agent::new("c", "claude", AgentStatus::Idle)),
        label: None,
        backend: BackendKind::Hc,
        process_ids: vec![42, 41],
    };
    let ws = vec![Workspace { id: "w".into(), label: "w".into(), tabs: vec![Tab { id: "t".into(), label: "t".into(), panes: vec![pane.clone()], ..Default::default() }] }];
    assert!(locate::pane(&ws, "t7").map(|p| p.id.as_str()) == Some("hc:t7") && locate::pane(&ws, "hc:t7").is_some(), "locate: by address or pane id");
    let now_s = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs_f64();
    let log = dir.path().join("sess-1.jsonl");
    std::fs::write(&log, t.join("\n") + "\n").unwrap();
    let e = locate::Entry { kind: "claude".into(), cwd: "/w".into(), pids: vec![42, 41], path: Some(log.to_string_lossy().into()), checked: now_s - 5.0 };
    assert!(locate::fresh(Some(&e), &pane, "claude", now_s), "locate: fresh entry used");
    let mut other = e.clone();
    other.pids = vec![43, 41];
    assert!(!locate::fresh(Some(&other), &pane, "claude", now_s), "locate: other processes, looked up again");
    assert!(!locate::fresh(Some(&e), &pane, "claude", now_s + 40.0), "locate: old entry, looked up again");
    let mut cache = locate::Cache::default();
    cache.entries.insert("t7".into(), e);
    cache.feeds.insert("t7".into(), s3.unwrap());
    let cf = dir.path().join("c/cache.json");
    locate::write_cache(&cache, Some(&cf));
    assert_eq!(locate::read_cache(Some(&cf)), cache, "locate: cache round trip");
}

#[test]
fn status_tags() {
    let words = status::shell_words(r#"'/opt/a b/herdr-contr-statusline' --dir "/tmp/x y" --x\ z"#);
    assert_eq!(words, ["/opt/a b/herdr-contr-statusline", "--dir", "/tmp/x y", "--x z"], "modfeed status: shell words");
    let dir = tempfile::tempdir().unwrap();
    let sdir = dir.path().join("status dir");
    let settings = dir.path().join("settings.json");
    let cmd = format!("'/opt/herdr-contr-statusline' --dir '{}'", sdir.display());
    std::fs::write(&settings, serde_json::json!({"statusLine": {"type": "command", "command": cmd}}).to_string()).unwrap();
    assert_eq!(status::folder(&settings), Some(sdir.clone()), "modfeed status: wrapper folder from settings");
    let log = dir.path().join("sess-1.jsonl");
    std::fs::write(&log, turn().join("\n") + "\n").unwrap();
    StatusLineFolder::new(&sdir).save(
        br#"{"session_id":"sess-1","model":{"display_name":"Opus 9"},"effort":{"level":"high"},"context_window":{"used_percentage":84}}"#,
        b"own part",
        "sess-1",
    );
    let mut o6 = JsonLines::new();
    status::run("claude", Some(&log), &settings, None, &mut o6, SystemTime::now());
    let st = &o6.lines[0];
    assert!(
        st.contains(r#""source":"statusline""#) && st.contains(r#""text":"Opus 9""#) && st.contains(r#""text":"ctx 84%""#) && st.contains(r#""warn":true"#),
        "modfeed status: wrapper facts, full ctx warns ({st})"
    );
    // autoCompactWindow: tokens used against it; current_usage first, else the percent of the window.
    let cmd2 = serde_json::json!({"statusLine": {"type": "command", "command": cmd}, "autoCompactWindow": 250000});
    std::fs::write(&settings, cmd2.to_string()).unwrap();
    assert_eq!(status::compact_limit(&[settings.clone()], None), Some(250000), "modfeed status: autoCompactWindow from settings");
    StatusLineFolder::new(&sdir).save(
        br#"{"session_id":"sess-1","context_window":{"context_window_size":1000000,"used_percentage":12,"current_usage":{"input_tokens":2000,"cache_creation_input_tokens":8000,"cache_read_input_tokens":110000,"output_tokens":500}}}"#,
        b"",
        "sess-1",
    );
    let mut o8 = JsonLines::new();
    status::run("claude", Some(&log), &settings, None, &mut o8, SystemTime::now());
    assert!(o8.lines[0].contains(r#""text":"ctx 120k / 250k""#) && o8.lines[0].contains(r#""percent":48"#) && o8.lines[0].contains(r#""warn":false"#), "modfeed status: ctx against the limit ({})", o8.lines[0]);
    StatusLineFolder::new(&sdir).save(br#"{"session_id":"sess-1","context_window":{"context_window_size":1000000,"used_percentage":21}}"#, b"", "sess-1");
    let mut o9 = JsonLines::new();
    status::run("claude", Some(&log), &settings, None, &mut o9, SystemTime::now());
    assert!(o9.lines[0].contains(r#""text":"ctx 210k / 250k""#) && o9.lines[0].contains(r#""warn":true"#), "modfeed status: near the limit warns ({})", o9.lines[0]);
    StatusLineFolder::new(&sdir).save(br#"{"session_id":"sess-1","context_window":{"used_percentage":21}}"#, b"", "sess-1");
    let mut o10 = JsonLines::new();
    status::run("claude", Some(&log), &settings, None, &mut o10, SystemTime::now());
    assert!(o10.lines[0].contains(r#""text":"ctx 21%""#), "modfeed status: tokens not known, percent as before ({})", o10.lines[0]);
    std::fs::write(&settings, "{}").unwrap();
    assert_eq!(status::compact_limit(&[settings.clone()], None), None, "modfeed status: no autoCompactWindow");
    assert_eq!(status::compact_limit(&[dir.path().join("missing.json")], None), None, "modfeed status: no settings file");
    let mut o7 = JsonLines::new();
    status::run("codex", Some(&log), &settings, None, &mut o7, SystemTime::now());
    assert!(o7.lines[0].contains(r#""source":"log""#) && o7.lines[0].contains("claude-test-1"), "modfeed status: model from the log otherwise");
}

#[test]
fn status_ctx_from_log() {
    let dir = tempfile::tempdir().unwrap();
    let settings = dir.path().join("settings.json");
    // Another tool's status line: no herdr-contr record, so the log is the source.
    std::fs::write(&settings, r#"{"statusLine":{"type":"command","command":"other-statusline"},"autoCompactWindow":250000}"#).unwrap();
    let log = dir.path().join("sess-2.jsonl");
    let usage = |id: &str, side: bool, n: i64| {
        serde_json::json!({"type": "assistant", "uuid": id, "isSidechain": side, "sessionId": "sess-2", "timestamp": "2026-11-12T10:00:00Z",
            "message": {"id": id, "role": "assistant", "model": "claude-test-1", "content": [{"type": "text", "text": "ok"}],
                "usage": {"input_tokens": 3, "cache_creation_input_tokens": 2000, "cache_read_input_tokens": n, "output_tokens": 50}}})
        .to_string()
    };
    let mut lines = turn();
    lines.push(usage("m1", false, 50_000));
    lines.push(usage("m2", false, 118_000));
    lines.push(usage("m3", true, 5_000));
    std::fs::write(&log, lines.join("\n") + "\n").unwrap();
    assert_eq!(tail::context_used(&lines), Some(120_003), "modfeed tail: newest main-thread usage");
    assert_eq!(tail::context_used(&turn()), None, "modfeed tail: no usage");
    let mut o = JsonLines::new();
    status::run("claude", Some(&log), &settings, None, &mut o, SystemTime::now());
    let st = &o.lines[0];
    assert!(st.contains(r#""source":"log""#) && st.contains(r#""text":"claude-test-1""#) && st.contains(r#""text":"ctx 120k / 250k""#) && st.contains(r#""percent":48"#), "modfeed status: ctx from the log ({st})");
    std::fs::write(&settings, "{}").unwrap();
    let mut o2 = JsonLines::new();
    status::run("claude", Some(&log), &settings, None, &mut o2, SystemTime::now());
    assert!(!o2.lines[0].contains("ctx"), "modfeed status: no limit, no window, no ctx tag ({})", o2.lines[0]);
    std::fs::write(&settings, r#"{"autoCompactWindow":250000}"#).unwrap();
    let mut o3 = JsonLines::new();
    status::run("codex", Some(&log), &settings, None, &mut o3, SystemTime::now());
    assert!(!o3.lines[0].contains("ctx"), "modfeed status: ctx from the log for claude only ({})", o3.lines[0]);
}

/// A Claude Code log whose newest main-thread reply used 120,003 tokens, from `model`.
fn usage_log(path: &std::path::Path, model: &str) {
    let mut lines = turn();
    lines.push(
        serde_json::json!({"type": "assistant", "uuid": "m1", "isSidechain": false, "sessionId": "sess-3", "timestamp": "2026-11-16T10:00:00Z",
            "message": {"id": "m1", "role": "assistant", "model": model, "content": [{"type": "text", "text": "ok"}],
                "usage": {"input_tokens": 3, "cache_creation_input_tokens": 2000, "cache_read_input_tokens": 118_000, "output_tokens": 50}}})
        .to_string(),
    );
    std::fs::write(path, lines.join("\n") + "\n").unwrap();
}

fn ctx_text(log: &std::path::Path, settings: &std::path::Path, cwd: Option<&std::path::Path>) -> String {
    let mut o = JsonLines::new();
    status::run("claude", Some(log), settings, cwd, &mut o, SystemTime::now());
    let v: serde_json::Value = serde_json::from_str(&o.lines[0]).unwrap();
    v["tags"].as_array().unwrap().iter().find(|t| t["kind"] == "context").map(|t| t["text"].as_str().unwrap().to_string()).unwrap_or_default()
}

#[test]
fn status_ctx_limit_per_model() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, log) = (dir.path().join("settings.json"), dir.path().join("sess-3.jsonl"));
    usage_log(&log, "claude-opus-5-5");
    let put = |v: serde_json::Value| std::fs::write(&settings, v.to_string()).unwrap();
    put(serde_json::json!({"modelSettings": {"claude-opus-5-5": {"effortLevel": "high", "autoCompactWindow": 300000}}}));
    assert_eq!(ctx_text(&log, &settings, None), "ctx 120k / 300k", "per-model value");
    put(serde_json::json!({"autoCompactWindow": 250000, "modelSettings": {"claude-sonnet-5-5": {"autoCompactWindow": 400000}}}));
    assert_eq!(ctx_text(&log, &settings, None), "ctx 120k / 250k", "top-level value; another model's value does not count");
    put(serde_json::json!({"autoCompactWindow": 250000, "modelSettings": {"claude-opus-5-5": {"autoCompactWindow": 300000}}}));
    assert_eq!(ctx_text(&log, &settings, None), "ctx 120k / 300k", "both: the per-model value wins");
    put(serde_json::json!({"modelSettings": {"claude-opus-5-5": {"effortLevel": "high"}}}));
    assert_eq!(ctx_text(&log, &settings, None), "", "none: no limit, window not known, no ctx tag");
    for bad in [serde_json::json!("300000"), serde_json::json!(0), serde_json::json!(-5), serde_json::json!(true), serde_json::json!({"n": 1})] {
        put(serde_json::json!({"autoCompactWindow": 250000, "modelSettings": {"claude-opus-5-5": {"autoCompactWindow": bad}}}));
        assert_eq!(ctx_text(&log, &settings, None), "ctx 120k / 250k", "a bad per-model value ({bad}) does not count");
        put(serde_json::json!({"autoCompactWindow": bad}));
        assert_eq!(ctx_text(&log, &settings, None), "", "a bad top-level value ({bad}) does not count");
    }
    std::fs::write(&settings, "{not json").unwrap();
    assert_eq!(ctx_text(&log, &settings, None), "", "a settings file that is not JSON");
}

#[test]
fn status_ctx_limit_model_forms() {
    let dir = tempfile::tempdir().unwrap();
    let settings = dir.path().join("settings.json");
    let files = [settings.clone()];
    let put = |v: serde_json::Value| std::fs::write(&settings, v.to_string()).unwrap();
    put(serde_json::json!({"modelSettings": {"claude-opus-5-5": {"autoCompactWindow": 300000}}}));
    for m in ["claude-opus-5-5", "claude-opus-5-5-20261101", "claude-opus-5-5[1m]", "Claude-Opus-5-5"] {
        assert_eq!(status::compact_limit(&files, Some(m)), Some(300000), "log model {m} matches the plain key");
    }
    assert_eq!(status::compact_limit(&files, Some("claude-opus-5-6")), None, "another model does not match");
    put(serde_json::json!({"modelSettings": {"claude-opus-5-5-20261101": {"autoCompactWindow": 310000}, "claude-sonnet-5-5[1m]": {"autoCompactWindow": 700000}}}));
    assert_eq!(status::compact_limit(&files, Some("claude-opus-5-5")), Some(310000), "a dated key matches the plain id");
    assert_eq!(status::compact_limit(&files, Some("claude-sonnet-5-5")), Some(700000), "a [1m] key matches the plain id");
    put(serde_json::json!({"modelSettings": {"claude-opus-5-5": {"autoCompactWindow": 300000}, "claude-opus-5-5[1m]": {"autoCompactWindow": 800000}}}));
    assert_eq!(status::compact_limit(&files, Some("claude-opus-5-5[1m]")), Some(800000), "the exact key first");
    assert_eq!(status::compact_limit(&files, Some("claude-opus-5-5")), Some(300000), "the exact key first (plain)");
    assert_eq!(status::base_model("claude-opus-5-5-20261101[1m]"), "claude-opus-5-5");
    assert_eq!(status::base_model("claude-opus-5-5-2026"), "claude-opus-5-5-2026", "not a date");
}

#[test]
fn status_ctx_limit_project_files() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, log, cwd) = (dir.path().join("settings.json"), dir.path().join("sess-3.jsonl"), dir.path().join("proj"));
    usage_log(&log, "claude-opus-5-5");
    std::fs::create_dir_all(cwd.join(".claude")).unwrap();
    std::fs::write(&settings, r#"{"autoCompactWindow":250000}"#).unwrap();
    assert_eq!(ctx_text(&log, &settings, Some(&cwd)), "ctx 120k / 250k", "user file when the project has none");
    std::fs::write(cwd.join(".claude/settings.json"), r#"{"autoCompactWindow":200000}"#).unwrap();
    assert_eq!(ctx_text(&log, &settings, Some(&cwd)), "ctx 120k / 200k", "project file before the user file");
    assert_eq!(ctx_text(&log, &settings, None), "ctx 120k / 250k", "cwd not known: user file only");
    std::fs::write(cwd.join(".claude/settings.local.json"), r#"{"autoCompactWindow":150000}"#).unwrap();
    assert_eq!(ctx_text(&log, &settings, Some(&cwd)), "ctx 120k / 150k", "local file first");
    std::fs::write(&settings, r#"{"modelSettings":{"claude-opus-5-5":{"autoCompactWindow":300000}}}"#).unwrap();
    assert_eq!(ctx_text(&log, &settings, Some(&cwd)), "ctx 120k / 300k", "a per-model value in any file wins over a top-level value");
    assert_eq!(
        status::settings_files(&settings, Some(&cwd)),
        vec![cwd.join(".claude/settings.local.json"), cwd.join(".claude/settings.json"), settings.clone()],
        "Claude Code's order"
    );
}

#[test]
fn status_ctx_window_without_limit() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, log) = (dir.path().join("settings.json"), dir.path().join("sess-3.jsonl"));
    std::fs::write(&settings, "{}").unwrap();
    usage_log(&log, "claude-opus-5-5");
    // A `/model` switch to the 1M window after the reply: the log's model is claude-opus-5-5[1m].
    let switch = serde_json::json!({"type": "user", "uuid": "u9", "sessionId": "sess-3", "timestamp": "2026-11-16T10:01:00Z",
        "message": {"role": "user", "content": "<local-command-stdout>Set model to Opus 5.5 (1M context)</local-command-stdout>"}});
    std::fs::write(&log, std::fs::read_to_string(&log).unwrap() + &switch.to_string() + "\n").unwrap();
    assert_eq!(ctx_text(&log, &settings, None), "ctx 12%", "no limit: percent of the 1M window");
    std::fs::write(&settings, r#"{"autoCompactWindow":250000}"#).unwrap();
    assert_eq!(ctx_text(&log, &settings, None), "ctx 120k / 250k", "a limit wins over the window");
    assert_eq!(status::context_window("claude-opus-5-5[1m]"), Some(1_000_000));
    assert_eq!(status::context_window("claude-opus-5-5"), None, "window not known");
}

#[test]
fn status_statusline_limit_per_model() {
    let dir = tempfile::tempdir().unwrap();
    let (sdir, settings, log) = (dir.path().join("sl"), dir.path().join("settings.json"), dir.path().join("sess-3.jsonl"));
    usage_log(&log, "claude-opus-5-5");
    let cmd = format!("herdr-contr-statusline --dir '{}'", sdir.display());
    std::fs::write(&settings, serde_json::json!({"statusLine": {"type": "command", "command": cmd}, "autoCompactWindow": 250000, "modelSettings": {"claude-opus-5-5": {"autoCompactWindow": 300000}}}).to_string()).unwrap();
    StatusLineFolder::new(&sdir).save(br#"{"session_id":"sess-3","model":{"display_name":"Opus 5.5"},"context_window":{"context_window_size":1000000,"used_percentage":12}}"#, b"", "sess-3");
    assert_eq!(ctx_text(&log, &settings, None), "ctx 120k / 300k", "status line source: the log's model picks the per-model value");
}

#[test]
fn cache_times_read_back_exactly() {
    // A value CI read back one ulp off (as ...617) before serde_json's float_roundtrip.
    let dir = tempfile::tempdir().unwrap();
    let mut cache = locate::Cache::default();
    cache.entries.insert("t7".into(), locate::Entry { kind: "claude".into(), cwd: "/w".into(), pids: vec![1], path: None, checked: 1793722804.7949615 });
    let cf = dir.path().join("cache.json");
    locate::write_cache(&cache, Some(&cf));
    assert_eq!(locate::read_cache(Some(&cf)), cache);
}

#[test]
fn pictures_only_when_asked() {
    // A pasted PNG: the mod's rows count it; rows read with line offsets (serve) also say where it is.
    let png = "iVBORw0KGgoAAAANSUhEUgAAAAMAAAAECAYAAABLLYUGAAAA";
    let pre = r#"{"type":"file-history-snapshot"}"#;
    let line = format!(r#"{{"type":"user","uuid":"u1","timestamp":"2026-11-03T01:40:00.000Z","message":{{"role":"user","content":[{{"type":"text","text":"see"}},{{"type":"image","source":{{"type":"base64","media_type":"image/png","data":"{png}"}}}}]}}}}"#);
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("s.jsonl");
    std::fs::write(&f, format!("{pre}\n\n{line}\n")).unwrap();
    let range = tail::read(&f, None, tail::BYTES).unwrap();
    let plain = rows::rows(&tail::items(&range.lines), &now(), &Utc);
    assert_eq!((plain[0].images, plain[0].pics.as_ref()), (Some(1), None));
    let at = rows::rows(&tail::items_at(&range), &now(), &Utc);
    let p = &at[0].pics.as_ref().unwrap()[0];
    assert_eq!((p.format, p.width, p.height, p.offset, p.index), ("png", 3, 4, pre.len() as u64 + 2, 0), "the line after an empty one");
    assert_eq!(p.digest.len(), 16);
}
