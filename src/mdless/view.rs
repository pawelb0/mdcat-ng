// Copyright 2026 Pawel Boguszewski
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Viewport for the interactive pager.
//!
//! This file contains:
//!
//! - [`View`] — scroll offset, terminal size, gutter toggle. Mutated
//!   by the event loop (a `j` keystroke advances `top` by one, a
//!   resize event updates `cols`/`rows`, and so on).
//! - [`View::draw`] — one-frame render. Given a [`RenderedDoc`] and
//!   the current search highlights, writes the visible slice plus
//!   the status line to any `Write`. Tests assert on `Vec<u8>`;
//!   production writes to stdout.
//! - [`View::draw_toc`] — modal frame replacing the body when the
//!   TOC is open.
//! - Private helpers for the line-number gutter.
//!
//! How it fits: `mdless::run` owns a single `View` plus a
//! `RenderedDoc`. Every keystroke calls into `View::apply` (scroll
//! commands), `View::scroll_to` (search/heading jumps), or
//! `View::jump_to` (bookmarks), then `View::draw` emits the next
//! frame. Search + heading modules never touch `View` directly —
//! they hand `mdless::mod` a target line, which calls the scroll
//! method.

use std::io::{self, Write};

use super::buffer::{HeadingEntry, RenderedDoc};
use super::highlight::{self, Highlight};
use super::keys::Command;
use super::search::Match;
use super::toc::Toc;

/// Scroll state bound to a terminal size.
#[derive(Debug, Clone, Copy)]
pub struct View {
    /// Rendered line currently at the top of the viewport (0-indexed).
    pub top: usize,
    /// Terminal width in columns.
    pub cols: u16,
    /// Terminal height in rows, including the status line.
    pub rows: u16,
    /// When true, each body row renders with a dim left-gutter
    /// showing its 1-indexed rendered line number. Toggled live
    /// with `#` and switched on at startup via `--line-numbers`.
    pub line_numbers: bool,
}

/// Columns stolen by the gutter: digit field (3) + ` │ ` separator (3).
///
/// Public because `render_doc` reserves these columns at render time
/// so code-block frames and tables fit once the gutter paints.
pub const GUTTER: u16 = 6;

impl View {
    /// New view at `(cols, rows)`, scrolled to the top.
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

    /// Number of document rows the viewport can show. Last row is the
    /// status line; guarantees at least 1.
    fn body_rows(&self) -> usize {
        self.rows.saturating_sub(1).max(1) as usize
    }

    /// Apply `cmd` to the scroll state. Returns `true` when the caller
    /// should quit the event loop.
    ///
    /// Search-related commands do not affect the scroll position; the
    /// event loop in [`crate::mdless`] wires those to the search state
    /// and may call [`Self::scroll_to`] separately.
    pub fn apply(&mut self, cmd: Command, doc: &RenderedDoc) -> bool {
        let max_top = doc.line_count().saturating_sub(self.body_rows());
        match cmd {
            Command::Quit => return true,
            Command::ScrollDown(n) => self.top = self.top.saturating_add(n as usize).min(max_top),
            Command::ScrollUp(n) => self.top = self.top.saturating_sub(n as usize),
            Command::PageDown => self.top = self.top.saturating_add(self.body_rows()).min(max_top),
            Command::PageUp => self.top = self.top.saturating_sub(self.body_rows()),
            Command::HalfPageDown => {
                self.top = self.top.saturating_add(self.body_rows() / 2).min(max_top);
            }
            Command::HalfPageUp => self.top = self.top.saturating_sub(self.body_rows() / 2),
            Command::Home => self.top = 0,
            Command::End => self.top = max_top,
            Command::GotoLine(n) => self.top = n.saturating_sub(1).min(max_top),
            // Non-scrolling commands (search, highlights, redraw, noop)
            // are handled by the event loop, not the view.
            _ => {}
        }
        false
    }

    /// Scroll so that `line` sits inside the viewport. Places the target
    /// near the top with a short breadcrumb above when space allows.
    pub fn scroll_to(&mut self, line: usize, doc: &RenderedDoc) {
        let max_top = doc.line_count().saturating_sub(self.body_rows());
        let desired = line.saturating_sub(2);
        self.top = desired.min(max_top);
    }

    /// Set the viewport top to `line` exactly, clamped to the document.
    ///
    /// Used for bookmarks so jumping to a saved line round-trips the
    /// view faithfully (no breadcrumb shift the way `scroll_to` does).
    pub fn jump_to(&mut self, line: usize, doc: &RenderedDoc) {
        let max_top = doc.line_count().saturating_sub(self.body_rows());
        self.top = line.min(max_top);
    }

    /// Resize the viewport and clamp `top` so we don't scroll past the end.
    pub fn resize(&mut self, cols: u16, rows: u16, doc: &RenderedDoc) {
        self.cols = cols;
        self.rows = rows;
        let max_top = doc.line_count().saturating_sub(self.body_rows());
        self.top = self.top.min(max_top);
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
            if line_index >= doc.line_count() {
                if self.line_numbers {
                    write_gutter_blank(out, gutter_width)?;
                }
                out.write_all(b"\r\n")?;
                continue;
            }
            if self.line_numbers {
                write_gutter_number(out, gutter_width, line_index + 1)?;
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

/// Decimal digit count of `n`, with a floor of 1 so `0` still reserves
/// one column of gutter.
fn digit_count(n: usize) -> usize {
    n.checked_ilog10().map_or(1, |log| log as usize + 1)
}

/// Write the line-number gutter: right-aligned number, separator, space.
///
/// Dim SGR keeps the number quiet next to the body so it reads as
/// chrome rather than content.
fn write_gutter_number<W: Write>(out: &mut W, width: usize, n: usize) -> io::Result<()> {
    write!(out, "\x1b[2m{n:>width$} │\x1b[0m ")
}

/// Blank gutter: same footprint, no number. Used for empty rows after
/// the document ends so the separator column stays aligned.
fn write_gutter_blank<W: Write>(out: &mut W, width: usize) -> io::Result<()> {
    write!(out, "\x1b[2m{:>width$} │\x1b[0m ", "")
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
