// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! In-memory document for the interactive view.
//!
//! [`RenderedDoc`] holds the styled bytes, an ANSI-stripped plain
//! copy, parallel line-start indexes, and a heading list. Built
//! once from `push_tty` output plus a [`HeadingRecorder`].

use pulldown_cmark::{Event, HeadingLevel, Tag, TagEnd};

use crate::RenderObserver;

/// One heading in the rendered document.
#[derive(Debug, Clone)]
pub struct HeadingEntry {
    /// Markdown heading level, `1` (`#`) through `6` (`######`).
    pub level: u8,
    /// Concatenated inline text of the heading.
    pub text: String,
    /// Byte offset in the plain buffer where the heading line begins.
    pub plain_offset: usize,
}

/// Pre-rendered markdown document with searchable plain copy + line index.
///
/// Built once per input; scrolled and searched interactively.
#[derive(Debug)]
pub struct RenderedDoc {
    /// Styled bytes from `push_tty`, including SGR + OSC 8 sequences.
    pub styled: Vec<u8>,
    /// ANSI-stripped view of `styled`; search corpus.
    pub plain: String,
    /// Byte offsets where each line begins in `plain`. Final entry equals
    /// `plain.len()` so line spans are always `starts[i..i+1]`.
    pub line_starts: Vec<usize>,
    /// Byte offsets where each line begins in `styled`. Parallel to
    /// `line_starts`; identical length.
    pub styled_line_starts: Vec<usize>,
    /// Headings in source order as recorded by [`HeadingRecorder`].
    pub headings: Vec<HeadingEntry>,
}

impl RenderedDoc {
    /// Rendered line count, always at least 1.
    pub fn line_count(&self) -> usize {
        self.line_starts.len().saturating_sub(1).max(1)
    }

    /// Styled bytes for rendered line `n`, including its trailing `\n`.
    ///
    /// Returns an empty slice when `n` is past the end.
    pub fn styled_line(&self, n: usize) -> &[u8] {
        self.styled_line_starts
            .get(n..=n + 1)
            .map_or(&[][..], |range| &self.styled[range[0]..range[1]])
    }

    /// Rendered line index containing `offset` in the plain buffer.
    pub fn line_for_plain_offset(&self, offset: usize) -> usize {
        match self.line_starts.binary_search(&offset) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        }
    }
}

/// Assemble a [`RenderedDoc`] from the styled output + recorded headings.
pub fn build(styled: Vec<u8>, headings: Vec<HeadingEntry>) -> RenderedDoc {
    let plain = strip_ansi(&styled);
    let (line_starts, styled_line_starts) = index_lines(&styled, &plain);
    RenderedDoc {
        styled,
        plain,
        line_starts,
        styled_line_starts,
        headings,
    }
}

/// [`RenderObserver`] that collects heading starts with their text.
///
/// pulldown-cmark emits `Start(Heading)` → inline events → `End(Heading)`.
/// The recorder accumulates text between start and end, then pushes a
/// finalised [`HeadingEntry`] on the closing event.
#[derive(Default)]
pub struct HeadingRecorder {
    pending: Option<PendingHeading>,
    done: Vec<HeadingEntry>,
}

/// Accumulator between `Start(Heading)` and `End(Heading)` events.
struct PendingHeading {
    level: u8,
    plain_offset: u64,
    text: String,
}

impl HeadingRecorder {
    /// Drop the recorder, returning the recorded entries.
    pub fn finish(self) -> Vec<HeadingEntry> {
        self.done
    }
}

impl RenderObserver for HeadingRecorder {
    fn on_event(&mut self, byte_offset: u64, event: &Event<'_>) {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                self.pending = Some(PendingHeading {
                    level: heading_level_to_u8(*level),
                    plain_offset: byte_offset,
                    text: String::new(),
                });
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some(p) = self.pending.take() {
                    self.done.push(HeadingEntry {
                        level: p.level,
                        text: p.text.trim().to_string(),
                        plain_offset: p.plain_offset as usize,
                    });
                }
            }
            Event::Text(s) | Event::Code(s) => {
                if let Some(p) = self.pending.as_mut() {
                    p.text.push_str(s);
                }
            }
            _ => {}
        }
    }
}

/// Map pulldown-cmark's [`HeadingLevel`] to the 1-6 numeric range.
fn heading_level_to_u8(level: HeadingLevel) -> u8 {
    level as u8
}

