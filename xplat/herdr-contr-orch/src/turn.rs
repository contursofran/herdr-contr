//! What a Claude Code transcript (JSON Lines) says about a session: its last turn, its context size and
//! model, and if it ran a slash command. The format belongs to Claude Code, so each function reads only the
//! fields that it needs, and skips a line that is not JSON. Ported from contr's `internal/agents`.
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use serde_json::Value;

/// The last turn of a transcript: the part after the last prompt.
#[derive(Debug, Default, PartialEq)]
pub struct Turn {
    /// The turn sent a message to another terminal: a Bash call with `hc send`, or the kit's report or message
    /// tool, whose result is not an error. A call that auto mode blocked or that hc refused is no report.
    pub reported: bool,
    /// The last text of the assistant in the turn.
    pub said: String,
}

/// Calls `f` with each line of `r` that is JSON.
fn each_entry(r: impl Read, mut f: impl FnMut(Value)) -> io::Result<()> {
    let mut r = BufReader::new(r);
    let mut line = Vec::new();
    loop {
        line.clear();
        if r.read_until(b'\n', &mut line)? == 0 {
            return Ok(());
        }
        if let Ok(v) = serde_json::from_slice::<Value>(&line) {
            f(v);
        }
    }
}

/// User content is a prompt (typed or sent text) when it is a string, or blocks with no tool result.
fn is_prompt(content: &Value) -> bool {
    match content {
        Value::String(_) => true,
        Value::Array(blocks) => !blocks.is_empty() && blocks.iter().all(|b| b["type"] != "tool_result"),
        _ => false,
    }
}

/// A tool call that sends a message to another terminal.
fn reports(block: &Value) -> bool {
    let name = block["name"].as_str().unwrap_or_default();
    if name == "Bash" {
        return block["input"]["command"].as_str().unwrap_or_default().contains("hc send");
    }
    name.ends_with("contr__report") || name.ends_with("contr__message")
}

