// Copyright 2026 Pawel Boguszewski
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Search-highlight writer for one line of styled output.
//!
//! This file contains:
//!
//! - [`Highlight`] — the per-line set of byte ranges to highlight:
//!   one optional "current" match plus zero or more "other" matches.
//! - [`write_line`] — writes a single line of styled bytes to a
//!   `Write`, wrapping each highlighted range in SGR. After each
//!   highlight it re-emits whatever SGR state was active at the
//!   match start so bold / italic / link underline survive the
//!   highlight reset.
//! - SGR constants for the match / current-match / highlight-off
//!   escape sequences.
//!
//! How it fits: [`view`](super::view) owns the draw loop.
//! For each visible line it builds a [`Highlight`] (by filtering
//! the full match list from [`search`](super::search) down to the
//! ranges that intersect this line) and calls [`write_line`].
//! Match + current styling are distinct so `n` / `N` cycle
//! visually obvious — the active match gets a yellow background,
//! the rest reverse video.

use std::io::{self, Write};
use std::ops::Range;

/// SGR for any non-current match. ANSI reverse video — terminal-portable.
pub const MATCH_SGR: &[u8] = b"\x1b[7m";
/// SGR for the currently-selected match (yellow background, black text).
pub const CURRENT_MATCH_SGR: &[u8] = b"\x1b[43;30m";
/// SGR reset after the highlight.
pub const HIGHLIGHT_OFF: &[u8] = b"\x1b[27;39;49m";

/// Ranges within a styled line that must be highlighted.
#[derive(Debug, Clone, Default)]
pub struct Highlight {
    /// Byte range within the line to emphasise as the current match.
    pub current: Option<Range<usize>>,
    /// Byte ranges within the line for all other matches, sorted ascending.
    pub others: Vec<Range<usize>>,
}

impl Highlight {
    /// Empty highlight (line renders verbatim).
    pub fn none() -> Self {
        Self::default()
    }

    /// True when no ranges are set.
    pub fn is_empty(&self) -> bool {
        self.current.is_none() && self.others.is_empty()
    }
}

/// Emit `line` to `out`, wrapping each highlighted byte range in SGR.
///
/// Ranges refer to byte offsets within `line`. Overlapping ranges are
/// merged on-the-fly (current wins over other). Escape sequences in
/// `line` are preserved verbatim; the highlight only splices in extra
/// SGR around the match bytes and restores the SGR state afterward.
pub fn write_line<W: Write>(out: &mut W, line: &[u8], hl: &Highlight) -> io::Result<()> {
    if hl.is_empty() {
        return out.write_all(line);
    }

    let events = merge_events(line.len(), hl);
    let mut cursor = 0;
    for HighlightRun { range, is_current } in events {
        // Bytes before this run render verbatim.
        if range.start > cursor {
            out.write_all(&line[cursor..range.start])?;
        }
        // Emit the match with wrapping SGR, then restore the inherent line
        // style by re-emitting whatever SGR was active at match start.
        let style_at_start = inherent_style(&line[..range.start]);
        out.write_all(if is_current {
            CURRENT_MATCH_SGR
        } else {
            MATCH_SGR
        })?;
        out.write_all(&line[range.clone()])?;
        out.write_all(HIGHLIGHT_OFF)?;
        if !style_at_start.is_empty() {
            out.write_all(&style_at_start)?;
        }
        cursor = range.end;
    }
    if cursor < line.len() {
        out.write_all(&line[cursor..])?;
    }
    Ok(())
}

#[derive(Debug)]
struct HighlightRun {
    range: Range<usize>,
    is_current: bool,
}

/// Merge current + other highlight ranges into a sorted, non-overlapping
/// sequence. Current wins when ranges overlap.
fn merge_events(len: usize, hl: &Highlight) -> Vec<HighlightRun> {
    let mut events: Vec<HighlightRun> = Vec::with_capacity(hl.others.len() + 1);
    for r in &hl.others {
        if r.start < len {
            events.push(HighlightRun {
                range: r.start..r.end.min(len),
                is_current: false,
            });
        }
    }
    if let Some(r) = hl.current.as_ref() {
        if r.start < len {
            events.push(HighlightRun {
                range: r.start..r.end.min(len),
                is_current: true,
            });
        }
    }
    events.sort_by_key(|e| e.range.start);

    // Collapse overlaps: if two runs touch, the later `is_current` wins
    // (we re-sort by start, not by kind, so "later" is positional).
    let mut merged: Vec<HighlightRun> = Vec::with_capacity(events.len());
    for run in events {
        match merged.last_mut() {
            Some(prev) if prev.range.end > run.range.start => {
                prev.range.end = prev.range.end.max(run.range.end);
                prev.is_current |= run.is_current;
            }
            _ => merged.push(run),
        }
    }
    merged
}

