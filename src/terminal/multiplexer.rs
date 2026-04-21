// Copyright 2025 mdcat contributors
// Copyright 2026 Pawel Boguszewski

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Terminal multiplexer detection and escape-sequence passthrough.
//!
//! Terminal multiplexers (tmux, GNU screen) sit between the user's terminal
//! and processes running inside the multiplexer session, and by default
//! intercept escape sequences they don't recognise. Image protocols like
//! Kitty, iTerm2, and Sixel therefore don't reach the real terminal.
//!
//! The workaround is *DCS passthrough*: wrap the offending bytes in a
//! multiplexer-specific escape that asks the multiplexer to forward the
//! payload verbatim.
//!
//! * **tmux**: `ESC P tmux; <escaped payload> ESC \`. Each `ESC` byte
//!   inside the payload must be doubled. Requires
//!   `set -g allow-passthrough on` in the user's tmux config (on by
//!   default from tmux 3.3+ only for some operations, so it's worth
//!   documenting).
//!
//! * **screen**: `ESC P <payload> ESC \`. Payloads longer than 768 bytes
//!   must be split across multiple DCS envelopes, but in practice our
//!   image writers emit chunks well under that.

use std::io::{Result, Write};

/// A detected terminal multiplexer.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Default)]
pub enum Multiplexer {
    /// No multiplexer in use.
    #[default]
    None,
    /// GNU screen (`$STY` set).
    Screen,
    /// tmux (`$TMUX` set).
    Tmux,
}

impl Multiplexer {
    /// Detect the multiplexer by examining `$TMUX` and `$STY`.
    ///
    /// `$TMUX` is always set inside a tmux session; `$STY` is always set
    /// inside a GNU screen session. Both survive `ssh` and `sudo` in most
    /// configurations.
    #[must_use]
    pub fn detect() -> Self {
        if std::env::var_os("TMUX").is_some() {
            Self::Tmux
        } else if std::env::var_os("STY").is_some() {
            Self::Screen
        } else {
            Self::None
        }
    }

    /// Write `payload` to `writer`, wrapping it in DCS passthrough for this
    /// multiplexer if needed.
    ///
    /// `payload` is assumed to be a single self-contained escape-sequence
    /// block (one image render). Callers should buffer a full image into a
    /// `Vec<u8>` before calling this so the wrapper brackets the entire
    /// payload rather than each individual chunk.
    pub fn write_passthrough<W: Write>(self, writer: &mut W, payload: &[u8]) -> Result<()> {
        match self {
            Self::None => writer.write_all(payload),
            Self::Tmux => write_tmux_passthrough(writer, payload),
            Self::Screen => write_screen_passthrough(writer, payload),
        }
    }
}

fn write_tmux_passthrough<W: Write>(writer: &mut W, payload: &[u8]) -> Result<()> {
    writer.write_all(b"\x1bPtmux;")?;
    // Double every ESC byte inside the payload — tmux uses `ESC ESC` to
    // represent a single ESC, and terminates the passthrough on bare
    // `ESC \` (ST).
    let mut start = 0;
    for (i, &byte) in payload.iter().enumerate() {
        if byte == 0x1b {
            writer.write_all(&payload[start..i])?;
            writer.write_all(b"\x1b\x1b")?;
            start = i + 1;
        }
    }
    writer.write_all(&payload[start..])?;
    writer.write_all(b"\x1b\\")?;
    Ok(())
}

fn write_screen_passthrough<W: Write>(writer: &mut W, payload: &[u8]) -> Result<()> {
    // screen's DCS is simpler: no doubling, just wrap. Payloads are split
    // on existing `ESC \` (ST) boundaries into separate DCS envelopes so
    // screen doesn't prematurely close the passthrough.
    let mut remaining = payload;
    loop {
        writer.write_all(b"\x1bP")?;
        if let Some(pos) = find_st(remaining) {
            // Include the ST in this envelope by ending it exactly where
            // the embedded ST sits. Advance past it for the next loop.
            writer.write_all(&remaining[..pos])?;
            writer.write_all(b"\x1b\\")?;
            remaining = &remaining[pos + 2..];
            if remaining.is_empty() {
                return Ok(());
            }
        } else {
            writer.write_all(remaining)?;
            writer.write_all(b"\x1b\\")?;
            return Ok(());
        }
    }
}

fn find_st(bytes: &[u8]) -> Option<usize> {
    bytes.windows(2).position(|w| w == b"\x1b\\")
}

#[cfg(test)]
mod tests {
    use super::*;
    use temp_env::with_vars;

    #[test]
    fn detect_none() {
        with_vars(vec![("TMUX", None::<&str>), ("STY", None)], || {
            assert_eq!(Multiplexer::detect(), Multiplexer::None);
        });
    }

    #[test]
    fn detect_tmux() {
        with_vars(
            vec![
                ("TMUX", Some("/tmp/tmux-1000/default,1234,0")),
                ("STY", None),
            ],
            || {
                assert_eq!(Multiplexer::detect(), Multiplexer::Tmux);
            },
        );
    }

    #[test]
    fn detect_screen() {
        with_vars(
            vec![("TMUX", None::<&str>), ("STY", Some("4321.pts-0.host"))],
            || {
                assert_eq!(Multiplexer::detect(), Multiplexer::Screen);
            },
        );
    }

    #[test]
    fn tmux_beats_screen_when_both_set() {
        // Nested screen-inside-tmux is rare and ambiguous; we pick tmux.
        with_vars(vec![("TMUX", Some("x")), ("STY", Some("y"))], || {
            assert_eq!(Multiplexer::detect(), Multiplexer::Tmux);
        });
    }

    #[test]
    fn no_multiplexer_writes_payload_verbatim() {
        let mut out = Vec::new();
        Multiplexer::None
            .write_passthrough(&mut out, b"\x1b[31mhi\x1b[0m")
            .unwrap();
        assert_eq!(out, b"\x1b[31mhi\x1b[0m");
    }

    #[test]
    fn tmux_doubles_escapes_and_wraps() {
        let mut out = Vec::new();
        Multiplexer::Tmux
            .write_passthrough(&mut out, b"\x1b]1337;hi\x1b\\")
            .unwrap();
        // Prefix `ESC P tmux;`, then the payload with both ESC bytes
        // doubled, then trailing ST (`ESC \`).
        assert_eq!(
            out,
            b"\x1bPtmux;\x1b\x1b]1337;hi\x1b\x1b\\\x1b\\".as_slice()
        );
    }

    #[test]
    fn screen_wraps_and_splits_on_st() {
        let mut out = Vec::new();
        Multiplexer::Screen
            .write_passthrough(&mut out, b"first\x1b\\second")
            .unwrap();
        // Expect: ESC P "first" ESC \ ESC P "second" ESC \
        assert_eq!(out, b"\x1bPfirst\x1b\\\x1bPsecond\x1b\\".as_slice());
    }
}
