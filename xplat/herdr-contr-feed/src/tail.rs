//! Swift: `Tail.swift`. Reads the end of a session log and parses it.

use herdr_core::ConversationItem;
use herdr_core::ConversationParser;
use serde_json::Value;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// The app shows the last 2 MB first; the mod does the same.
pub const BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Range {
    pub lines: Vec<String>,
    /// Each line's file offset.
    pub offsets: Vec<u64>,
    /// File offset of the first whole line read.
    pub start: u64,
    /// File size when read. Bytes after the last newline (a line still being written) are left for the next read.
    pub size: u64,
}

fn read_span(path: &Path, from: u64, to: u64) -> std::io::Result<Vec<u8>> {
    let mut f = std::fs::File::open(path)?;
    f.seek(SeekFrom::Start(from))?;
    let mut data = Vec::with_capacity((to - from) as usize);
    f.take(to - from).read_to_end(&mut data)?;
    Ok(data)
}

fn size_of(path: &Path) -> std::io::Result<u64> {
    let mut f = std::fs::File::open(path)?;
    f.seek(SeekFrom::End(0))
}

/// The whole lines from `from` (a line start), or else from the last `bytes` of the file, where a cut first line is
/// dropped. A last line with no newline yet is left out.
pub fn read(path: &Path, from: Option<u64>, bytes: u64) -> std::io::Result<Range> {
    let size = size_of(path)?;
    let mut start = match from {
        Some(f) => f.min(size),
        None => size.saturating_sub(bytes),
    };
    let data = read_span(path, start, size)?;
    let mut body: &[u8] = &data;
    if from.is_none() && start > 0 {
        let Some(nl) = body.iter().position(|&b| b == b'\n') else {
            return Ok(Range { lines: vec![], offsets: vec![], start: size, size });
        };
        start += nl as u64 + 1;
        body = &body[nl + 1..];
    }
    let mut parts: Vec<&[u8]> = body.split(|&b| b == b'\n').collect();
    parts.pop(); // after the last "\n": empty, or a line still being written
    let (lines, offsets) = whole(parts, start);
    Ok(Range { lines, offsets, start, size })
}

/// The non-empty lines of `parts` (split at newlines, the first at `start`), and their file offsets.
fn whole(parts: Vec<&[u8]>, start: u64) -> (Vec<String>, Vec<u64>) {
    let (mut lines, mut offsets, mut at) = (vec![], vec![], start);
    for p in parts {
        if !p.is_empty() {
            lines.push(String::from_utf8_lossy(p).into_owned());
            offsets.push(at);
        }
        at += p.len() as u64 + 1;
    }
    (lines, offsets)
}

/// The whole lines that end before `before` (a line start): up to `bytes` back, a cut first line dropped.
/// `start` 0 means the file's start was reached.
pub fn read_before(path: &Path, before: u64, bytes: u64) -> std::io::Result<Range> {
    read_before_max(path, before, bytes, MAX_LINE)
}

/// A line longer than this before the range (a screenshot) is skipped, not read: rows do not show it whole.
pub const MAX_LINE: u64 = 16 * 1024 * 1024;

/// `read_before` with the longest line it reads whole: a longer one gives no lines and `start` at its start.
pub fn read_before_max(path: &Path, before: u64, bytes: u64, max_line: u64) -> std::io::Result<Range> {
    let size = size_of(path)?;
    let end = before.min(size);
    let mut start = end.saturating_sub(bytes);
    let data = read_span(path, start, end)?;
    let mut body: &[u8] = &data;
    if start > 0 {
        // The last byte is the newline that ends the line before `before`: a newline that starts a whole line
        // must come before it.
        let Some(nl) = body[..body.len().saturating_sub(1)].iter().position(|&b| b == b'\n') else {
            // One line longer than `bytes` (a tool result with a screenshot): the range still moves back, to its
            // start (staying at `before` would stop older rows there for good). Read whole up to `max_line`.
            let from = line_start(path, start)?;
            if end - from > max_line {
                return Ok(Range { lines: vec![], offsets: vec![], start: from, size });
            }
            let data = read_span(path, from, end)?;
            let (lines, offsets) = whole(data.split(|&b| b == b'\n').collect(), from);
            return Ok(Range { lines, offsets, start: from, size });
        };
        start += nl as u64 + 1;
        body = &body[nl + 1..];
    }
    let (lines, offsets) = whole(body.split(|&b| b == b'\n').collect(), start);
    Ok(Range { lines, offsets, start, size })
}

