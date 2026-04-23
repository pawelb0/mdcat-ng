// Copyright 2018-2020 Sebastian Wiesner <sebastian@swsnr.de>

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Detect the terminal application mdcat is running on.

use crate::terminal::capabilities::iterm2::ITerm2Protocol;
use crate::terminal::capabilities::*;
use std::fmt::{Display, Formatter};

/// A terminal application.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum TerminalProgram {
    /// A dumb terminal which does not support any formatting.
    Dumb,
    /// A plain ANSI terminal which supports only standard ANSI formatting.
    Ansi,
    /// iTerm2 — <https://www.iterm2.com>.
    ITerm2,
    /// Terminology — <http://terminolo.gy>.
    Terminology,
    /// Kitty — <https://sw.kovidgoyal.net/kitty/>.
    Kitty,
    /// WezTerm — <https://wezfurlong.org/wezterm/>.
    WezTerm,
    /// The built-in terminal in VSCode (since 1.80, iTerm2 image protocol).
    VSCode,
    /// Ghostty — <https://mitchellh.com/ghostty>.
    Ghostty,
    /// Alacritty — ANSI + OSC 8 hyperlinks.
    Alacritty,
    /// Foot, Wayland terminal — ANSI + OSC 8 + Sixel (when the feature lands).
    Foot,
    /// KDE Konsole — ANSI + OSC 8.
    Konsole,
    /// Apple's Terminal.app — ANSI only on older macOS; OSC 8 on macOS 15+.
    AppleTerminal,
    /// Warp — <https://warp.dev>.
    Warp,
    /// Rio — <https://raphamorim.io/rio/>. Supports Kitty graphics.
    Rio,
    /// Hyper (Electron-based) — ANSI only.
    Hyper,
    /// Contour — ANSI + OSC 8 (Sixel when the feature lands).
    Contour,
    /// mlterm — ANSI + Sixel (when the feature lands).
    Mlterm,
    /// Windows Terminal — ANSI + OSC 8 (Sixel since 1.22 beta).
    WindowsTerminal,
}

impl Display for TerminalProgram {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let name = match *self {
            TerminalProgram::Dumb => "dumb",
            TerminalProgram::Ansi => "ansi",
            TerminalProgram::ITerm2 => "iTerm2",
            TerminalProgram::Terminology => "Terminology",
            TerminalProgram::Kitty => "kitty",
            TerminalProgram::WezTerm => "WezTerm",
            TerminalProgram::VSCode => "vscode",
            TerminalProgram::Ghostty => "ghostty",
            TerminalProgram::Alacritty => "Alacritty",
            TerminalProgram::Foot => "foot",
            TerminalProgram::Konsole => "Konsole",
            TerminalProgram::AppleTerminal => "Apple Terminal",
            TerminalProgram::Warp => "Warp",
            TerminalProgram::Rio => "Rio",
            TerminalProgram::Hyper => "Hyper",
            TerminalProgram::Contour => "Contour",
            TerminalProgram::Mlterm => "mlterm",
            TerminalProgram::WindowsTerminal => "Windows Terminal",
        };
        write!(f, "{name}")
    }
}

/// Extract major and minor version from `$TERM_PROGRAM_VERSION`.
///
/// Return `None` if the variable doesn't exist, or has invalid contents, such as
/// non-numeric parts, insufficient parts for a major.minor version, etc.
fn get_term_program_major_minor_version() -> Option<(u16, u16)> {
    let value = std::env::var("TERM_PROGRAM_VERSION").ok()?;
    let mut parts = value.split('.').take(2);
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    Some((major, minor))
}

impl TerminalProgram {
    fn detect_term() -> Option<Self> {
        let term = std::env::var("TERM").ok();
        let t = term.as_deref()?;
        match t {
            "wezterm" => Some(Self::WezTerm),
            "xterm-kitty" => Some(Self::Kitty),
            "xterm-ghostty" => Some(Self::Ghostty),
            "alacritty" | "xterm-alacritty" => Some(Self::Alacritty),
            "foot" | "foot-extra" | "xterm-foot" => Some(Self::Foot),
            "rio" | "xterm-rio" => Some(Self::Rio),
            _ if t.starts_with("mlterm") => Some(Self::Mlterm),
            _ => None,
        }
    }

    fn detect_term_program() -> Option<Self> {
        match std::env::var("TERM_PROGRAM").ok().as_deref() {
            Some("WezTerm") => Some(Self::WezTerm),
            Some("iTerm.app") => Some(Self::ITerm2),
            Some("ghostty") => Some(Self::Ghostty),
            Some("Apple_Terminal") => Some(Self::AppleTerminal),
            Some("WarpTerminal") => Some(Self::Warp),
            Some("Hyper") => Some(Self::Hyper),
            Some("alacritty") => Some(Self::Alacritty),
            Some("rio") => Some(Self::Rio),
            Some("vscode")
                if get_term_program_major_minor_version()
                    .is_some_and(|version| (1, 80) <= version) =>
            {
                Some(Self::VSCode)
            }
            _ => None,
        }
    }

