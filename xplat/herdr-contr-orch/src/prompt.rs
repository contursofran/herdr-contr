//! Reads the menu that a blocked worker shows on its terminal: the question and the numbered choices.
//! Ported from contr's `internal/prompt`. It is pure: screen text in, the prompt out. The screen is the only
//! place where hc and the agents give the text of a menu.
use crate::workers::is_version;

/// The most choices that one digit key can pick.
const MAX_CHOICES: usize = 9;
/// The most lines above the question in which the top of the menu's box is looked for.
const MAX_DETAIL: usize = 12;

/// A question with its choices.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Prompt {
    /// The lines of the menu's box above the question: for a permission prompt, the tool and its command.
    /// Empty when the menu has no box or separator above it.
    pub detail: Vec<String>,
    pub question: String,
    /// The label of each choice; the digit key n picks the nth.
    pub choices: Vec<String>,
}

impl Prompt {
    /// The detail, the question and the numbered choices, one on each line.
    pub fn text(&self) -> String {
        let mut lines = self.detail.clone();
        lines.push(self.question.clone());
        lines.extend(self.choices.iter().enumerate().map(|(i, c)| format!("{}. {c}", i + 1)));
        lines.join("\n").trim_end_matches('\n').to_string()
    }
}

/// Reads the last menu on the screen. `command` is the worker's command, and `kind` is the blocked kind
/// (permission, question, auth). None for an agent that is not Claude Code or Codex, an auth prompt, or a
/// screen with no menu. Claude Code can name itself by its version.
pub fn parse(command: &str, kind: &str, screen: &str) -> Option<Prompt> {
    let program = command.split_whitespace().next().unwrap_or_default();
    let name = program.rsplit('/').next().unwrap_or_default();
    if kind == "auth" || !(name == "claude" || name == "codex" || is_version(name)) {
        return None;
    }
    parse_menu(screen)
}

/// One screen line less the box characters and the cursor mark.
fn clean(line: &str) -> &str {
    let line = line.trim_matches(|c| " \t\r│┃╭╮╰╯─━".contains(c));
    let line = ["❯", "›", ">"].iter().find_map(|m| line.strip_prefix(m)).unwrap_or(line);
    line.trim()
}

/// The number and the label of a line "<n>. <label>".
fn item(line: &str) -> Option<(usize, &str)> {
    let (digits, rest) = line.split_once('.')?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let label = rest.trim_start();
    (label.len() < rest.len() && !label.is_empty()).then(|| digits.parse().ok().map(|n| (n, label.trim_end())))?
}

/// The last run of numbered items that counts up from 1, with the nearest text line above it.
fn parse_menu(screen: &str) -> Option<Prompt> {
    let raw: Vec<&str> = screen.split('\n').collect();
    let lines: Vec<&str> = raw.iter().map(|l| clean(l)).collect();
    let items: Vec<(usize, usize, &str)> = lines.iter().enumerate().filter_map(|(i, l)| item(l).map(|(n, label)| (i, n, label))).collect();
    // Walk back from the last item until the numbers reach 1.
    let mut start = items.len().checked_sub(1)?;
    while items[start].1 != 1 {
        if start == 0 || items[start - 1].1 + 1 != items[start].1 {
            return None;
        }
        start -= 1;
    }
    let run = &items[start..];
    if run.len() < 2 || run.len() > MAX_CHOICES {
        return None;
    }
    let mut p = Prompt { choices: run.iter().map(|it| it.2.to_string()).collect(), ..Prompt::default() };
    if let Some(q) = (0..run[0].0).rev().find(|i| !lines[*i].is_empty()) {
        p.question = lines[q].to_string();
        p.detail = detail(&raw, &lines, q);
    }
    Some(p)
}

/// The cleaned lines between the top of the menu's box (a ╭ line or a line of ─) and the question line `q`.
/// With no top within MAX_DETAIL lines, or with agent output (●) or another menu item in between, there is no
/// detail.
fn detail(raw: &[&str], lines: &[&str], q: usize) -> Vec<String> {
    for i in (q.saturating_sub(MAX_DETAIL)..q).rev() {
        let r = raw[i].trim();
        if r.starts_with('╭') || (!r.is_empty() && r.chars().all(|c| c == '─' || c == '━')) {
            return lines[i + 1..q].iter().filter(|l| !l.is_empty()).map(|l| l.to_string()).collect();
        }
        if lines[i].starts_with('●') || item(lines[i]).is_some() {
            break;
        }
    }
    Vec::new()
}
