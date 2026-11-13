//! A panel that Claude Code draws in place of its input box (the screen of `/model`, `/config`, `/agents`,
//! `/resume`): its plain text for the chat's panel card, and the few keys that card sends. hc gives no text styles:
//! a selected row shows only when the panel marks it with a character.

use crate::cards::{self, Done};
use crate::queue;
use herdr_core::{keygate, AgentStatus, Pane, PaneScreen};
use serde_json::{json, Value};

/// The keys the panel card can send: arrows, Enter, Tab, Space, Escape and the digits.
pub fn allowed(key: &str) -> bool {
    matches!(key, "Up" | "Down" | "Left" | "Right" | "Enter" | "Tab" | "Space" | "Escape")
        || (key.len() == 1 && key.as_bytes()[0].is_ascii_digit())
}

/// The agents whose panels herdr-contr reads.
pub fn reads(kind: &str) -> bool {
    kind == "claude"
}

fn is_rule(line: &str) -> bool {
    let t = line.trim();
    t.chars().count() >= 10 && t.chars().all(|c| c == '─')
}

/// The panel's lines: the screen has no Claude Code input box, so a panel stands in its place. The lines under the
/// last rule line (the panel's top), or the whole screen when it has no rule. Blank lines at the ends go.
/// `None`: the input box is on screen, or nothing is left.
pub fn read(screen: &str) -> Option<Vec<String>> {
    if queue::box_has_text("claude", screen, false).is_some() {
        return None;
    }
    let lines: Vec<String> = screen.split('\n').map(|l| l.replace('\u{a0}', " ").trim_end().to_string()).collect();
    let start = lines.iter().rposition(|l| is_rule(l)).map(|i| i + 1).unwrap_or(0);
    let mut out: Vec<String> = lines[start..].to_vec();
    while out.last().is_some_and(|l| l.trim().is_empty()) {
        out.pop();
    }
    let lead = out.iter().position(|l| !l.trim().is_empty())?;
    Some(out.split_off(lead))
}

/// The `panel` line; no panel: `lines` is `null`.
pub fn line(lines: Option<&Vec<String>>) -> Value {
    json!({"type": "panel", "lines": lines})
}

/// The panel on this pane now: a Claude Code agent that does not work, with no approval or question card on screen.
pub fn on_pane<S: PaneScreen + ?Sized>(pane: &Pane, screen: &S, log: Option<&std::path::Path>) -> Option<Vec<String>> {
    pane.agent.as_ref().filter(|a| reads(&a.kind) && a.status != AgentStatus::Working)?;
    if cards::current(pane, screen, log).is_some() {
        return None;
    }
    read(&screen.visible_text(&pane.id).ok()?)
}

/// Sends `keys` to the panel, one gated key at a time: before each key the pane must still hold the same agent and
/// show a panel. A key that is not allowed: nothing is sent.
pub fn send<S: PaneScreen + ?Sized>(pane: &Pane, screen: &S, log: Option<&std::path::Path>, keys: &[String]) -> Done {
    if keys.is_empty() || !keys.iter().all(|k| allowed(k)) {
        return Done::new("failed", Some("the panel card sends only arrows, Enter, Tab, Space, Escape and digits"));
    }
    if pane.agent.is_none() || on_pane(pane, screen, log).is_none() {
        return Done::new("changed", Some("no panel on screen now; nothing was sent"));
    }
    let ok = |s: &str| read(s).is_some();
    match keygate::gated_keys(screen, keys, Some(&cards::gate(pane)), &pane.id, Some(&ok)) {
        Ok(()) => Done::new("sent", None),
        Err(herdr_core::HerdrError::KeyGateStop) => Done::new("changed", Some("the panel closed or the agent changed; check the terminal")),
        Err(_) => Done::new("failed", Some("herdr-core did not take the keys")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use herdr_core::fake::FakeScreen;
    use herdr_core::{Agent, BackendKind};

    const RULE: &str = "────────────────────────────────────────";
    fn model_panel() -> String {
        format!("⏺ Done.\n\n{RULE}\n Select model\n\n ❯ 1. Default (Opus)  ✔\n   2. Sonnet\n\n Enter to confirm · Esc to exit\n\n")
    }
    fn input_box() -> String {
        format!("⏺ Done.\n\n{RULE}\n❯ \n{RULE}\n  ? for shortcuts\n")
    }
    fn pane(status: AgentStatus) -> Pane {
        Pane {
            id: "p".into(),
            terminal_id: "t".into(),
            agent: Some(Agent { kind: "claude".into(), status, process: Some("1@2".into()), backend: BackendKind::Hc, ..Default::default() }),
            ..Default::default()
        }
    }

    #[test]
    fn reads_the_lines_under_the_last_rule() {
        assert_eq!(
            read(&model_panel()).unwrap(),
            vec![" Select model", "", " ❯ 1. Default (Opus)  ✔", "   2. Sonnet", "", " Enter to confirm · Esc to exit"]
        );
    }

    #[test]
    fn no_panel_while_the_input_box_shows() {
        assert_eq!(read(&input_box()), None);
    }

    #[test]
    fn whole_screen_when_there_is_no_rule() {
        assert_eq!(read("\n Resume a conversation\n ❯ fix tests\n").unwrap(), vec![" Resume a conversation", " ❯ fix tests"]);
        assert_eq!(read("\n\n"), None);
    }

    #[test]
    fn only_the_listed_keys() {
        for k in ["Up", "Down", "Left", "Right", "Enter", "Tab", "Space", "Escape", "0", "9"] {
            assert!(allowed(k), "{k}");
        }
        for k in ["a", "C-c", "12", "enter", "BTab", ""] {
            assert!(!allowed(k), "{k}");
        }
    }

    #[test]
    fn sends_keys_while_the_panel_shows() {
        let p = pane(AgentStatus::Idle);
        let s = FakeScreen::with_panes(vec![p.clone()], vec![&model_panel()]);
        let d = send(&p, &s, None, &["Down".into(), "Enter".into()]);
        assert_eq!(d.result, "sent");
        assert_eq!(s.sent(), vec!["Down", "Enter"]);
    }

    #[test]
    fn sends_nothing_without_a_panel_or_with_a_bad_key() {
        let p = pane(AgentStatus::Idle);
        let s = FakeScreen::with_panes(vec![p.clone()], vec![&input_box()]);
        assert_eq!(send(&p, &s, None, &["Enter".into()]).result, "changed");
        let s = FakeScreen::with_panes(vec![p.clone()], vec![&model_panel()]);
        assert_eq!(send(&p, &s, None, &["x".into()]).result, "failed");
        assert!(s.sent().is_empty());
    }

    #[test]
    fn no_panel_while_the_agent_works() {
        let p = pane(AgentStatus::Working);
        let s = FakeScreen::with_panes(vec![p.clone()], vec![&model_panel()]);
        assert_eq!(on_pane(&p, &s, None), None);
        assert_eq!(on_pane(&pane(AgentStatus::Idle), &FakeScreen::with_panes(vec![pane(AgentStatus::Idle)], vec![&model_panel()]), None).map(|l| l.len()), Some(6));
    }
}