    /// Look at less-common environment variables terminals set to announce
    /// themselves. Third-tier after `$TERM` and `$TERM_PROGRAM`.
    fn detect_secondary_env() -> Option<Self> {
        if std::env::var_os("WT_SESSION").is_some() {
            return Some(Self::WindowsTerminal);
        }
        if std::env::var_os("KONSOLE_VERSION").is_some() {
            return Some(Self::Konsole);
        }
        if let Ok(value) = std::env::var("TERMINAL_EMULATOR") {
            if value.eq_ignore_ascii_case("contour") {
                return Some(Self::Contour);
            }
        }
        if matches!(std::env::var("TERMINOLOGY").ok().as_deref(), Some("1")) {
            return Some(Self::Terminology);
        }
        None
    }

    /// Attempt to detect the terminal program mdcat is running on.
    ///
    /// Environment variables are consulted in the following priority order:
    ///
    /// 1. `$TERM` (most reliable — it propagates across `sudo`/`ssh`)
    /// 2. `$TERM_PROGRAM`
    /// 3. Terminal-specific markers: `$WT_SESSION` (Windows Terminal),
    ///    `$KONSOLE_VERSION`, `$TERMINAL_EMULATOR` (Contour),
    ///    `$TERMINOLOGY`.
    ///
    /// Falls back to [`TerminalProgram::Ansi`] when no signal is found.
    pub fn detect() -> Self {
        Self::detect_term()
            .or_else(Self::detect_term_program)
            .or_else(Self::detect_secondary_env)
            .unwrap_or(Self::Ansi)
    }

