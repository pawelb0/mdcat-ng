// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Viewport and draw loop.
//!
//! [`View`] owns the scroll offset and size. [`draw`](View::draw)
//! emits the next frame; [`draw_toc`](View::draw_toc) replaces the
//! body when the TOC modal is open.

use std::io::{self, Write};

use super::buffer::{HeadingEntry, RenderedDoc};
use super::highlight::{self, Highlight};
use super::keys::Command;
use super::search::Match;
use super::toc::Toc;

/// Scroll state bound to a terminal size.
#[derive(Debug, Clone, Copy)]
#[allow(missing_docs)]
pub struct View {
    /// Zero-indexed rendered line at the top of the viewport.
    pub top: usize,
    pub cols: u16,
    pub rows: u16,
    /// Dim 1-indexed line-number gutter. Toggled live with `#`.
    pub line_numbers: bool,
}

/// Columns the gutter reserves: `NNN │ ` = digits(3) + separator(3).
pub const GUTTER: u16 = 6;

impl View {
    /// Viewport at `(cols, rows)`, scrolled to the top.
    pub fn new(cols: u16, rows: u16) -> Self {
        Self {
            top: 0,
            cols,
            rows,
            line_numbers: false,
        }
    }

    /// Builder toggle for the line-number gutter.
    pub fn with_line_numbers(mut self, on: bool) -> Self {
        self.line_numbers = on;
        self
    }

    /// Body rows available above the status line; at least 1.
    fn body_rows(&self) -> usize {
        self.rows.saturating_sub(1).max(1) as usize
    }

    fn max_top(&self, doc: &RenderedDoc) -> usize {
        doc.line_count().saturating_sub(self.body_rows())
    }

    /// Apply `cmd` to the scroll state. Returns `true` on `Quit`.
    ///
    /// Search / highlight / redraw commands are handled by the event
    /// loop; this method only reacts to scroll commands.
    pub fn apply(&mut self, cmd: Command, doc: &RenderedDoc) -> bool {
        let max = self.max_top(doc);
        let body = self.body_rows();
        match cmd {
            Command::Quit => return true,
            Command::ScrollDown(n) => self.top = (self.top + n as usize).min(max),
            Command::ScrollUp(n) => self.top = self.top.saturating_sub(n as usize),
            Command::PageDown => self.top = (self.top + body).min(max),
            Command::PageUp => self.top = self.top.saturating_sub(body),
            Command::HalfPageDown => self.top = (self.top + body / 2).min(max),
            Command::HalfPageUp => self.top = self.top.saturating_sub(body / 2),
            Command::Home => self.top = 0,
            Command::End => self.top = max,
            Command::GotoLine(n) => self.top = n.saturating_sub(1).min(max),
            _ => {}
        }
        false
    }

    /// Scroll so `line` sits near the top, leaving a two-line breadcrumb
    /// above it when the document has room. Jumps for search / heading
    /// navigation use this; bookmarks want exact placement and call
    /// [`jump_to`](Self::jump_to) instead.
    pub fn scroll_to(&mut self, line: usize, doc: &RenderedDoc) {
        self.top = line.saturating_sub(2).min(self.max_top(doc));
    }

    /// Place `line` at the exact top of the viewport, clamped to the doc.
    pub fn jump_to(&mut self, line: usize, doc: &RenderedDoc) {
        self.top = line.min(self.max_top(doc));
    }

    /// Update size after a terminal resize and clamp `top` to the new end.
    pub fn resize(&mut self, cols: u16, rows: u16, doc: &RenderedDoc) {
        self.cols = cols;
        self.rows = rows;
        self.top = self.top.min(self.max_top(doc));
    }