/// The start of the line that holds byte `at`: just after the newline before it, or 0.
fn line_start(path: &Path, at: u64) -> std::io::Result<u64> {
    const STEP: u64 = 64 * 1024;
    let mut end = at;
    while end > 0 {
        let from = end.saturating_sub(STEP);
        let data = read_span(path, from, end)?;
        if let Some(nl) = data.iter().rposition(|&b| b == b'\n') {
            return Ok(from + nl as u64 + 1);
        }
        end = from;
    }
    Ok(0)
}

pub fn items(lines: &[String]) -> Vec<ConversationItem> {
    let mut p = ConversationParser::new();
    for l in lines {
        p.feed(l);
    }
    p.items()
}

/// `items`, with each line's file offset (`start`: the first line's): picture refs point into the log. Codex, Grok and
/// Cursor items then get ids from their offsets, so rows differ from `items`' (only `serve` reads this way).
pub fn items_at(range: &Range) -> Vec<ConversationItem> {
    let mut p = ConversationParser::new();
    for (l, at) in range.lines.iter().zip(&range.offsets) {
        p.feed_at(l, Some(*at));
    }
    p.items()
}

/// The tokens in the context after the newest main-thread Claude assistant message: its usage's input, cache write and
/// cache read tokens. None without one.
pub fn context_used(lines: &[String]) -> Option<i64> {
    for l in lines.iter().rev().filter(|l| l.contains("\"usage\"")) {
        let Ok(o) = serde_json::from_str::<Value>(l) else { continue };
        if o.get("type").and_then(Value::as_str) != Some("assistant") || o.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let Some(u) = o.get("message").and_then(|m| m.get("usage")) else { continue };
        let n: i64 = ["input_tokens", "cache_creation_input_tokens", "cache_read_input_tokens"]
            .iter()
            .filter_map(|k| u.get(*k).and_then(Value::as_i64))
            .sum();
        if n > 0 {
            return Some(n);
        }
    }
    None
}

/// The model of the newest line that names one, for logs with no status line: an assistant line's
/// `message.model`, a Codex `payload.model`, or a `/model` switch (`Set model to …` local command output, the label
/// mapped back to its id; `Default` names no model and is skipped).
pub fn model(lines: &[String]) -> Option<String> {
    for l in lines.iter().rev().filter(|l| l.contains("\"model\"") || l.contains("Set model to")) {
        let Ok(o) = serde_json::from_str::<Value>(l) else { continue };
        if !o.is_object() {
            continue;
        }
        if let Some((m, _)) = model_switch(&o) {
            if m.is_some() {
                return m;
            }
            continue;
        }
        if let Some(m) = o.get("message").and_then(|m| m.get("model")).and_then(Value::as_str) {
            if !m.is_empty() && !m.starts_with('<') {
                return Some(m.into());
            }
        }
        if let Some(m) = o.get("payload").and_then(|m| m.get("model")).and_then(Value::as_str) {
            if !m.is_empty() {
                return Some(m.into());
            }
        }
    }
    None
}

/// The effort that the newest `/model` switch names (`… with high effort`), when no assistant line came after it.
pub fn switch_effort(lines: &[String]) -> Option<String> {
    for l in lines.iter().rev().filter(|l| l.contains("\"model\"") || l.contains("Set model to")) {
        let Ok(o) = serde_json::from_str::<Value>(l) else { continue };
        if let Some((_, e)) = model_switch(&o) {
            return e;
        }
        if o.get("type").and_then(Value::as_str) == Some("assistant") && o.get("isSidechain").and_then(Value::as_bool) != Some(true) {
            return None;
        }
    }
    None
}