/// Concatenation of every SGR sequence that appears in `prefix`.
///
/// When the highlight clears its inverse / bg / fg, we need to restore
/// whatever the line was already saying. A lossless rebuild would parse
/// and re-apply parameters; we take a simpler approach that works for
/// the SGR sequences `push_tty` emits: replay every SGR byte-for-byte in
/// order. Any redundancy the terminal will collapse on its own.
fn inherent_style(prefix: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < prefix.len() {
        if prefix[i] == 0x1b && prefix.get(i + 1) == Some(&b'[') {
            let mut j = i + 2;
            while prefix.get(j).is_some_and(|&b| !(0x40..=0x7e).contains(&b)) {
                j += 1;
            }
            if let Some(&final_byte) = prefix.get(j) {
                if final_byte == b'm' {
                    out.extend_from_slice(&prefix[i..=j]);
                }
                i = j + 1;
                continue;
            }
            break;
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hl(current: Option<Range<usize>>, others: &[Range<usize>]) -> Highlight {
        Highlight {
            current,
            others: others.to_vec(),
        }
    }

    #[test]
    fn no_highlight_passes_through_verbatim() {
        let mut out = Vec::new();
        write_line(&mut out, b"hello world\n", &Highlight::none()).unwrap();
        assert_eq!(out, b"hello world\n");
    }

    #[test]
    fn highlights_single_match() {
        let mut out = Vec::new();
        write_line(&mut out, b"hello world", &hl(Some(6..11), &[])).unwrap();
        // Expect: "hello " + CURRENT SGR + "world" + OFF.
        assert_eq!(out, b"hello \x1b[43;30mworld\x1b[27;39;49m".as_slice());
    }

    #[test]
    fn highlights_multiple_other_matches_in_order() {
        let mut out = Vec::new();
        write_line(&mut out, b"foo bar baz\n", &hl(None, &[0..3, 8..11])).unwrap();
        assert_eq!(
            out,
            b"\x1b[7mfoo\x1b[27;39;49m bar \x1b[7mbaz\x1b[27;39;49m\n".as_slice()
        );
    }

    #[test]
    fn current_wins_on_overlap() {
        let mut out = Vec::new();
        // Lint wants `[2; 8]` for an array of eights; we want a single
        // overlapping range, which is exactly what `[2..8]` encodes.
        #[allow(clippy::single_range_in_vec_init)]
        let others = [2..8];
        write_line(&mut out, b"aaaaabbbcc", &hl(Some(0..5), &others)).unwrap();
        // Merged range is 0..8, flagged current.
        assert_eq!(out, b"\x1b[43;30maaaaabbb\x1b[27;39;49mcc".as_slice());
    }

    #[test]
    fn restores_inherent_sgr_after_match() {
        // Line: "foo <SGR-bold>bar<SGR-reset> baz", highlight "bar".
        // "foo " = 0..4, "\x1b[1m" = 4..8, "bar" = 8..11.
        let line = b"foo \x1b[1mbar\x1b[0m baz";
        let mut out = Vec::new();
        write_line(&mut out, line, &hl(Some(8..11), &[])).unwrap();
        let s = String::from_utf8_lossy(&out);
        // After highlight clear, bold SGR should be re-emitted so any
        // subsequent text keeps the line's inherent style.
        assert!(s.contains("\x1b[43;30mbar\x1b[27;39;49m\x1b[1m"));
    }

    #[test]
    fn inherent_style_collects_every_sgr_in_prefix() {
        let prefix = b"\x1b[1m\x1b[34mhello ";
        let collected = inherent_style(prefix);
        assert_eq!(collected, b"\x1b[1m\x1b[34m".as_slice());
    }

    #[test]
    fn inherent_style_ignores_non_sgr_escapes() {
        // OSC 8 link start should be ignored (not an `m`-terminated CSI).
        let prefix = b"\x1b]8;;http://x\x1b\\text";
        let collected = inherent_style(prefix);
        assert!(collected.is_empty());
    }
}