    /// Render the visible document slice + status line to `out`.
    ///
    /// `status` controls the bottom row: `None` draws the default
    /// position indicator, `Some("…")` draws whatever the caller
    /// supplies (used for search prompt and search summaries).
    /// `matches` lists highlight ranges (plain byte ranges) across the
    /// whole document; the draw routine filters per-line and maps into
    /// styled byte offsets via `RenderedDoc::line_starts`.
    pub fn draw<W: Write>(
        &self,
        out: &mut W,
        doc: &RenderedDoc,
        matches: &[Match],
        current: Option<&Match>,
        status: Option<&str>,
    ) -> io::Result<()> {
        out.write_all(b"\x1b[H\x1b[0J")?;

        let body = self.body_rows();
        // Gutter width scales to the document so 1M-line docs still fit.
        let gutter_width = if self.line_numbers {
            digit_count(doc.line_count()).max(3)
        } else {
            0
        };
        for row in 0..body {
            // Reset SGR before each row so style leaks from multi-line
            // blocks (HTML colouring, code-block box fills) don't stain
            // the gutter or the next line's prose.
            out.write_all(b"\x1b[0m")?;
            let line_index = self.top + row;
            let past_eof = line_index >= doc.line_count();
            if self.line_numbers {
                let n = if past_eof { None } else { Some(line_index + 1) };
                write_gutter(out, gutter_width, n)?;
            }
            if past_eof {
                out.write_all(b"\r\n")?;
                continue;
            }
            let line_bytes = doc.styled_line(line_index);
            // Raw mode needs CR before LF, otherwise the cursor stays
            // in the previous column and every subsequent row is
            // indented further right. Strip the renderer's trailing
            // `\n` and emit `\r\n` ourselves.
            let content = line_bytes.strip_suffix(b"\n").unwrap_or(line_bytes);
            let hl = line_highlights(doc, line_index, matches, current);
            highlight::write_line(out, content, &hl)?;
            out.write_all(b"\r\n")?;
        }

        self.draw_status(out, doc, status)?;
        out.flush()
    }

    /// Render the TOC modal: full-frame heading list with the selected
    /// row reversed, plus a status line prompting navigation keys.
    pub fn draw_toc<W: Write>(
        &self,
        out: &mut W,
        headings: &[HeadingEntry],
        toc: &Toc,
    ) -> io::Result<()> {
        out.write_all(b"\x1b[H\x1b[0J")?;
        let body = self.body_rows();
        toc.draw(out, headings, body)?;
        out.write_all(b"\x1b[7m")?;
        if headings.is_empty() {
            out.write_all(b"-- TOC --  (document has no headings)  Esc:close")?;
        } else {
            write!(
                out,
                "-- TOC --  {}/{}  Enter:jump  Esc/T:close  j/k:move",
                toc.selected + 1,
                headings.len(),
            )?;
        }
        out.write_all(b"\x1b[0m")?;
        out.flush()
    }

    fn draw_status<W: Write>(
        &self,
        out: &mut W,
        doc: &RenderedDoc,
        status: Option<&str>,
    ) -> io::Result<()> {
        out.write_all(b"\x1b[7m")?;
        match status {
            Some(text) => out.write_all(text.as_bytes())?,
            None => {
                let body = self.body_rows();
                let percent = if doc.line_count() <= body {
                    100
                } else {
                    ((self.top + body).min(doc.line_count()) * 100 / doc.line_count()).min(100)
                };
                write!(
                    out,
                    "-- mdless --  line {}/{} ({percent}%)  q:quit  /:search  ]]:next  T:toc  m/':mark",
                    self.top + 1,
                    doc.line_count(),
                )?;
            }
        }
        out.write_all(b"\x1b[0m")
    }
}

/// Decimal digit count of `n`, with a floor of 1.
fn digit_count(n: usize) -> usize {
    n.checked_ilog10().map_or(1, |log| log as usize + 1)
}

/// Write one row's gutter. `number = None` leaves a blank field (past-EOF
/// rows) so the separator column stays aligned. Dim SGR keeps the number
/// quiet enough to read as chrome rather than content.
fn write_gutter<W: Write>(out: &mut W, width: usize, number: Option<usize>) -> io::Result<()> {
    match number {
        Some(n) => write!(out, "\x1b[2m{n:>width$} │\x1b[0m "),
        None => write!(out, "\x1b[2m{:>width$} │\x1b[0m ", ""),
    }
}

