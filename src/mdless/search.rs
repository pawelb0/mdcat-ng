// Copyright 2026 Pawel Boguszewski
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Pattern search over the document.
//!
//! This file contains:
//!
//! - [`SearchState`] — the compiled query, the full list of matches,
//!   and a cursor that `n` / `N` advance through. Built once per
//!   committed query and dropped when the user clears highlights
//!   with `Esc`.
//! - [`Match`] — one hit, carrying both the plain-buffer byte range
//!   (what the regex matched) and the styled-buffer byte range
//!   (what [`highlight`](super::highlight) wraps in SGR).
//! - [`CaseMode`] and [`Direction`] — the case-sensitivity policy
//!   and the forward/backward cycle direction.
//! - [`SearchState::compile`] — build the state from a pattern + a
//!   [`RenderedDoc`]. Runs the regex
//!   against the ANSI-stripped plain buffer, then translates every
//!   match range into the parallel styled range using the
//!   line-start indexes from `buffer.rs`.
//!
//! How it fits: the dispatch layer in `mdless::mod` owns an
//! `Option<SearchState>`. `/pattern` or `--search pattern`
//! replaces it via `compile`; `n` / `N` call `step`;
//! [`view`](super::view) reads the match list each frame and hands
//! it to [`highlight`](super::highlight) for inline rendering.
//! Escape sequences are skipped during the plain-to-styled mapping
//! so a match starting just after an SGR boundary lands on the
//! visible byte, not on the introducer.

use std::ops::Range;

use anyhow::{anyhow, Result};
use regex::Regex;

use super::buffer::RenderedDoc;

/// One match inside the rendered document.
#[derive(Debug, Clone)]
pub struct Match {
    /// Byte range within `RenderedDoc::plain`.
    pub plain: Range<usize>,
    /// Byte range within `RenderedDoc::styled`, mapped from `plain`.
    pub styled: Range<usize>,
    /// Rendered line containing the first byte of the match.
    pub line: usize,
}

/// Case-sensitivity policy for a compiled search.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CaseMode {
    /// Case-insensitive unless the pattern contains an uppercase ASCII
    /// letter.
    Smart,
    /// Always case-sensitive.
    Sensitive,
    /// Always case-insensitive.
    Insensitive,
}

/// Search direction for `next` / `previous`.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Direction {
    /// Advance to the next match after the cursor.
    Forward,
    /// Retreat to the previous match before the cursor.
    Backward,
}

/// Compiled query + cached match list + cycle cursor.
#[derive(Debug)]
pub struct SearchState {
    matches: Vec<Match>,
    cursor: Option<usize>,
}

impl SearchState {
    /// Compile `pattern` against `doc`, returning the initial state.
    ///
    /// `pattern` is a literal by default; callers pass `regex = true` to
    /// interpret it as a regular expression. Invalid regex patterns
    /// surface as `Err`.
    pub fn compile(doc: &RenderedDoc, pattern: &str, regex: bool, case: CaseMode) -> Result<Self> {
        let insensitive = match case {
            CaseMode::Sensitive => false,
            CaseMode::Insensitive => true,
            CaseMode::Smart => !pattern.chars().any(|c| c.is_ascii_uppercase()),
        };

        // Build the compiled regex. Literal mode escapes metacharacters.
        let body = if regex {
            pattern.to_string()
        } else {
            regex::escape(pattern)
        };
        let compiled = regex::RegexBuilder::new(&body)
            .case_insensitive(insensitive)
            .build()
            .map_err(|e| anyhow!("invalid search pattern: {e}"))?;

        let matches = scan(doc, &compiled);
        let cursor = if matches.is_empty() { None } else { Some(0) };
        Ok(Self { matches, cursor })
    }

    /// Number of matches found.
    pub fn len(&self) -> usize {
        self.matches.len()
    }

    /// True when the pattern didn't match anywhere.
    pub fn is_empty(&self) -> bool {
        self.matches.is_empty()
    }

    /// Currently-selected match, if any.
    pub fn current(&self) -> Option<&Match> {
        self.cursor.and_then(|i| self.matches.get(i))
    }

    /// All matches, stable source order.
    pub fn all(&self) -> &[Match] {
        &self.matches
    }

    /// Advance or retreat the cursor. Returns the newly-selected match,
    /// and `true` when the cursor wrapped around the buffer end.
    pub fn step(&mut self, dir: Direction) -> Option<(&Match, bool)> {
        let n = self.matches.len();
        if n == 0 {
            return None;
        }
        let (next, wrapped) = match (self.cursor, dir) {
            (None, Direction::Forward) => (0, false),
            (None, Direction::Backward) => (n - 1, false),
            (Some(i), Direction::Forward) => {
                if i + 1 >= n {
                    (0, true)
                } else {
                    (i + 1, false)
                }
            }
            (Some(i), Direction::Backward) => {
                if i == 0 {
                    (n - 1, true)
                } else {
                    (i - 1, false)
                }
            }
        };
        self.cursor = Some(next);
        self.matches.get(next).map(|m| (m, wrapped))
    }
}

fn scan(doc: &RenderedDoc, re: &Regex) -> Vec<Match> {
    re.find_iter(&doc.plain)
        .map(|m| {
            let plain = m.range();
            // Start must land on a visible byte: if the mapping leaves
            // us on an SGR introducer we consume it so the highlight
            // wraps "bar" rather than "\x1b[1mbar".
            let mut styled_start = plain_to_styled(doc, plain.start);
            while doc.styled.get(styled_start) == Some(&0x1b) {
                styled_start = skip_escape(&doc.styled, styled_start);
            }
            let styled_end = plain_to_styled(doc, plain.end);
            let line = doc.line_for_plain_offset(plain.start);
            Match {
                plain,
                styled: styled_start..styled_end,
                line,
            }
        })
        .collect()
}

