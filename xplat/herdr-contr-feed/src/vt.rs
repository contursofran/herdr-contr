//! A small terminal model: only as much as is needed to find faint text on a screen. A grid of cells with a faint
//! flag (SGR 2) and an inverse flag (SGR 7, the cursor that Claude Code draws). It knows printable text (UTF-8, wide
//! characters take 2 cells), CR, LF, BS, TAB, cursor moves (CUP, CUU, CUD, CUF, CUB, CHA, VPA), erase (ED, EL, ECH)
//! and SGR. It ignores all other sequences.

#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    pub ch: char,
    pub faint: bool,
    pub inverse: bool,
    /// The right half of a wide character.
    pub tail: bool,
}

const BLANK: Cell = Cell { ch: ' ', faint: false, inverse: false, tail: false };

#[derive(Debug, PartialEq)]
enum State {
    Ground,
    Esc,
    /// ESC with an intermediate byte (such as `ESC ( B`): one more byte ends it.
    EscInter,
    Csi,
    /// OSC, DCS and the other strings: they end at BEL or ESC \.
    Str,
    StrEsc,
}

pub struct Screen {
    pub cols: usize,
    pub rows: usize,
    pub grid: Vec<Vec<Cell>>,
    x: usize,
    y: usize,
    wrap: bool,
    faint: bool,
    inverse: bool,
    saved: (usize, usize),
    state: State,
    params: String,
    utf8: Vec<u8>,
}

impl Screen {
    pub fn new(cols: usize, rows: usize) -> Screen {
        let (cols, rows) = (cols.max(1), rows.max(1));
        Screen { cols, rows, grid: vec![vec![BLANK; cols]; rows], x: 0, y: 0, wrap: false, faint: false, inverse: false, saved: (0, 0), state: State::Ground, params: String::new(), utf8: Vec::new() }
    }

