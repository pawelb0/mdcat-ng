// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Keystroke decoder.
//!
//! [`KeyEvent`] in, [`Command`] out. [`Decoder`] tracks pending
//! prefixes (`gg`, `]]`, `m{reg}`, numeric counts, search input).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// One user intent produced by key decoding.
///
/// `Noop` means "unrecognised input"; unknown keys map to this instead
/// of a separate `Option::None` so the event loop stays linear.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[allow(missing_docs)]
pub enum Command {
    Noop,
    Quit,
    Redraw,
    ScrollDown(u16),
    ScrollUp(u16),
    PageDown,
    PageUp,
    HalfPageDown,
    HalfPageUp,
    Home,
    End,
    /// Jump to 1-indexed rendered line `n` (numeric prefix + `G`).
    GotoLine(usize),
    /// Enter search-input mode; subsequent keys build a pattern.
    BeginSearch(SearchDirection),
    SearchChar(char),
    SearchBackspace,
    SearchCommit,
    SearchCancel,
    SearchNext,
    SearchPrev,
    ClearHighlights,
    NextHeading,
    PrevHeading,
    ToggleToc,
    /// Activate the current TOC entry (`Enter` while the modal is open).
    TocActivate,
    /// Save the current viewport top under bookmark `letter` (`m{a-z}`).
    SetBookmark(char),
    /// Jump to bookmark `letter` (`'{a-z}`).
    JumpBookmark(char),
    ToggleLineNumbers,
}

/// Direction selected by `/` vs `?`.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[allow(missing_docs)]
pub enum SearchDirection {
    Forward,
    Backward,
}

/// Stateful decoder: absorbs prefix keys and emits commands.
///
/// Each field tracks one kind of pending prefix (`gg`, `]]`, numeric
/// count, `m`/`'` bookmark register, or active `/` / `?` input). The
/// next keystroke either completes a digraph or cancels and dispatches.
#[derive(Debug, Default)]
pub struct Decoder {
    count: u32,
    pending_g: bool,
    pending_bracket: Option<char>,
    pending_mark_set: bool,
    pending_mark_jump: bool,
    searching: bool,
}

impl Decoder {
    /// True while the decoder is collecting a `/` / `?` pattern.
    pub fn in_search(&self) -> bool {
        self.searching
    }

