// Copyright 2026 Pawel Boguszewski
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Table-of-contents modal for the interactive pager.
//!
//! This file contains:
//!
//! - [`Toc`] — modal state: which heading index is currently
//!   highlighted. `j` / `k` move the selection, `Enter` activates,
//!   `Esc` closes.
//! - [`Toc::draw`] — paints the heading list as a full-frame
//!   overlay, reverse-video on the selected row, scrolling the
//!   list so the selection stays visible.
//!
//! How it fits: the dispatch layer in `mdless::mod` holds an
//! `Option<Toc>`. Pressing `T` toggles the option; while it's
//! `Some`, scroll-style keystrokes move the selection instead of
//! the viewport, and `Enter` scrolls the body to the selected
//! heading via `view::scroll_to`. Entries come from
//! [`buffer::HeadingEntry`](super::buffer::HeadingEntry), populated
//! at render time by the [`HeadingRecorder`](super::buffer::HeadingRecorder).

use std::io::{self, Write};

use super::buffer::HeadingEntry;

/// State of an open TOC modal.
#[derive(Debug, Clone, Copy, Default)]
pub struct Toc {
    /// Index into `RenderedDoc::headings` of the highlighted entry.
    pub selected: usize,
}

impl Toc {
    /// New modal with the first heading selected.
    pub fn new(_headings: &[HeadingEntry]) -> Self {
        Self::default()
    }

    /// Move the selection by `delta`, clamped to the heading count.
    pub fn step(&mut self, delta: isize, total: usize) {
        if total == 0 {
            self.selected = 0;
            return;
        }
        let max = (total - 1) as isize;
        let next = self.selected as isize + delta;
        self.selected = next.clamp(0, max) as usize;
    }

    /// Draw `rows` heading rows, reverse-video marking the selected row.
    ///
    /// Scrolls the list so the selection stays on-screen; entries past
    /// the end render as blank rows.
    pub fn draw<W: Write>(
        &self,
        out: &mut W,
        headings: &[HeadingEntry],
        rows: usize,
    ) -> io::Result<()> {
        // Keep the selection in the top half of the modal so new users
        // see the next few headings at a glance.
        let top = self.selected.saturating_sub(rows / 2);
        for row in 0..rows {
            let idx = top + row;
            if let Some(h) = headings.get(idx) {
                let indent = " ".repeat(usize::from(h.level.saturating_sub(1)) * 2);
                if idx == self.selected {
                    write!(out, "\x1b[7m{indent}{}\x1b[0m\r\n", h.text)?;
                } else {
                    write!(out, "{indent}{}\r\n", h.text)?;
                }
            } else {
                out.write_all(b"\r\n")?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(n: usize) -> Vec<HeadingEntry> {
        (0..n)
            .map(|i| HeadingEntry {
                level: 1,
                text: format!("heading {i}"),
                plain_offset: i * 10,
            })
            .collect()
    }

    #[test]
    fn step_clamps_to_bounds() {
        let hs = entries(3);
        let mut t = Toc::new(&hs);
        t.step(-5, hs.len());
        assert_eq!(t.selected, 0);
        t.step(10, hs.len());
        assert_eq!(t.selected, 2);
    }

    #[test]
    fn draw_marks_selected_entry_with_reverse_sgr() {
        let hs = entries(3);
        let mut t = Toc::new(&hs);
        t.selected = 1;
        let mut out = Vec::new();
        t.draw(&mut out, &hs, 3).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("\x1b[7mheading 1"));
        assert!(s.contains("heading 0"));
        assert!(s.contains("heading 2"));
    }
}