    /// Get the capabilities of this terminal emulator.
    pub fn capabilities(self) -> TerminalCapabilities {
        let ansi = TerminalCapabilities {
            style: Some(StyleCapability::Ansi),
            image: None,
            marks: None,
        };
        let kitty = || ImageCapability::Kitty(self::kitty::KittyGraphicsProtocol);
        let iterm2 = || ImageCapability::ITerm2(ITerm2Protocol);
        #[cfg(feature = "sixel")]
        let sixel = || ImageCapability::Sixel(self::sixel::SixelProtocol);
        match self {
            TerminalProgram::Dumb => TerminalCapabilities::default(),
            TerminalProgram::Ansi
            | TerminalProgram::Alacritty
            | TerminalProgram::Konsole
            | TerminalProgram::AppleTerminal
            | TerminalProgram::Warp
            | TerminalProgram::Hyper => ansi,
            TerminalProgram::ITerm2 => ansi
                .with_mark_capability(MarkCapability::ITerm2(ITerm2Protocol))
                .with_image_capability(iterm2()),
            TerminalProgram::VSCode => ansi.with_image_capability(iterm2()),
            TerminalProgram::Terminology => {
                ansi.with_image_capability(ImageCapability::Terminology(terminology::Terminology))
            }
            TerminalProgram::Kitty
            | TerminalProgram::WezTerm
            | TerminalProgram::Ghostty
            | TerminalProgram::Rio => ansi.with_image_capability(kitty()),
            // Sixel-capable terminals: get the Sixel protocol when the feature
            // is enabled, otherwise fall back to plain ANSI.
            TerminalProgram::Foot
            | TerminalProgram::Contour
            | TerminalProgram::Mlterm
            | TerminalProgram::WindowsTerminal => {
                #[cfg(feature = "sixel")]
                {
                    ansi.with_image_capability(sixel())
                }
                #[cfg(not(feature = "sixel"))]
                {
                    ansi
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::terminal::TerminalProgram;

    use temp_env::with_vars;

    #[test]
    pub fn detect_term_kitty() {
        with_vars(vec![("TERM", Some("xterm-kitty"))], || {
            assert_eq!(TerminalProgram::detect(), TerminalProgram::Kitty)
        })
    }

    #[test]
    pub fn detect_term_wezterm() {
        with_vars(vec![("TERM", Some("wezterm"))], || {
            assert_eq!(TerminalProgram::detect(), TerminalProgram::WezTerm)
        })
    }

    #[test]
    pub fn detect_term_program_wezterm() {
        with_vars(
            vec![
                ("TERM", Some("xterm-256color")),
                ("TERM_PROGRAM", Some("WezTerm")),
            ],
            || assert_eq!(TerminalProgram::detect(), TerminalProgram::WezTerm),
        )
    }

    #[test]
    pub fn detect_term_program_iterm2() {
        with_vars(
            vec![
                ("TERM", Some("xterm-256color")),
                ("TERM_PROGRAM", Some("iTerm.app")),
            ],
            || assert_eq!(TerminalProgram::detect(), TerminalProgram::ITerm2),
        )
    }

    #[test]
    pub fn detect_terminology() {
        with_vars(
            vec![
                ("TERM", Some("xterm-256color")),
                ("TERM_PROGRAM", None),
                ("TERMINOLOGY", Some("1")),
            ],
            || assert_eq!(TerminalProgram::detect(), TerminalProgram::Terminology),
        );
        with_vars(
            vec![
                ("TERM", Some("xterm-256color")),
                ("TERM_PROGRAM", None),
                ("TERMINOLOGY", Some("0")),
            ],
            || assert_eq!(TerminalProgram::detect(), TerminalProgram::Ansi),
        );
    }

    #[test]
    pub fn detect_term_ghostty() {
        with_vars(vec![("TERM", Some("xterm-ghostty"))], || {
            assert_eq!(TerminalProgram::detect(), TerminalProgram::Ghostty)
        })
    }

    #[test]
    pub fn detect_term_program_ghostty() {
        with_vars(
            vec![
                ("TERM", Some("xterm-256color")),
                ("TERM_PROGRAM", Some("ghostty")),
            ],
            || assert_eq!(TerminalProgram::detect(), TerminalProgram::Ghostty),
        )
    }

    #[test]
    pub fn detect_ansi() {
        with_vars(
            vec![
                ("TERM", Some("xterm-256color")),
                ("TERM_PROGRAM", None),
                ("TERMINOLOGY", None),
            ],
            || assert_eq!(TerminalProgram::detect(), TerminalProgram::Ansi),
        )
    }

    /// Regression test for <https://github.com/swsnr/mdcat/issues/230>
    #[test]
    #[allow(non_snake_case)]
    pub fn GH_230_detect_nested_kitty_from_iterm2() {
        with_vars(
            vec![
                ("TERM_PROGRAM", Some("iTerm.app")),
                ("TERM", Some("xterm-kitty")),
            ],
            || assert_eq!(TerminalProgram::detect(), TerminalProgram::Kitty),
        )
    }

    // ─── terminals added in 3.0 ────────────────────────────────────────────

    fn assert_detects(env: Vec<(&str, Option<&str>)>, expected: TerminalProgram) {
        with_vars(env, || assert_eq!(TerminalProgram::detect(), expected));
    }

    #[test]
    fn detect_alacritty_via_term() {
        assert_detects(
            vec![("TERM", Some("alacritty"))],
            TerminalProgram::Alacritty,
        );
    }

    #[test]
    fn detect_alacritty_via_term_program() {
        assert_detects(
            vec![
                ("TERM", Some("xterm-256color")),
                ("TERM_PROGRAM", Some("alacritty")),
            ],
            TerminalProgram::Alacritty,
        );
    }

    #[test]
    fn detect_foot() {
        assert_detects(vec![("TERM", Some("foot"))], TerminalProgram::Foot);
    }

    #[test]
    fn detect_rio_via_term() {
        assert_detects(vec![("TERM", Some("rio"))], TerminalProgram::Rio);
    }

    #[test]
    fn detect_rio_via_term_program() {
        assert_detects(
            vec![
                ("TERM", Some("xterm-256color")),
                ("TERM_PROGRAM", Some("rio")),
            ],
            TerminalProgram::Rio,
        );
    }

    #[test]
    fn detect_mlterm() {
        assert_detects(vec![("TERM", Some("mlterm"))], TerminalProgram::Mlterm);
    }

    #[test]
    fn detect_warp() {
        assert_detects(
            vec![
                ("TERM", Some("xterm-256color")),
                ("TERM_PROGRAM", Some("WarpTerminal")),
            ],
            TerminalProgram::Warp,
        );
    }

    #[test]
    fn detect_hyper() {
        assert_detects(
            vec![
                ("TERM", Some("xterm-256color")),
                ("TERM_PROGRAM", Some("Hyper")),
            ],
            TerminalProgram::Hyper,
        );
    }

    #[test]
    fn detect_apple_terminal() {
        assert_detects(
            vec![
                ("TERM", Some("xterm-256color")),
                ("TERM_PROGRAM", Some("Apple_Terminal")),
            ],
            TerminalProgram::AppleTerminal,
        );
    }

    #[test]
    fn detect_windows_terminal() {
        assert_detects(
            vec![
                ("TERM", Some("xterm-256color")),
                ("TERM_PROGRAM", None),
                ("WT_SESSION", Some("abc-123")),
            ],
            TerminalProgram::WindowsTerminal,
        );
    }

    #[test]
    fn detect_konsole() {
        assert_detects(
            vec![
                ("TERM", Some("xterm-256color")),
                ("TERM_PROGRAM", None),
                ("KONSOLE_VERSION", Some("240100")),
            ],
            TerminalProgram::Konsole,
        );
    }

    #[test]
    fn detect_contour() {
        assert_detects(
            vec![
                ("TERM", Some("xterm-256color")),
                ("TERM_PROGRAM", None),
                ("TERMINAL_EMULATOR", Some("contour")),
            ],
            TerminalProgram::Contour,
        );
    }
}