    /// Feed one key event; get back the resulting command.
    pub fn feed(&mut self, key: KeyEvent) -> Command {
        let KeyEvent {
            code, modifiers, ..
        } = key;

        // Ctrl+C always quits, even mid-prefix or mid-search.
        if modifiers.contains(KeyModifiers::CONTROL) && matches!(code, KeyCode::Char('c')) {
            *self = Self::default();
            return Command::Quit;
        }

        if self.searching {
            return self.feed_search(code);
        }

        // Bookmark register capture: the previous key was `m` or `'`,
        // so consume the next ASCII letter as the register name.
        if self.pending_mark_set {
            self.pending_mark_set = false;
            return match code {
                KeyCode::Char(c) if c.is_ascii_alphabetic() => Command::SetBookmark(c),
                _ => Command::Noop,
            };
        }
        if self.pending_mark_jump {
            self.pending_mark_jump = false;
            return match code {
                KeyCode::Char(c) if c.is_ascii_alphabetic() => Command::JumpBookmark(c),
                _ => Command::Noop,
            };
        }

        // Collect a numeric count. Digits keep accumulating; any other
        // key consumes the count and dispatches.
        if let KeyCode::Char(c) = code {
            if c.is_ascii_digit() && modifiers.is_empty() {
                // Lone `0` at the start is Home (less-compatible); digits
                // after an existing count extend it.
                if self.count == 0 && c == '0' {
                    return Command::Home;
                }
                self.count = self.count.saturating_mul(10) + (c as u32 - b'0' as u32);
                return Command::Noop;
            }
        }

        let count = std::mem::take(&mut self.count);
        let prev_g = std::mem::replace(&mut self.pending_g, false);
        let prev_bracket = self.pending_bracket.take();

        match (code, modifiers) {
            (KeyCode::Char('q'), KeyModifiers::NONE) => Command::Quit,
            (KeyCode::Char('j'), KeyModifiers::NONE) | (KeyCode::Down, _) => {
                Command::ScrollDown(count.max(1).try_into().unwrap_or(1))
            }
            (KeyCode::Char('k'), KeyModifiers::NONE) | (KeyCode::Up, _) => {
                Command::ScrollUp(count.max(1).try_into().unwrap_or(1))
            }
            (KeyCode::Char(' '), KeyModifiers::NONE) | (KeyCode::PageDown, _) => Command::PageDown,
            (KeyCode::Char('f'), KeyModifiers::CONTROL) => Command::PageDown,
            (KeyCode::Char('b'), KeyModifiers::NONE) | (KeyCode::PageUp, _) => Command::PageUp,
            (KeyCode::Char('b'), KeyModifiers::CONTROL) => Command::PageUp,
            (KeyCode::Char('d'), KeyModifiers::CONTROL) => Command::HalfPageDown,
            (KeyCode::Char('u'), KeyModifiers::CONTROL) => Command::HalfPageUp,
            (KeyCode::Char('l'), KeyModifiers::CONTROL) => Command::Redraw,
            (KeyCode::Home, _) => Command::Home,
            (KeyCode::End, _) => Command::End,
            (KeyCode::Char('g'), KeyModifiers::NONE) => {
                if prev_g {
                    Command::Home
                } else {
                    self.pending_g = true;
                    Command::Noop
                }
            }
            (KeyCode::Char('G'), _) => {
                if count > 0 {
                    Command::GotoLine(count as usize)
                } else {
                    Command::End
                }
            }
            (KeyCode::Char('/'), KeyModifiers::NONE) => {
                self.searching = true;
                Command::BeginSearch(SearchDirection::Forward)
            }
            (KeyCode::Char('?'), _) => {
                self.searching = true;
                Command::BeginSearch(SearchDirection::Backward)
            }
            (KeyCode::Char('n'), KeyModifiers::NONE) => Command::SearchNext,
            (KeyCode::Char('N'), _) => Command::SearchPrev,
            (KeyCode::Char(']'), KeyModifiers::NONE) => {
                if prev_bracket == Some(']') {
                    Command::NextHeading
                } else {
                    self.pending_bracket = Some(']');
                    Command::Noop
                }
            }
            (KeyCode::Char('['), KeyModifiers::NONE) => {
                if prev_bracket == Some('[') {
                    Command::PrevHeading
                } else {
                    self.pending_bracket = Some('[');
                    Command::Noop
                }
            }
            (KeyCode::Char('T'), _) => Command::ToggleToc,
            (KeyCode::Char('m'), KeyModifiers::NONE) => {
                self.pending_mark_set = true;
                Command::Noop
            }
            (KeyCode::Char('\''), KeyModifiers::NONE) => {
                self.pending_mark_jump = true;
                Command::Noop
            }
            (KeyCode::Enter, _) => Command::TocActivate,
            (KeyCode::Char('#'), _) => Command::ToggleLineNumbers,
            (KeyCode::Esc, _) => Command::ClearHighlights,
            _ => Command::Noop,
        }
    }

