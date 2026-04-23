// Copyright 2025 mdcat contributors

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Active terminal capability probing.
//!
//! When env-var based detection falls back to `TerminalProgram::Ansi`
//! we can still learn something about the terminal by asking it directly.
//! [`probe_da1`] sends the Primary Device Attributes query (`ESC [ c`) and
//! parses the response, in particular looking for the `;4;` parameter that
//! signals Sixel support.
//!
//! Probing is Unix-only. On Windows and when `/dev/tty` is unavailable the
//! functions return `None` so callers can fall back to env-var detection.

#[cfg(unix)]
use tracing::{event, Level};

/// Capabilities the terminal self-reported in response to a DA1 query.
#[derive(Debug, Default, Copy, Clone, Eq, PartialEq)]
pub struct DeviceAttributes {
    /// `;4;` appeared in the DA1 response, meaning the terminal advertises
    /// Sixel graphics.
    pub sixel: bool,
}

/// Probe the terminal for DA1 capabilities, or return `None` if probing is
/// not possible (not a TTY, `/dev/tty` unavailable, Windows, etc.).
///
/// `timeout` caps how long we block reading the response. 200–500 ms is a
/// sensible range — long enough for a terminal to answer, short enough that
/// unresponsive terminals don't stall startup noticeably.
#[cfg(unix)]
pub fn probe_da1(timeout: std::time::Duration) -> Option<DeviceAttributes> {
    use std::fs::OpenOptions;
    use std::os::fd::AsFd;

    use rustix::termios::{tcgetattr, tcsetattr, LocalModes, OptionalActions, SpecialCodeIndex};

    let mut tty = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .ok()?;

    let original = tcgetattr(tty.as_fd()).ok()?;
    let mut raw = original.clone();

    // Disable canonical mode + echo so the terminal's response reaches us
    // byte-by-byte without being interpreted by the line discipline.
    raw.local_modes
        .remove(LocalModes::ICANON | LocalModes::ECHO | LocalModes::ECHONL);

    // VMIN=0 + VTIME = deciseconds → read() returns after up to VTIME
    // 10ths of a second, even if no byte arrives.
    let deciseconds = timeout.as_millis().div_ceil(100).min(255) as u8;
    raw.special_codes[SpecialCodeIndex::VMIN] = 0;
    raw.special_codes[SpecialCodeIndex::VTIME] = deciseconds;

    tcsetattr(tty.as_fd(), OptionalActions::Now, &raw).ok()?;

    // Always restore termios on the way out, even if the DA1 exchange fails.
    let result = perform_da1_exchange(&mut tty);
    let _ = tcsetattr(tty.as_fd(), OptionalActions::Now, &original);

    let response = result?;
    event!(Level::TRACE, ?response, "DA1 response");
    Some(parse_da1(&response))
}

#[cfg(unix)]
fn perform_da1_exchange(tty: &mut std::fs::File) -> Option<Vec<u8>> {
    use std::io::{Read, Write};
    tty.write_all(b"\x1b[c").ok()?;
    tty.flush().ok()?;

    let mut buffer = Vec::with_capacity(64);
    let mut chunk = [0u8; 32];
    // DA1 response ends with `c`. Keep reading chunks until we see one,
    // or the VTIME timeout kicks in and `read` returns 0 bytes.
    loop {
        match tty.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buffer.extend_from_slice(&chunk[..n]);
                if buffer.contains(&b'c') {
                    break;
                }
            }
            Err(_) => break,
        }
    }

    if buffer.is_empty() {
        None
    } else {
        Some(buffer)
    }
}

/// Windows stub — active probing over `/dev/tty` isn't available. Always
/// returns `None` so callers fall back to env-var detection.
#[cfg(not(unix))]
pub fn probe_da1(_timeout: std::time::Duration) -> Option<DeviceAttributes> {
    None
}

#[cfg(unix)]
fn parse_da1(response: &[u8]) -> DeviceAttributes {
    // A DA1 response is `ESC [ ? <params> c` where <params> is a
    // semicolon-separated list of parameters. Parameter 4 means Sixel.
    let text = std::str::from_utf8(response).unwrap_or("");
    let Some(start) = text.find("\x1b[?") else {
        return DeviceAttributes::default();
    };
    let remainder = &text[start + 3..];
    let end = remainder.find('c').unwrap_or(remainder.len());
    let params = &remainder[..end];
    let sixel = params.split(';').any(|p| p == "4");
    DeviceAttributes { sixel }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn parses_sixel_capable_response() {
        let resp = b"\x1b[?63;1;2;4;6;9;15c";
        assert!(parse_da1(resp).sixel);
    }

    #[test]
    fn parses_non_sixel_response() {
        let resp = b"\x1b[?61;6;22c";
        assert!(!parse_da1(resp).sixel);
    }

    #[test]
    fn handles_garbage() {
        assert_eq!(parse_da1(b""), DeviceAttributes::default());
        assert_eq!(
            parse_da1(b"not a DA1 response"),
            DeviceAttributes::default()
        );
    }

    #[test]
    fn param_4_inside_other_numbers_is_not_a_match() {
        // `;14;` must NOT be treated as sixel. Split-on-`;` prevents that.
        let resp = b"\x1b[?14;22c";
        assert!(!parse_da1(resp).sixel);
    }
}