/// The last turn of the transcript in `r`.
pub fn last_turn(r: impl Read) -> io::Result<Turn> {
    let mut t = Turn::default();
    // The ids of the report calls of the turn that have no result yet.
    let mut calls: Vec<String> = Vec::new();
    each_entry(r, |e| {
        let content = &e["message"]["content"];
        match e["type"].as_str() {
            Some("user") if e["isMeta"] != true && is_prompt(content) => {
                t = Turn::default();
                calls.clear();
            }
            Some("user") => {
                for b in content.as_array().into_iter().flatten() {
                    let id = b["tool_use_id"].as_str().unwrap_or_default();
                    if b["type"] == "tool_result" && b["is_error"] != true && calls.iter().any(|c| c == id) {
                        t.reported = true;
                    }
                }
            }
            Some("assistant") => {
                for b in content.as_array().into_iter().flatten() {
                    match b["type"].as_str() {
                        Some("text") => {
                            let s = b["text"].as_str().unwrap_or_default().trim();
                            if !s.is_empty() {
                                t.said = s.to_string();
                            }
                        }
                        Some("tool_use") if reports(b) => calls.extend(b["id"].as_str().map(str::to_string)),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    })?;
    Ok(t)
}

/// How much of the end of a transcript `context_and_model` reads.
pub const TAIL_BYTES: u64 = 1 << 20;

/// The context size and the model of the session at `path`, from the end of its transcript: the input tokens
/// (cache included) of the last assistant message with usage, and the model id of the last assistant message,
/// or "". Claude Code writes a model in angle brackets for a message it made itself; that is no model.
pub fn context_and_model(path: &Path) -> io::Result<(i64, String)> {
    let mut f = File::open(path)?;
    let size = f.metadata()?.len();
    if size > TAIL_BYTES {
        // The first line after the seek is cut. It is not JSON, so it is skipped.
        f.seek(SeekFrom::Start(size - TAIL_BYTES))?;
    }
    let (mut tokens, mut model) = (0, String::new());
    each_entry(f, |e| {
        if e["type"] != "assistant" {
            return;
        }
        let m = e["message"]["model"].as_str().unwrap_or_default();
        if !m.is_empty() && !m.starts_with('<') {
            model = m.to_string();
        }
        let u = &e["message"]["usage"];
        if u.is_object() {
            let n = |k: &str| u[k].as_i64().unwrap_or(0);
            tokens = n("input_tokens") + n("cache_read_input_tokens") + n("cache_creation_input_tokens");
        }
    })?;
    Ok((tokens, model))
}

/// The transcript at `path` holds a run of the slash command `name` (such as "contr:orchestrate"). Claude
/// Code records it as a user message whose content is a string with the command-name tag. The tag in a tool
/// result or in the assistant's text does not count.
pub fn ran_command(path: &Path, name: &str) -> io::Result<bool> {
    let mark = format!("<command-name>/{name}</command-name>");
    let mut ran = false;
    each_entry(File::open(path)?, |e| {
        if e["type"] == "user" && e["message"]["content"].as_str().is_some_and(|s| s.contains(&mark)) {
            ran = true;
        }
    })?;
    Ok(ran)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // Transcript lines in the shape that Claude Code writes.
    const PROMPT: &str = r#"{"type":"user","message":{"role":"user","content":"do the task"}}"#;
    const PROMPT_LIST: &str = r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"<hc from=\"t1 lead\">fix it</hc>"}]}}"#;
    const META_PROMPT: &str = r#"{"type":"user","isMeta":true,"message":{"role":"user","content":"caveat"}}"#;
    const RESULT: &str = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"x","content":"ok"}]}}"#;
    const SEND_CALL: &str = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"s1","name":"Bash","input":{"command":"hc send \"$CONTR_COORDINATOR\" \"b1: done\""}}]}}"#;
    const SEND_OK: &str = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"s1","content":"sent"}]}}"#;
    // hc refused the send (exit 1), or auto mode blocked the command: Claude Code marks the result as an error.
    const SEND_REFUSED: &str = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"s1","is_error":true,"content":"Exit code 1\nhc: t1 did not take the text: blocked"}]}}"#;
    const SEND_BLOCKED: &str = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"s1","is_error":true,"content":"Permission for this action has been denied. Reason: Unsanctioned Communication"}]}}"#;
    const OTHER_CALL: &str = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"o1","name":"Bash","input":{"command":"cargo test"}}]}}"#;
    const OTHER_OK: &str = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"o1","content":"ok"}]}}"#;
    const MCP_CALL: &str = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"m1","name":"mcp__plugin_contr_contr__message","input":{"name":"t1"}}]}}"#;
    const MCP_OK: &str = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"m1","content":[{"type":"text","text":"sent"}]}]}}"#;
    const REPORT_CALL: &str = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"r1","name":"mcp__plugin_contr_contr__report","input":{"text":"b1: done"}}]}}"#;
    const REPORT_HELD: &str = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"r1","content":[{"type":"text","text":"held: t1 did not take it now (blocked)"}]}]}}"#;
    const WORDS: &str = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"  PR #5 is open.  "}]}}"#;
    const SUMMARY: &str = r#"{"type":"summary","summary":"x"}"#;

    fn turn(reported: bool, said: &str) -> Turn {
        Turn { reported, said: said.into() }
    }

    #[test]
    fn last_turn_is_the_part_after_the_last_prompt() {
        let cases: [(&str, &[&str], Turn); 11] = [
            ("report in the turn", &[PROMPT, OTHER_CALL, OTHER_OK, SEND_CALL, SEND_OK, WORDS], turn(true, "PR #5 is open.")),
            ("no report", &[PROMPT, OTHER_CALL, OTHER_OK, WORDS], turn(false, "PR #5 is open.")),
            ("report only in an earlier turn", &[PROMPT, SEND_CALL, SEND_OK, WORDS, PROMPT_LIST, OTHER_CALL, OTHER_OK], turn(false, "")),
            ("tool results and meta lines keep the turn", &[PROMPT, SEND_CALL, SEND_OK, META_PROMPT, SUMMARY, WORDS], turn(true, "PR #5 is open.")),
            ("the kit's message tool counts", &[PROMPT_LIST, MCP_CALL, MCP_OK], turn(true, "")),
            ("the kit's report tool counts, also when it held the report", &[PROMPT, REPORT_CALL, REPORT_HELD], turn(true, "")),
            ("a send that hc refused is no report", &[PROMPT, SEND_CALL, SEND_REFUSED, WORDS], turn(false, "PR #5 is open.")),
            ("a send that auto mode blocked is no report", &[PROMPT, SEND_CALL, SEND_BLOCKED], turn(false, "")),
            ("a call with no result is no report", &[PROMPT, SEND_CALL, RESULT], turn(false, "")),
            ("bad lines are skipped", &[PROMPT, "not json", SEND_CALL, SEND_OK], turn(true, "")),
            ("empty", &[], turn(false, "")),
        ];
        for (name, lines, want) in cases {
            assert_eq!(last_turn(lines.join("\n").as_bytes()).unwrap(), want, "{name}");
        }
    }

    fn transcript(dir: &Path, lines: &[&str]) -> std::path::PathBuf {
        let p = dir.join("t.jsonl");
        fs::write(&p, lines.join("\n") + "\n").unwrap();
        p
    }

    fn usage(input: i64, read: i64, create: i64) -> String {
        format!(r#"{{"type":"assistant","message":{{"content":[],"usage":{{"input_tokens":{input},"cache_read_input_tokens":{read},"cache_creation_input_tokens":{create},"output_tokens":9}}}}}}"#)
    }

    #[test]
    fn the_context_is_the_input_of_the_last_reply() {
        let d = tempfile::tempdir().unwrap();
        let p = transcript(d.path(), &[PROMPT, &usage(10, 100, 5), PROMPT, &usage(3, 200000, 30000), r#"{"type":"user","message":{"content":"x"}}"#]);
        assert_eq!(context_and_model(&p).unwrap(), (230003, String::new()));
        assert_eq!(context_and_model(&transcript(d.path(), &[PROMPT])).unwrap().0, 0);
        // A long file: only the tail is read, and the cut first line is skipped.
        let filler = format!(r#"{{"type":"user","message":{{"content":"{}"}}}}"#, "x".repeat(1000));
        let big = format!("{}\n{}{}\n", usage(1, 1, 1), format!("{filler}\n").repeat(TAIL_BYTES as usize / 1000 + 10), usage(7, 0, 0));
        fs::write(d.path().join("big.jsonl"), big).unwrap();
        assert_eq!(context_and_model(&d.path().join("big.jsonl")).unwrap().0, 7);
        assert_eq!(context_and_model(&d.path().join("none")).unwrap_err().kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn the_model_is_the_last_real_one() {
        let d = tempfile::tempdir().unwrap();
        let opus = r#"{"type":"assistant","message":{"model":"claude-opus-5-5","content":[],"usage":{"input_tokens":4}}}"#;
        let synthetic = r#"{"type":"assistant","message":{"model":"<synthetic>","content":[]}}"#;
        let fable = r#"{"type":"assistant","message":{"model":"claude-fable-5-2","content":[]}}"#;
        assert_eq!(context_and_model(&transcript(d.path(), &[PROMPT, opus, synthetic])).unwrap(), (4, "claude-opus-5-5".to_string()));
        // After a model change the next assistant message has the new model.
        assert_eq!(context_and_model(&transcript(d.path(), &[PROMPT, opus, PROMPT, fable])).unwrap().1, "claude-fable-5-2");
    }

    #[test]
    fn a_command_ran_only_when_a_user_message_names_it() {
        let d = tempfile::tempdir().unwrap();
        let cmd = r#"{"type":"user","message":{"content":"<command-message>contr:orchestrate</command-message>\n<command-name>/contr:orchestrate</command-name>"}}"#;
        assert!(ran_command(&transcript(d.path(), &[PROMPT, cmd, WORDS]), "contr:orchestrate").unwrap());
        assert!(!ran_command(&transcript(d.path(), &[PROMPT, WORDS]), "contr:orchestrate").unwrap());
        // The tag in a file that the session read, or in what the assistant wrote, is no run of the command.
        let read = r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"<command-name>/contr:orchestrate</command-name>"}]}}"#;
        let said = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"<command-name>/contr:orchestrate</command-name>"}]}}"#;
        assert!(!ran_command(&transcript(d.path(), &[PROMPT, read, said, WORDS]), "contr:orchestrate").unwrap());
        assert!(ran_command(&d.path().join("none"), "x").is_err());
    }
}