    /// Search-input mode: absorb characters, commit on Enter, cancel on Esc.
    fn feed_search(&mut self, code: KeyCode) -> Command {
        match code {
            KeyCode::Enter => {
                self.searching = false;
                Command::SearchCommit
            }
            KeyCode::Esc => {
                self.searching = false;
                Command::SearchCancel
            }
            KeyCode::Backspace => Command::SearchBackspace,
            KeyCode::Char(c) => Command::SearchChar(c),
            _ => Command::Noop,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn key_mod(c: char, m: KeyModifiers) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), m)
    }

    #[test]
    fn single_keys_map_directly() {
        let mut d = Decoder::default();
        assert_eq!(d.feed(key('j')), Command::ScrollDown(1));
        assert_eq!(d.feed(key('k')), Command::ScrollUp(1));
        assert_eq!(d.feed(key(' ')), Command::PageDown);
        assert_eq!(d.feed(key('b')), Command::PageUp);
        assert_eq!(d.feed(key('G')), Command::End);
        assert_eq!(d.feed(key('q')), Command::Quit);
    }

    #[test]
    fn double_g_is_home() {
        let mut d = Decoder::default();
        assert_eq!(d.feed(key('g')), Command::Noop);
        assert_eq!(d.feed(key('g')), Command::Home);
    }

    #[test]
    fn numeric_prefix_drives_goto_line() {
        let mut d = Decoder::default();
        for c in "42".chars() {
            assert_eq!(d.feed(key(c)), Command::Noop);
        }
        assert_eq!(d.feed(key('G')), Command::GotoLine(42));
    }

    #[test]
    fn numeric_prefix_multiplies_scroll() {
        let mut d = Decoder::default();
        assert_eq!(d.feed(key('5')), Command::Noop);
        assert_eq!(d.feed(key('j')), Command::ScrollDown(5));
    }

    #[test]
    fn ctrl_c_quits_mid_prefix() {
        let mut d = Decoder::default();
        assert_eq!(d.feed(key('9')), Command::Noop);
        assert_eq!(d.feed(key_mod('c', KeyModifiers::CONTROL)), Command::Quit);
        // Count was cleared, so a fresh `G` is End not GotoLine(9).
        assert_eq!(d.feed(key('G')), Command::End);
    }

    #[test]
    fn lone_zero_goes_to_first_column() {
        let mut d = Decoder::default();
        assert_eq!(d.feed(key('0')), Command::Home);
    }

    #[test]
    fn unknown_key_is_noop_not_error() {
        let mut d = Decoder::default();
        assert_eq!(d.feed(key('x')), Command::Noop);
    }

    #[test]
    fn double_bracket_emits_heading_jumps() {
        let mut d = Decoder::default();
        assert_eq!(d.feed(key(']')), Command::Noop);
        assert_eq!(d.feed(key(']')), Command::NextHeading);
        assert_eq!(d.feed(key('[')), Command::Noop);
        assert_eq!(d.feed(key('[')), Command::PrevHeading);
    }

    #[test]
    fn mismatched_bracket_cancels_pending() {
        let mut d = Decoder::default();
        assert_eq!(d.feed(key(']')), Command::Noop);
        // A non-bracket key consumes the pending state and dispatches.
        assert_eq!(d.feed(key('j')), Command::ScrollDown(1));
        // Second `]` alone doesn't fire — the pending flag cleared above.
        assert_eq!(d.feed(key(']')), Command::Noop);
    }

    #[test]
    fn capital_t_toggles_toc() {
        let mut d = Decoder::default();
        assert_eq!(d.feed(key('T')), Command::ToggleToc);
    }

    #[test]
    fn m_letter_sets_bookmark_register() {
        let mut d = Decoder::default();
        assert_eq!(d.feed(key('m')), Command::Noop);
        assert_eq!(d.feed(key('a')), Command::SetBookmark('a'));
    }

    #[test]
    fn apostrophe_letter_jumps_to_bookmark() {
        let mut d = Decoder::default();
        assert_eq!(d.feed(key('\'')), Command::Noop);
        assert_eq!(d.feed(key('q')), Command::JumpBookmark('q'));
    }

    #[test]
    fn hash_toggles_line_numbers() {
        let mut d = Decoder::default();
        assert_eq!(d.feed(key('#')), Command::ToggleLineNumbers);
    }

    #[test]
    fn bookmark_register_rejects_non_letter() {
        let mut d = Decoder::default();
        assert_eq!(d.feed(key('m')), Command::Noop);
        assert_eq!(d.feed(key('1')), Command::Noop);
        // Pending flag cleared: a fresh `j` decodes normally.
        assert_eq!(d.feed(key('j')), Command::ScrollDown(1));
    }
}
