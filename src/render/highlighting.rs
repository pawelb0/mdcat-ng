// Copyright 2018-2020 Sebastian Wiesner <sebastian@swsnr.de>

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Tools for syntax highlighting.

use anstyle::Effects;
use std::{
    io::{Result, Write},
    sync::OnceLock,
};
use syntect::highlighting::{FontStyle, Highlighter, Style, Theme};

static SOLARIZED_DARK_DUMP: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/theme.dump"));
static THEME: OnceLock<Theme> = OnceLock::new();
static HIGHLIGHTER: OnceLock<Highlighter> = OnceLock::new();

fn theme() -> &'static Theme {
    THEME.get_or_init(|| syntect::dumps::from_binary(SOLARIZED_DARK_DUMP))
}

pub fn highlighter() -> &'static Highlighter<'static> {
    HIGHLIGHTER.get_or_init(|| Highlighter::new(theme()))
}

/// Write `regions` to `writer`, recoloring Solarized accents through `syntax_map`.
///
/// The Solarized base tones collapse to the terminal default so light
/// and dark variants render identically. Backgrounds are dropped.
pub fn write_as_ansi<'a, W: Write, I: Iterator<Item = (Style, &'a str)>>(
    writer: &mut W,
    regions: I,
    syntax_map: crate::SyntaxMap,
) -> Result<()> {
    for (style, text) in regions {
        let rgb = {
            let fg = style.foreground;
            (fg.r, fg.g, fg.b)
        };
        let color = match rgb {
            // base03, base02, base01, base00, base0, base1, base2, and base3
            (0x00, 0x2b, 0x36)
            | (0x07, 0x36, 0x42)
            | (0x58, 0x6e, 0x75)
            | (0x65, 0x7b, 0x83)
            | (0x83, 0x94, 0x96)
            | (0x93, 0xa1, 0xa1)
            | (0xee, 0xe8, 0xd5)
            | (0xfd, 0xf6, 0xe3) => None,
            (0xb5, 0x89, 0x00) => Some(syntax_map[crate::SLOT_YELLOW].into()),
            (0xcb, 0x4b, 0x16) => Some(syntax_map[crate::SLOT_ORANGE].into()),
            (0xdc, 0x32, 0x2f) => Some(syntax_map[crate::SLOT_RED].into()),
            (0xd3, 0x36, 0x82) => Some(syntax_map[crate::SLOT_MAGENTA].into()),
            (0x6c, 0x71, 0xc4) => Some(syntax_map[crate::SLOT_VIOLET].into()),
            (0x26, 0x8b, 0xd2) => Some(syntax_map[crate::SLOT_BLUE].into()),
            (0x2a, 0xa1, 0x98) => Some(syntax_map[crate::SLOT_CYAN].into()),
            (0x85, 0x99, 0x00) => Some(syntax_map[crate::SLOT_GREEN].into()),
            (r, g, b) => panic!("Unexpected RGB colour: #{r:2>0x}{g:2>0x}{b:2>0x}"),
        };
        let font = style.font_style;
        let effects = Effects::new()
            .set(Effects::BOLD, font.contains(FontStyle::BOLD))
            .set(Effects::ITALIC, font.contains(FontStyle::ITALIC))
            .set(Effects::UNDERLINE, font.contains(FontStyle::UNDERLINE));
        let style = anstyle::Style::new().fg_color(color).effects(effects);
        write!(writer, "{}{}{}", style.render(), text, style.render_reset())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use anstyle::AnsiColor;
    use syntect::highlighting::{Color, FontStyle, Style as SynStyle};

    fn region_yellow_accent() -> Vec<(SynStyle, &'static str)> {
        vec![(
            SynStyle {
                foreground: Color {
                    r: 0xb5,
                    g: 0x89,
                    b: 0x00,
                    a: 0xff,
                },
                background: Color {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 0,
                },
                font_style: FontStyle::empty(),
            },
            "code",
        )]
    }

    #[test]
    fn write_as_ansi_uses_provided_map() {
        let mut classic_buf = Vec::new();
        let classic = [
            AnsiColor::Yellow,
            AnsiColor::BrightRed,
            AnsiColor::Red,
            AnsiColor::Magenta,
            AnsiColor::BrightMagenta,
            AnsiColor::Blue,
            AnsiColor::Cyan,
            AnsiColor::Green,
        ];
        write_as_ansi(
            &mut classic_buf,
            region_yellow_accent().into_iter(),
            classic,
        )
        .unwrap();

        let mut bright_buf = Vec::new();
        let mut bright = classic;
        bright[0] = AnsiColor::BrightYellow;
        write_as_ansi(&mut bright_buf, region_yellow_accent().into_iter(), bright).unwrap();

        assert_ne!(classic_buf, bright_buf);
    }
}