/// Byte past the end of an ANSI escape sequence starting at `input[start]`.
///
/// Covers the shapes `push_tty` emits: CSI (`ESC [ ... final`), OSC / DCS /
/// APC / PM (`ESC X ... BEL` or `... ESC \`), and unknown two-byte escapes.
/// Returns `start` unchanged when `input[start]` isn't an escape or the
/// sequence is truncated.
fn skip_escape(input: &[u8], start: usize) -> usize {
    let Some(&next) = input.get(start + 1) else {
        return start + 1;
    };
    match next {
        b'[' => {
            // CSI parameters + intermediates, then one final byte 0x40..=0x7e.
            let mut i = start + 2;
            while input.get(i).is_some_and(|&b| !(0x40..=0x7e).contains(&b)) {
                i += 1;
            }
            i + usize::from(i < input.len())
        }
        b']' | b'P' | b'_' | b'^' => {
            // OSC / DCS / APC / PM, terminated by BEL or ST.
            let mut i = start + 2;
            while let Some(&b) = input.get(i) {
                if b == 0x07 {
                    return i + 1;
                }
                if b == 0x1b && input.get(i + 1) == Some(&b'\\') {
                    return i + 2;
                }
                i += 1;
            }
            i
        }
        _ => start + 2,
    }
}

/// Remove ANSI control sequences and return the decoded text.
fn strip_ansi(input: &[u8]) -> String {
    let mut out = Vec::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        if input[i] == 0x1b {
            i = skip_escape(input, i);
        } else {
            out.push(input[i]);
            i += 1;
        }
    }
    // Input is UTF-8 markdown with ASCII-only escapes; stripping keeps it valid.
    String::from_utf8(out)
        .unwrap_or_else(|err| String::from_utf8_lossy(err.as_bytes()).into_owned())
}

/// Line-start offsets in both buffers, parallel by index.
///
/// Final entries are `(plain.len(), styled.len())` so callers can always
/// slice `line_starts[i..=i+1]` for line `i`.
fn index_lines(styled: &[u8], plain: &str) -> (Vec<usize>, Vec<usize>) {
    let mut plain_starts = vec![0];
    let mut styled_starts = vec![0];
    let (mut p, mut s) = (0usize, 0usize);

    while s < styled.len() {
        if styled[s] == 0x1b {
            s = skip_escape(styled, s);
            continue;
        }
        if styled[s] == b'\n' {
            s += 1;
            p += 1;
            plain_starts.push(p);
            styled_starts.push(s);
            continue;
        }
        s += 1;
        p += 1;
    }

    if *plain_starts.last().unwrap() != plain.len() {
        plain_starts.push(plain.len());
        styled_starts.push(styled.len());
    }
    (plain_starts, styled_starts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_sgr_and_preserves_text() {
        let bytes = b"\x1b[1mbold\x1b[0m plain \x1b[34mblue\x1b[0m";
        assert_eq!(strip_ansi(bytes), "bold plain blue");
    }

    #[test]
    fn strips_osc8_hyperlinks() {
        let bytes = b"\x1b]8;;https://example.com\x1b\\label\x1b]8;;\x1b\\";
        assert_eq!(strip_ansi(bytes), "label");
    }

    #[test]
    fn preserves_newlines_for_line_indexing() {
        let bytes = b"line one\nline two\n";
        let s = strip_ansi(bytes);
        assert_eq!(s, "line one\nline two\n");
    }

    #[test]
    fn build_indexes_three_lines() {
        let styled = b"\x1b[1malpha\x1b[0m\nbeta\ngamma\n".to_vec();
        let doc = build(styled, Vec::new());
        assert_eq!(doc.plain, "alpha\nbeta\ngamma\n");
        assert_eq!(doc.line_count(), 3);
        // Plain line starts: 0, 6 ("alpha\n"), 11 ("beta\n"), 17 (sentinel).
        assert_eq!(doc.line_starts, vec![0, 6, 11, 17]);
        // Styled line 1 is "beta\n" — no escapes on that line.
        assert_eq!(doc.styled_line(1), b"beta\n");
        // Styled line 0 includes the SGR codes.
        assert_eq!(doc.styled_line(0), b"\x1b[1malpha\x1b[0m\n");
    }

    #[test]
    fn line_lookup_round_trips() {
        // "one\n" occupies offsets 0..=3, "two\n" occupies 4..=7, "three\n"
        // occupies 8..=13. line_starts = [0, 4, 8, 14].
        let styled = b"one\ntwo\nthree\n".to_vec();
        let doc = build(styled, Vec::new());
        assert_eq!(doc.line_for_plain_offset(0), 0);
        assert_eq!(doc.line_for_plain_offset(3), 0); // newline still on line 0
        assert_eq!(doc.line_for_plain_offset(4), 1); // start of "two"
        assert_eq!(doc.line_for_plain_offset(8), 2); // start of "three"
        assert_eq!(doc.line_for_plain_offset(100), 3); // past end clamps to sentinel
    }
}