    /// A new size: the cells that fit stay.
    pub fn resize(&mut self, cols: usize, rows: usize) {
        let (cols, rows) = (cols.max(1), rows.max(1));
        for r in &mut self.grid {
            r.resize(cols, BLANK);
        }
        self.grid.resize(rows, vec![BLANK; cols]);
        (self.cols, self.rows) = (cols, rows);
        self.x = self.x.min(cols - 1);
        self.y = self.y.min(rows - 1);
        self.wrap = false;
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.byte(b);
        }
    }

    /// Each row as text (a wide character once) with a faint and an inverse flag for each character.
    pub fn lines(&self) -> Vec<Vec<&Cell>> {
        self.grid.iter().map(|r| r.iter().filter(|c| !c.tail).collect()).collect()
    }

    fn byte(&mut self, b: u8) {
        match self.state {
            State::Ground => self.ground(b),
            State::Esc => {
                self.state = State::Ground;
                match b {
                    b'[' => {
                        self.params.clear();
                        self.state = State::Csi;
                    }
                    b']' | b'P' | b'X' | b'^' | b'_' => self.state = State::Str,
                    b'7' => self.saved = (self.x, self.y),
                    b'8' => (self.x, self.y, self.wrap) = (self.saved.0, self.saved.1, false),
                    0x20..=0x2f => self.state = State::EscInter,
                    _ => {}
                }
            }
            State::EscInter => self.state = State::Ground,
            State::Csi => match b {
                0x40..=0x7e => {
                    self.state = State::Ground;
                    self.csi(b);
                }
                0x1b => self.state = State::Esc,
                _ => self.params.push(b as char),
            },
            State::Str => match b {
                0x07 => self.state = State::Ground,
                0x1b => self.state = State::StrEsc,
                _ => {}
            },
            State::StrEsc => self.state = if b == b'\\' { State::Ground } else { State::Str },
        }
    }

    fn ground(&mut self, b: u8) {
        if !self.utf8.is_empty() || b >= 0x80 {
            self.utf8.push(b);
            match std::str::from_utf8(&self.utf8) {
                Ok(s) => {
                    let c = s.chars().next().unwrap_or(' ');
                    self.utf8.clear();
                    self.print(c);
                }
                Err(e) if e.error_len().is_some() || self.utf8.len() >= 4 => {
                    self.utf8.clear();
                    self.print('\u{fffd}');
                }
                Err(_) => {}
            }
            return;
        }
        match b {
            0x1b => self.state = State::Esc,
            b'\r' => (self.x, self.wrap) = (0, false),
            b'\n' | 0x0b | 0x0c => self.line_feed(),
            0x08 => (self.x, self.wrap) = (self.x.saturating_sub(1), false),
            b'\t' => self.x = ((self.x / 8 + 1) * 8).min(self.cols - 1),
            0x20..=0x7e => self.print(b as char),
            _ => {}
        }
    }

    fn line_feed(&mut self) {
        self.wrap = false;
        if self.y + 1 < self.rows {
            self.y += 1;
        } else {
            self.grid.remove(0);
            self.grid.push(vec![BLANK; self.cols]);
        }
    }

    fn print(&mut self, c: char) {
        let w = if is_wide(c) { 2 } else { 1 };
        if self.wrap || self.x + w > self.cols {
            if w > self.cols {
                return;
            }
            self.x = 0;
            self.line_feed();
        }
        let cell = Cell { ch: c, faint: self.faint, inverse: self.inverse, tail: false };
        self.grid[self.y][self.x] = cell.clone();
        if w == 2 {
            self.grid[self.y][self.x + 1] = Cell { ch: ' ', tail: true, ..cell };
        }
        if self.x + w >= self.cols {
            self.x = self.cols - 1;
            self.wrap = true;
        } else {
            self.x += w;
        }
    }

    fn csi(&mut self, end: u8) {
        if self.params.starts_with(['?', '>', '<', '=']) {
            return; // modes and queries: not drawn
        }
        // A colon starts sub-parameters (`38:2:r:g:b`): they belong to the parameter before.
        let ps: Vec<Option<usize>> = self.params.split(';').map(|p| p.split(':').next().unwrap_or("").parse().ok()).collect();
        let n = |i: usize| ps.get(i).copied().flatten().filter(|v| *v > 0).unwrap_or(1);
        let raw = |i: usize| ps.get(i).copied().flatten().unwrap_or(0);
        self.wrap = false;
        match end {
            b'H' | b'f' => (self.y, self.x) = ((n(0) - 1).min(self.rows - 1), (n(1) - 1).min(self.cols - 1)),
            b'A' => self.y = self.y.saturating_sub(n(0)),
            b'B' => self.y = (self.y + n(0)).min(self.rows - 1),
            b'C' => self.x = (self.x + n(0)).min(self.cols - 1),
            b'D' => self.x = self.x.saturating_sub(n(0)),
            b'G' | b'`' => self.x = (n(0) - 1).min(self.cols - 1),
            b'd' => self.y = (n(0) - 1).min(self.rows - 1),
            b'J' => match raw(0) {
                0 => {
                    self.erase_row(self.y, self.x, self.cols);
                    (self.y + 1..self.rows).for_each(|r| self.erase_row(r, 0, self.cols));
                }
                1 => {
                    (0..self.y).for_each(|r| self.erase_row(r, 0, self.cols));
                    self.erase_row(self.y, 0, self.x + 1);
                }
                _ => (0..self.rows).for_each(|r| self.erase_row(r, 0, self.cols)),
            },
            b'K' => match raw(0) {
                0 => self.erase_row(self.y, self.x, self.cols),
                1 => self.erase_row(self.y, 0, self.x + 1),
                _ => self.erase_row(self.y, 0, self.cols),
            },
            b'X' => self.erase_row(self.y, self.x, self.x + n(0)),
            b's' => self.saved = (self.x, self.y),
            b'u' => (self.x, self.y) = self.saved,
            b'm' => self.sgr(&ps),
            _ => {}
        }
    }

    fn erase_row(&mut self, row: usize, from: usize, to: usize) {
        let to = to.min(self.cols);
        for c in &mut self.grid[row][from.min(to)..to] {
            *c = BLANK;
        }
    }

    fn sgr(&mut self, ps: &[Option<usize>]) {
        let mut i = 0;
        while i < ps.len() {
            match ps[i].unwrap_or(0) {
                0 => (self.faint, self.inverse) = (false, false),
                2 => self.faint = true,
                22 => self.faint = false,
                7 => self.inverse = true,
                27 => self.inverse = false,
                // Colors: skip their values, so that a 2 in them is not read as faint.
                38 | 48 | 58 => match ps.get(i + 1).copied().flatten() {
                    Some(5) => i += 2,
                    Some(2) => i += 4,
                    _ => {}
                },
                _ => {}
            }
            i += 1;
        }
    }
}

