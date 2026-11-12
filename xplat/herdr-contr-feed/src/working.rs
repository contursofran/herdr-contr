//! The agent's working line, read from its screen: Claude Code's "✶ Improvising… (1m 16s · ↓ 6.0k tokens · thought
//! for 1s)". The session log does not have it. Read only: nothing goes to the agent.

use serde_json::{json, Value};

/// The glyphs Claude Code turns in front of its working word.
const GLYPHS: [char; 8] = ['·', '✢', '✳', '✶', '✻', '✽', '*', '∗'];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Working {
    /// "Improvising…"
    pub text: String,
    /// "1m 16s · ↓ 6.0k tokens · thought for 1s", without key hints ("esc to interrupt"). Can be empty.
    pub detail: String,
}

/// The `working` line; none on screen: one with no text.
pub fn line(w: Option<&Working>) -> Value {
    match w {
        Some(w) => json!({"type": "working", "text": w.text, "detail": w.detail}),
        None => json!({"type": "working", "text": null, "detail": null}),
    }
}

/// The agents whose working line herdr-contr reads.
pub fn reads(kind: &str) -> bool {
    kind == "claude"
}

/// Claude Code's working line: the lowest screen line that starts with a spinner glyph and has a word that ends in
/// "…". A line in the past tense ("✻ Worked for 1m 2s") is not one.
pub fn claude(screen: &str) -> Option<Working> {
    screen.lines().rev().find_map(parse)
}

fn parse(line: &str) -> Option<Working> {
    let s = line.trim();
    let mut chars = s.chars();
    let g = chars.next()?;
    if !GLYPHS.contains(&g) {
        return None;
    }
    let rest = chars.as_str();
    if !rest.starts_with(' ') {
        return None;
    }
    let rest = rest.trim_start();
    let end = rest.find('…')? + '…'.len_utf8();
    let text = &rest[..end];
    // One or a few words before "…", no sentence.
    if text.is_empty() || text.chars().count() > 40 || text.starts_with('…') {
        return None;
    }
    let after = rest[end..].trim();
    let inner = match after.strip_prefix('(').and_then(|a| a.strip_suffix(')')) {
        Some(i) => i,
        None if after.is_empty() => "",
        None => return None,
    };
    let detail = inner
        .split(" · ")
        .map(str::trim)
        .filter(|p| !p.is_empty() && !p.contains("esc to") && !p.contains("ctrl+"))
        .collect::<Vec<_>>()
        .join(" · ");
    // A bare "* word…" may be text the agent wrote: only the spinner glyphs count without the parentheses.
    if detail.is_empty() && g == '*' {
        return None;
    }
    Some(Working { text: text.to_string(), detail })
}