/// Collect the highlight ranges that intersect `line`, translated to
/// line-local styled byte offsets.
fn line_highlights(
    doc: &RenderedDoc,
    line: usize,
    matches: &[Match],
    current: Option<&Match>,
) -> Highlight {
    let line_start = doc.styled_line_starts[line];
    let mut hl = Highlight::default();
    for m in matches {
        if m.line != line {
            continue;
        }
        let local = (m.styled.start - line_start)..(m.styled.end - line_start);
        if current.is_some_and(|c| std::ptr::eq(c, m)) {
            hl.current = Some(local);
        } else {
            hl.others.push(local);
        }
    }
    hl
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mdless::buffer;

    fn doc(n_lines: usize) -> RenderedDoc {
        use std::fmt::Write;
        let mut styled = String::new();
        for i in 0..n_lines {
            writeln!(styled, "line {i}").unwrap();
        }
        buffer::build(styled.into_bytes(), Vec::new())
    }

    #[test]
    fn scroll_down_clamps_at_end() {
        let d = doc(10);
        let mut v = View::new(80, 5); // 4 body rows + 1 status
        v.apply(Command::ScrollDown(100), &d);
        assert_eq!(v.top, 10 - 4);
    }

    #[test]
    fn page_down_moves_by_body_rows() {
        let d = doc(20);
        let mut v = View::new(80, 6); // body = 5
        v.apply(Command::PageDown, &d);
        assert_eq!(v.top, 5);
    }

    #[test]
    fn goto_line_uses_one_indexed_input() {
        let d = doc(10);
        let mut v = View::new(80, 5);
        v.apply(Command::GotoLine(7), &d);
        assert_eq!(v.top, 6);
    }

    #[test]
    fn home_and_end_flip_between_boundaries() {
        let d = doc(50);
        let mut v = View::new(80, 10);
        v.apply(Command::End, &d);
        assert_eq!(v.top, 50 - 9);
        v.apply(Command::Home, &d);
        assert_eq!(v.top, 0);
    }

    #[test]
    fn draw_emits_first_body_lines_and_status() {
        let d = doc(10);
        let v = View::new(80, 4); // 3 body rows
        let mut out = Vec::new();
        v.draw(&mut out, &d, &[], None, None).unwrap();
        let s = String::from_utf8(out).unwrap();
        // Lines emitted CR-LF-terminated for raw mode.
        assert!(s.contains("line 0\r\n"));
        assert!(s.contains("line 1\r\n"));
        assert!(s.contains("line 2\r\n"));
        assert!(!s.contains("line 3"));
        assert!(s.contains("line 1/10"));
    }

    #[test]
    fn draw_with_custom_status_uses_it() {
        let d = doc(5);
        let v = View::new(80, 4);
        let mut out = Vec::new();
        v.draw(&mut out, &d, &[], None, Some("/needle_")).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("/needle_"));
        assert!(!s.contains("-- mdless --"));
    }

    #[test]
    fn scroll_to_places_line_near_top_with_breadcrumb() {
        let d = doc(40);
        let mut v = View::new(80, 10);
        v.scroll_to(15, &d);
        assert_eq!(v.top, 13);
    }

    #[test]
    fn draw_with_line_numbers_prefixes_each_row() {
        let d = doc(12);
        let v = View::new(80, 4).with_line_numbers(true); // 3 body rows
        let mut out = Vec::new();
        v.draw(&mut out, &d, &[], None, None).unwrap();
        let s = String::from_utf8(out).unwrap();
        // Two-digit gutter (document has 12 lines, floor is 3 columns).
        // SGR reset closes the dim effect between gutter and body text.
        assert!(s.contains("  1 │\x1b[0m line 0"), "row 1: {s}");
        assert!(s.contains("  2 │\x1b[0m line 1"), "row 2: {s}");
        assert!(s.contains("  3 │\x1b[0m line 2"), "row 3: {s}");
    }

    #[test]
    fn resize_clamps_top() {
        let d = doc(10);
        let mut v = View::new(80, 5);
        v.apply(Command::End, &d);
        assert_eq!(v.top, 6);
        v.resize(80, 20, &d); // body_rows grows; top clamps down.
        assert_eq!(v.top, 0);
    }

    #[test]
    fn apply_returns_true_on_quit() {
        let d = doc(1);
        let mut v = View::new(80, 5);
        assert!(v.apply(Command::Quit, &d));
    }
}
