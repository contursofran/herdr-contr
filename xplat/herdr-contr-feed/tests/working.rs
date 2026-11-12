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