/// Map a plain-buffer byte offset to the matching offset in the styled
/// buffer. Uses the parallel line starts to pin the line, then walks the
/// styled bytes skipping escape sequences until the plain-relative offset
/// is reached.
fn plain_to_styled(doc: &RenderedDoc, plain_offset: usize) -> usize {
    let line = doc.line_for_plain_offset(plain_offset);
    let plain_base = doc.line_starts[line];
    let styled_base = doc.styled_line_starts[line];
    let target = plain_offset - plain_base;

    let mut plain_cursor = 0;
    let mut styled_cursor = styled_base;
    while plain_cursor < target && styled_cursor < doc.styled.len() {
        let b = doc.styled[styled_cursor];
        if b == 0x1b {
            styled_cursor = skip_escape(&doc.styled, styled_cursor);
            continue;
        }
        plain_cursor += 1;
        styled_cursor += 1;
    }
    styled_cursor
}

/// Duplicate of `buffer::skip_escape` — factored here so the search pass
/// stays self-contained. If both pagers ever share an ANSI parser we can
/// pull this up into a common module.
fn skip_escape(input: &[u8], start: usize) -> usize {
    let Some(&next) = input.get(start + 1) else {
        return start + 1;
    };
    match next {
        b'[' => {
            let mut i = start + 2;
            while input.get(i).is_some_and(|&b| !(0x40..=0x7e).contains(&b)) {
                i += 1;
            }
            i + usize::from(i < input.len())
        }
        b']' | b'P' | b'_' | b'^' => {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mdless::buffer::build;

    fn doc(s: &str) -> RenderedDoc {
        build(s.as_bytes().to_vec(), Vec::new())
    }

    #[test]
    fn literal_pattern_finds_all_occurrences() {
        let d = doc("foo bar foo\nbaz foo\n");
        let s = SearchState::compile(&d, "foo", false, CaseMode::Sensitive).unwrap();
        assert_eq!(s.len(), 3);
        assert_eq!(s.all()[0].plain, 0..3);
        assert_eq!(s.all()[1].plain, 8..11);
        assert_eq!(s.all()[2].plain, 16..19);
    }

    #[test]
    fn smart_case_is_insensitive_until_uppercase_in_pattern() {
        let d = doc("FOO foo Foo\n");
        let lower = SearchState::compile(&d, "foo", false, CaseMode::Smart).unwrap();
        assert_eq!(lower.len(), 3);
        let mixed = SearchState::compile(&d, "Foo", false, CaseMode::Smart).unwrap();
        assert_eq!(mixed.len(), 1);
    }

    #[test]
    fn regex_flag_enables_metacharacters() {
        let d = doc("foo123 bar45 baz6\n");
        let literal = SearchState::compile(&d, r"\d+", false, CaseMode::Sensitive).unwrap();
        assert_eq!(literal.len(), 0);
        let re = SearchState::compile(&d, r"\d+", true, CaseMode::Sensitive).unwrap();
        assert_eq!(re.len(), 3);
    }

    #[test]
    fn invalid_regex_surfaces_as_err() {
        let d = doc("x\n");
        let err = SearchState::compile(&d, "[", true, CaseMode::Sensitive).unwrap_err();
        assert!(err.to_string().contains("invalid search pattern"));
    }

    #[test]
    fn step_cycles_and_reports_wrap() {
        let d = doc("foo foo foo\n");
        let mut s = SearchState::compile(&d, "foo", false, CaseMode::Sensitive).unwrap();
        // Fresh cursor points at index 0 after compile.
        assert_eq!(s.current().unwrap().plain, 0..3);
        assert_eq!(s.step(Direction::Forward).unwrap().0.plain, 4..7);
        assert_eq!(s.step(Direction::Forward).unwrap().0.plain, 8..11);
        // Third Forward wraps.
        let (m, wrapped) = s.step(Direction::Forward).unwrap();
        assert!(wrapped);
        assert_eq!(m.plain, 0..3);
        // Backward from head wraps.
        let (m, wrapped) = s.step(Direction::Backward).unwrap();
        assert!(wrapped);
        assert_eq!(m.plain, 8..11);
    }

    #[test]
    fn styled_range_skips_escape_sequences() {
        // "foo \x1b[1mbar\x1b[0m foo" — plain is "foo bar foo".
        let styled = b"foo \x1b[1mbar\x1b[0m foo\n".to_vec();
        let d = build(styled, Vec::new());
        let s = SearchState::compile(&d, "bar", false, CaseMode::Sensitive).unwrap();
        let m = &s.all()[0];
        assert_eq!(m.plain, 4..7);
        // Styled range should include the SGR-on prefix but not the
        // SGR-off suffix (escapes before the match are consumed in the
        // mapping, escapes at the end are handled by highlighting).
        assert_eq!(&d.styled[m.styled.clone()], b"bar");
    }

    #[test]
    fn no_matches_yields_none_cursor() {
        let d = doc("nothing here\n");
        let mut s = SearchState::compile(&d, "missing", false, CaseMode::Sensitive).unwrap();
        assert!(s.is_empty());
        assert!(s.current().is_none());
        assert!(s.step(Direction::Forward).is_none());
    }
}