/// A `/model` switch row: `Some((model id, effort))`; None for any other line.
fn model_switch(o: &Value) -> Option<(Option<String>, Option<String>)> {
    let c = o.get("message")?.get("content")?.as_str()?;
    let body = c.trim().strip_prefix("<local-command-stdout>")?.strip_suffix("</local-command-stdout>")?;
    let text = strip_ansi(body).replace('`', "");
    let rest = text.trim().strip_prefix("Set model to ")?;
    let (label, effort) = match rest.rsplit_once(" with ") {
        Some((l, e)) if e.trim().ends_with(" effort") => (l.trim(), Some(e.trim().trim_end_matches(" effort").trim().to_string())),
        _ => (rest.trim(), None),
    };
    let label = label.trim_end_matches("(default)").trim();
    Some((label_id(label), effort.filter(|e| !e.is_empty())))
}

/// `Opus 5.5` → `claude-opus-5-5`, `Sonnet 5.5 (1M context)` → `claude-sonnet-5-5[1m]`: `models::claude_label` in
/// reverse. An id stays as it is. None for `Default` or text that is no model.
fn label_id(label: &str) -> Option<String> {
    if label.starts_with("claude-") && !label.contains(' ') {
        return Some(label.into());
    }
    let (base, wide) = label.strip_suffix("(1M context)").map(|b| (b.trim(), true)).unwrap_or((label, false));
    let (family, version) = base.split_once(' ')?;
    if family.is_empty() || !family.chars().all(|c| c.is_ascii_alphabetic()) || family.eq_ignore_ascii_case("default") {
        return None;
    }
    let parts: Vec<&str> = version.split('.').collect();
    if parts.iter().any(|v| v.is_empty() || v.len() > 2 || !v.chars().all(|c| c.is_ascii_digit())) {
        return None;
    }
    let id = format!("claude-{}-{}{}", family.to_lowercase(), parts.join("-"), if wide { "[1m]" } else { "" });
    (crate::models::claude_label(&id) == label).then_some(id)
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\u{1b}' {
            if it.peek() == Some(&'[') {
                it.next();
                for d in it.by_ref() {
                    if d.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod model_tests {
    use super::*;

    fn fixture() -> Vec<String> {
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../fixtures/model-switch.jsonl")).lines().map(String::from).collect()
    }

    #[test]
    fn a_switch_after_the_last_reply_gives_the_model_and_effort() {
        let l = fixture();
        assert_eq!(model(&l).as_deref(), Some("claude-opus-5-5"));
        assert_eq!(switch_effort(&l).as_deref(), Some("high"));
    }

    #[test]
    fn a_reply_after_the_switch_wins() {
        let mut l = fixture();
        l.push(r#"{"type":"assistant","uuid":"a2","message":{"role":"assistant","model":"claude-fable-5-1","content":[]}}"#.into());
        assert_eq!(model(&l).as_deref(), Some("claude-fable-5-1"));
        assert_eq!(switch_effort(&l), None);
    }

    #[test]
    fn switch_text_shapes() {
        let row = |t: &str| format!(r#"{{"type":"user","message":{{"role":"user","content":"<local-command-stdout>{t}</local-command-stdout>"}}}}"#);
        let base = r#"{"type":"assistant","message":{"role":"assistant","model":"claude-sonnet-5-5","content":[]}}"#.to_string();
        for (t, want, eff) in [
            ("Set model to `Opus 5.5`", Some("claude-opus-5-5"), None),
            ("Set model to Sonnet 5.5 (1M context)", Some("claude-sonnet-5-5[1m]"), None),
            ("Set model to Opus 5.5 (default) with low effort", Some("claude-opus-5-5"), Some("low")),
            ("Set model to claude-fable-5-1", Some("claude-fable-5-1"), None),
            ("Set model to Default (recommended)", Some("claude-sonnet-5-5"), None),
            ("Set effort to high", Some("claude-sonnet-5-5"), None),
        ] {
            let l = vec![base.clone(), row(t)];
            assert_eq!(model(&l).as_deref(), want, "{t}");
            assert_eq!(switch_effort(&l).as_deref(), eff, "{t}");
        }
    }
}
