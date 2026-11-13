//! Claude Code's suggested prompt: the dim text in its empty input box. `terminal.read` gives text with no styles,
//! so the helper observes the terminal's bytes (`framed::first_paint`) and finds the faint text in a small terminal
//! model (`vt::Screen`).

use crate::framed::{Paint, PaintEvent};
use crate::queue::{is_rule, CLAUDE_PLACEHOLDER};
use crate::vt::{Cell, Screen};
use serde_json::{json, Value};

/// The grid size for the attach, until the core tells the terminal's size.
pub const COLS: u16 = 200;
pub const ROWS: u16 = 60;

pub fn line(text: &str) -> Value {
    json!({"type": "suggestion", "text": text})
}

/// The screen that a paint draws.
pub fn screen(paint: &Paint) -> Screen {
    let mut s = Screen::new(COLS as usize, ROWS as usize);
    for e in &paint.events {
        match e {
            PaintEvent::Data(b) => s.feed(b),
            PaintEvent::Resize(c, r) => s.resize(*c as usize, *r as usize),
        }
    }
    s
}

/// The suggestion in Claude Code's input box: the box is the rows between the last two rule lines, and its first
/// row starts with "❯ ". The text is there when every character after the mark is faint (the first one may be the
/// inverse cursor instead) and the box's other rows are empty. Else "": the owner typed, a dialog shows, or no box.
/// The placeholders ("Try \"…\"", the queue's note) are dim too: they are not suggestions.
pub fn pick(screen: &Screen) -> String {
    let lines = screen.lines();
    let text: Vec<String> = lines.iter().map(|r| r.iter().map(|c| c.ch).collect()).collect();
    let rules: Vec<usize> = (0..text.len()).filter(|&i| is_rule(&text[i])).collect();
    let [.., a, b] = rules[..] else { return String::new() };
    // No row in the box, or text on a second row.
    if b == a + 1 || lines[a + 2..b].iter().any(|r| !blank(r)) {
        return String::new();
    }
    let row = &lines[a + 1];
    let start = row.iter().position(|c| !is_space(c.ch));
    let Some(start) = start.filter(|&i| row[i].ch == '❯') else { return String::new() };
    let rest = &row[start + 1..];
    let Some(first) = rest.iter().position(|c| !is_space(c.ch)) else { return String::new() };
    let ok = rest[first..].iter().enumerate().all(|(i, c)| is_space(c.ch) || c.faint || (i == 0 && c.inverse));
    if !ok {
        return String::new();
    }
    let s: String = rest[first..].iter().map(|c| if c.ch == '\u{a0}' { ' ' } else { c.ch }).collect();
    let s = s.trim_end().to_string();
    if (s.starts_with("Try \"") && s.ends_with('"')) || s == CLAUDE_PLACEHOLDER {
        return String::new();
    }
    s
}

fn is_space(c: char) -> bool {
    c == ' ' || c == '\u{a0}'
}

fn blank(r: &[&Cell]) -> bool {
    r.iter().all(|c| is_space(c.ch))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RULE: &str = "────────────────────";

    /// Claude Code's box, drawn as its renderer does: rule, prompt row, rule, footer; `input` is the bytes after "❯ ".
    fn paint(input: &[u8], second: &[u8]) -> Paint {
        let mut b = Vec::new();
        b.extend_from_slice(b"\x1b[2J\x1b[H\x1b[1m\xe2\x97\x8f\x1b[22m Done.\r\n\r\n");
        b.extend_from_slice(format!("\x1b[38;5;244m{RULE}\x1b[39m\r\n").as_bytes());
        b.extend_from_slice("❯\u{a0}".as_bytes());
        b.extend_from_slice(input);
        b.extend_from_slice(b"\x1b[0m\r\n");
        b.extend_from_slice(second);
        if !second.is_empty() {
            b.extend_from_slice(b"\r\n");
        }
        b.extend_from_slice(format!("{RULE}\r\n  \x1b[2m? for shortcuts\x1b[22m").as_bytes());
        Paint { events: vec![PaintEvent::Resize(40, 10), PaintEvent::Data(b)] }
    }

    fn pick_of(p: &Paint) -> String {
        pick(&screen(p))
    }

    #[test]
    fn the_dim_text_in_the_empty_box_is_the_suggestion() {
        assert_eq!(pick_of(&paint(b"\x1b[2mrun the tests\x1b[22m", b"")), "run the tests");
        // Claude Code's cursor: the first character inverse, the rest dim.
        assert_eq!(pick_of(&paint(b"\x1b[7mr\x1b[27m\x1b[2mun the tests\x1b[22m", b"")), "run the tests");
    }

    #[test]
    fn typed_text_is_not_a_suggestion() {
        assert_eq!(pick_of(&paint(b"run the tests", b"")), "");
        assert_eq!(pick_of(&paint(b"run \x1b[2mthe tests", b"")), "", "a part typed");
        assert_eq!(pick_of(&paint(b"\x1b[2mrun the tests", b"  more")), "", "text on a second row");
    }

    #[test]
    fn an_empty_box_placeholders_and_no_box_give_nothing() {
        assert_eq!(pick_of(&paint(b"", b"")), "");
        assert_eq!(pick_of(&paint(b"\x1b[2mTry \"fix typecheck errors\"", b"")), "");
        assert_eq!(pick_of(&paint(format!("\x1b[2m{CLAUDE_PLACEHOLDER}").as_bytes(), b"")), "");
        let p = Paint { events: vec![PaintEvent::Data(b"$ \x1b[2mls".to_vec())] };
        assert_eq!(pick_of(&p), "", "not Claude Code");
        let p = Paint { events: vec![PaintEvent::Data(format!("{RULE}\r\n Do you want to proceed?\r\n \x1b[2m1. Yes\r\n{RULE}").into_bytes())] };
        assert_eq!(pick_of(&p), "", "a dialog");
    }

    #[test]
    fn a_wide_suggestion_and_a_suggestion_drawn_with_cursor_moves() {
        assert_eq!(pick_of(&paint("\x1b[2m日本語で説明して".as_bytes(), b"")), "日本語で説明して");
        // The renderer draws the box, then goes up and writes the suggestion into its row.
        let mut p = paint(b"", b"");
        if let PaintEvent::Data(b) = &mut p.events[1] {
            b.extend_from_slice(b"\x1b[2A\x1b[3G\x1b[2mcommit this\x1b[22m");
        }
        assert_eq!(pick_of(&p), "commit this");
    }
}