/// East Asian wide and emoji ranges: they take two cells.
pub fn is_wide(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x115f | 0x2e80..=0x303e | 0x3041..=0x33ff | 0x3400..=0x4dbf | 0x4e00..=0x9fff | 0xa000..=0xa4cf
        | 0xac00..=0xd7a3 | 0xf900..=0xfaff | 0xfe30..=0xfe4f | 0xff00..=0xff60 | 0xffe0..=0xffe6
        | 0x1f300..=0x1f64f | 0x1f900..=0x1f9ff | 0x20000..=0x3fffd)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &Screen, row: usize) -> String {
        s.lines()[row].iter().map(|c| c.ch).collect::<String>().trim_end().to_string()
    }
    fn faint(s: &Screen, row: usize) -> String {
        s.lines()[row].iter().map(|c| if c.faint { 'f' } else { '.' }).collect::<String>()
    }

    #[test]
    fn prints_text_with_cr_lf_and_wraps() {
        let mut s = Screen::new(5, 3);
        s.feed(b"ab\r\ncd");
        assert_eq!((text(&s, 0), text(&s, 1)), ("ab".into(), "cd".into()));
        let mut s = Screen::new(3, 2);
        s.feed(b"abcde");
        assert_eq!((text(&s, 0), text(&s, 1)), ("abc".into(), "de".into()));
        s.feed(b"\r\nxy");
        assert_eq!((text(&s, 0), text(&s, 1)), ("de".into(), "xy".into()), "a line feed at the bottom scrolls");
    }

    #[test]
    fn utf8_and_wide_characters() {
        let mut s = Screen::new(6, 1);
        s.feed("❯日b".as_bytes());
        assert_eq!(text(&s, 0), "❯日b");
        assert_eq!(s.grid[0][2].tail, true, "日 takes two cells");
        assert_eq!(s.grid[0][3].ch, 'b');
        // A character split over two feeds.
        let mut s = Screen::new(4, 1);
        let b = "é".as_bytes();
        s.feed(&b[..1]);
        s.feed(&b[1..]);
        assert_eq!(text(&s, 0), "é");
    }

    #[test]
    fn cursor_moves() {
        let mut s = Screen::new(10, 4);
        s.feed(b"\x1b[3;5HX\x1b[2AY\x1b[BZ\x1b[3DW\x1b[2CV\x1b[1GU\x1b[4dT");
        assert_eq!(text(&s, 0), "     Y");
        assert_eq!(text(&s, 1), "U   W ZV");
        assert_eq!(text(&s, 2), "    X");
        assert_eq!(text(&s, 3), " T");
    }

    #[test]
    fn erase_line_and_display() {
        let mut s = Screen::new(5, 3);
        s.feed(b"abcde\r\nfghij\r\nklmno");
        s.feed(b"\x1b[2;3H\x1b[K");
        assert_eq!(text(&s, 1), "fg");
        s.feed(b"\x1b[1K");
        assert_eq!(text(&s, 1), "");
        s.feed(b"\x1b[1;2H\x1b[J");
        assert_eq!((text(&s, 0), text(&s, 2)), ("a".into(), "".into()));
        s.feed(b"\x1b[2J");
        assert_eq!(text(&s, 0), "");
    }

    #[test]
    fn sgr_faint_on_and_off_and_colors_are_not_faint() {
        let mut s = Screen::new(8, 1);
        s.feed(b"a\x1b[2mb\x1b[22mc\x1b[2;31md\x1b[0me\x1b[38;5;2mf\x1b[38;2;2;2;2mg\x1b[2m\x1b[mh");
        assert_eq!(text(&s, 0), "abcdefgh");
        assert_eq!(faint(&s, 0), ".f.f....");
    }

    #[test]
    fn strings_and_modes_draw_nothing() {
        let mut s = Screen::new(10, 1);
        s.feed(b"\x1b]0;title\x07\x1b]8;;http://x\x1b\\a\x1b[?25l\x1b(Bb\x1b[?2026h");
        assert_eq!(text(&s, 0), "ab");
    }

    #[test]
    fn resize_keeps_the_cells_that_fit() {
        let mut s = Screen::new(3, 1);
        s.feed(b"abc");
        s.resize(5, 2);
        s.feed(b"\r\nxyzvw");
        assert_eq!((text(&s, 0), text(&s, 1)), ("abc".into(), "xyzvw".into()));
    }
}
