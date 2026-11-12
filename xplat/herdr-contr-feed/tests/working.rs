//! The working line read from a Claude Code screen (`working::claude`). The screens are shaped like Claude Code 2.1
//! draws them; the text is made up.

use modfeedkit::working::{self, Working};

fn w(text: &str, detail: &str) -> Option<Working> {
    Some(Working { text: text.into(), detail: detail.into() })
}

fn screen(spinner: &str) -> String {
    let rule = "─".repeat(60);
    ["⏺ I will look at the tests.", "", spinner, "", &rule, "❯ ", &rule, "  ⏵⏵ auto mode on (shift+tab to cycle)"].join("\n")
}

#[test]
fn reads_the_word_and_the_detail() {
    assert_eq!(working::claude(&screen("✶ Improvising… (1m 16s · ↓ 6.0k tokens · thought for 1s)")), w("Improvising…", "1m 16s · ↓ 6.0k tokens · thought for 1s"));
    assert_eq!(working::claude(&screen("· Pouncing… (10s · ↓ 864 tokens)")), w("Pouncing…", "10s · ↓ 864 tokens"));
}

#[test]
fn drops_key_hints_and_keeps_a_bare_word() {
    assert_eq!(working::claude(&screen("✻ Thinking… (3s · esc to interrupt)")), w("Thinking…", "3s"));
    assert_eq!(working::claude(&screen("✢ Brewing…")), w("Brewing…", ""));
    assert_eq!(working::claude(&screen("  ✳ Hatching… (2s · ↑ 12 tokens · ctrl+t to show todos)")), w("Hatching…", "2s · ↑ 12 tokens"));
}

#[test]
fn not_a_working_line() {
    // The past tense, text the agent wrote, no spinner.
    assert_eq!(working::claude(&screen("✻ Worked for 1m 2s")), None);
    assert_eq!(working::claude(&screen("* Notes… to read later")), None);
    assert_eq!(working::claude(&screen("* Loading…")), None);
    assert_eq!(working::claude(&screen("")), None);
    assert_eq!(working::claude("⏺ Done… (that was quick)\n"), None);
}

#[test]
fn the_line_for_the_mod() {
    let l = working::line(w("Improvising…", "1m 16s").as_ref());
    assert_eq!(l["type"], "working"); assert_eq!(l["text"], "Improvising…"); assert_eq!(l["detail"], "1m 16s");
    assert!(working::line(None)["text"].is_null());
}

/// A Claude Code screen with a note under its prompt box (the footer's " · " parts).
fn footer(note: &str) -> String {
    let rule = "─".repeat(60);
    ["⏺ The server runs in the background.", "", &rule, "❯ ", &rule, &format!("  ⏵⏵ auto mode on (shift+tab to cycle) · {note}")].join("\n")
}

#[test]
fn reads_the_background_note_under_the_box() {
    assert_eq!(working::background(&footer("1 shell still running")).as_deref(), Some("1 shell still running"));
    assert_eq!(working::background(&footer("2 shells still running")).as_deref(), Some("2 shells still running"));
    assert_eq!(working::background(&footer("1 background task")).as_deref(), Some("1 background task"));
    assert_eq!(working::background(&footer("3 background tasks · ? for shortcuts")).as_deref(), Some("3 background tasks"));
}

#[test]
fn no_background_note_without_one() {
    assert_eq!(working::background(&screen("✶ Improvising… (3s)")), None);
    // The same words above the box are the agent's text, not the note.
    let rule = "─".repeat(60);
    let text = ["⏺ Now 1 shell still running", "", &rule, "❯ ", &rule, "  ? for shortcuts"].join("\n");
    assert_eq!(working::background(&text), None);
    assert_eq!(working::background(&footer("12 files changed")), None);
    assert_eq!(working::background("no box at all · 1 shell still running"), None);
}

#[test]
fn the_background_line() {
    assert_eq!(working::background_line("1 shell still running"), serde_json::json!({"type": "background", "text": "1 shell still running"}));
}
