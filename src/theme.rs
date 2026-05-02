// Copyright 2018-2020 Sebastian Wiesner <sebastian@swsnr.de>

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Provide a colour theme for mdcat.

use anstyle::{AnsiColor, Color, Style};

/// A colour theme for mdcat.
///
/// Currently you cannot create custom styles, but only use the default theme via [`Theme::default`].
#[derive(Debug, Clone)]
pub struct Theme {
    /// Style for HTML blocks.
    pub(crate) html_block_style: Style,
    /// Style for inline HTML.
    pub(crate) inline_html_style: Style,
    /// Style for code, unless the code is syntax-highlighted.
    pub(crate) code_style: Style,
    /// Style for links.
    pub(crate) link_style: Style,
    /// Color for image links (unless the image is rendered inline)
    pub(crate) image_link_style: Style,
    /// Color for rulers.
    pub(crate) rule_color: Color,
    /// Color for borders around code blocks.
    pub(crate) code_block_border_color: Color,
    /// Color for the `▌` bar drawn on every line of a blockquote.
    pub(crate) quote_bar_color: Color,
    /// Color for headings
    pub(crate) heading_style: Style,
}

impl Default for Theme {
    /// The default theme from mdcat 1.x
    fn default() -> Self {
        Self {
            html_block_style: Style::new().fg_color(Some(AnsiColor::Green.into())),
            inline_html_style: Style::new().fg_color(Some(AnsiColor::Green.into())),
            code_style: Style::new().fg_color(Some(AnsiColor::Yellow.into())),
            link_style: Style::new().fg_color(Some(AnsiColor::Blue.into())),
            image_link_style: Style::new().fg_color(Some(AnsiColor::Magenta.into())),
            rule_color: AnsiColor::Green.into(),
            code_block_border_color: AnsiColor::Green.into(),
            quote_bar_color: AnsiColor::BrightBlack.into(),
            heading_style: Style::new().fg_color(Some(AnsiColor::Blue.into())).bold(),
        }
    }
}

/// AnsiColor slots for the eight Solarized accent colors. Index with
/// the [`SLOT_YELLOW`] / [`SLOT_ORANGE`] / [`SLOT_RED`] / [`SLOT_MAGENTA`]
/// / [`SLOT_VIOLET`] / [`SLOT_BLUE`] / [`SLOT_CYAN`] / [`SLOT_GREEN`]
/// constants from this module.
pub type SyntaxMap = [anstyle::AnsiColor; 8];

/// Slot index in [`SyntaxMap`] for the Solarized yellow accent.
pub const SLOT_YELLOW: usize = 0;
/// Slot index for Solarized orange.
pub const SLOT_ORANGE: usize = 1;
/// Slot index for Solarized red.
pub const SLOT_RED: usize = 2;
/// Slot index for Solarized magenta.
pub const SLOT_MAGENTA: usize = 3;
/// Slot index for Solarized violet.
pub const SLOT_VIOLET: usize = 4;
/// Slot index for Solarized blue.
pub const SLOT_BLUE: usize = 5;
/// Slot index for Solarized cyan.
pub const SLOT_CYAN: usize = 6;
/// Slot index for Solarized green.
pub const SLOT_GREEN: usize = 7;

/// Built-in color preset selectable via `--theme`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum Preset {
    /// Pastel default: cool slots, magenta headings
    #[default]
    Catppuccin,
    /// mdcat 1.x defaults
    Classic,
    /// Warm magenta-led palette
    Dracula,
    /// Cool blue palette
    Nord,
}

impl Preset {
    /// Short one-line description for `--list-themes`.
    pub fn description(self) -> &'static str {
        match self {
            Preset::Catppuccin => "Pastel default. Cool slots, magenta headings.",
            Preset::Classic => "mdcat 1.x defaults.",
            Preset::Dracula => "Warm magenta-led palette.",
            Preset::Nord => "Cool blue palette.",
        }
    }

    /// Chrome colors (headings, links, rules, etc.) for this preset.
    pub fn theme(self) -> Theme {
        use anstyle::AnsiColor::{
            Blue, BrightBlack, BrightBlue, BrightCyan, BrightMagenta, BrightYellow, Cyan, Magenta,
        };
        use anstyle::Style;
        match self {
            Preset::Catppuccin => Theme {
                heading_style: Style::new().fg_color(Some(Magenta.into())).bold(),
                link_style: Style::new().fg_color(Some(Cyan.into())),
                code_style: Style::new().fg_color(Some(BrightYellow.into())),
                image_link_style: Style::new().fg_color(Some(BrightMagenta.into())),
                rule_color: BrightBlack.into(),
                code_block_border_color: BrightBlack.into(),
                quote_bar_color: BrightBlack.into(),
                html_block_style: Style::new().fg_color(Some(BrightBlack.into())),
                inline_html_style: Style::new().fg_color(Some(BrightBlack.into())),
            },
            Preset::Classic => Theme::default(),
            Preset::Dracula => Theme {
                heading_style: Style::new().fg_color(Some(BrightMagenta.into())).bold(),
                link_style: Style::new().fg_color(Some(BrightCyan.into())),
                code_style: Style::new().fg_color(Some(BrightYellow.into())),
                image_link_style: Style::new().fg_color(Some(BrightMagenta.into())),
                rule_color: BrightMagenta.into(),
                code_block_border_color: BrightBlack.into(),
                quote_bar_color: BrightBlack.into(),
                html_block_style: Style::new().fg_color(Some(BrightMagenta.into())),
                inline_html_style: Style::new().fg_color(Some(BrightMagenta.into())),
            },
            Preset::Nord => Theme {
                heading_style: Style::new().fg_color(Some(BrightCyan.into())).bold(),
                link_style: Style::new().fg_color(Some(Cyan.into())),
                code_style: Style::new().fg_color(Some(BrightBlue.into())),
                image_link_style: Style::new().fg_color(Some(Blue.into())),
                rule_color: BrightBlack.into(),
                code_block_border_color: BrightBlack.into(),
                quote_bar_color: BrightBlack.into(),
                html_block_style: Style::new().fg_color(Some(Cyan.into())),
                inline_html_style: Style::new().fg_color(Some(Cyan.into())),
            },
        }
    }

    /// Syntax-token AnsiColor mapping for this preset.
    pub fn syntax_map(self) -> SyntaxMap {
        use anstyle::AnsiColor::{
            Blue, BrightBlue, BrightCyan, BrightGreen, BrightMagenta, BrightRed, BrightYellow,
            Cyan, Green, Magenta, Red, Yellow,
        };
        match self {
            // Pastel: warm slots → bright peach/yellow, blue softened to cyan,
            // greens/cyans bumped bright. Violet collapses onto magenta.
            Preset::Catppuccin => [
                BrightYellow,
                BrightYellow,
                BrightRed,
                Magenta,
                Magenta,
                Cyan,
                BrightCyan,
                BrightGreen,
            ],
            // Legacy mdcat 1.x mapping. Mirrors the old hardcoded table in
            // `write_as_ansi`. Do not change without bumping the major.
            Preset::Classic => [
                Yellow,
                BrightRed,
                Red,
                Magenta,
                BrightMagenta,
                Blue,
                Cyan,
                Green,
            ],
            // Warm magenta-led: most slots bright; blue/violet collapse onto cyan.
            Preset::Dracula => [
                BrightYellow,
                BrightRed,
                BrightRed,
                BrightMagenta,
                BrightMagenta,
                BrightCyan,
                BrightCyan,
                BrightGreen,
            ],
            // Cool/icy: blue → bright blue, violet → bright cyan, warm slots
            // pushed yellow to keep the palette cool overall.
            Preset::Nord => [
                Yellow,
                BrightYellow,
                Red,
                BrightMagenta,
                BrightCyan,
                BrightBlue,
                BrightCyan,
                BrightGreen,
            ],
        }
    }
}

/// Combine styles.
pub trait CombineStyle {
    /// Put this style on top of the other style.
    ///
    /// Return a new style which falls back to the `other` style for all style attributes not
    /// specified in this style.
    fn on_top_of(self, other: &Self) -> Self;
}

impl CombineStyle for Style {
    /// Put this style on top of the `other` style.
    fn on_top_of(self, other: &Style) -> Style {
        Style::new()
            .fg_color(self.get_fg_color().or(other.get_fg_color()))
            .bg_color(self.get_bg_color().or(other.get_bg_color()))
            .effects(other.get_effects() | self.get_effects())
            .underline_color(self.get_underline_color().or(other.get_underline_color()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anstyle::AnsiColor;

    fn fg(s: anstyle::Style) -> Option<anstyle::Color> {
        s.get_fg_color()
    }

    #[test]
    fn classic_matches_legacy_default() {
        let p = Preset::Classic.theme();
        let d = Theme::default();
        assert_eq!(fg(p.heading_style), fg(d.heading_style));
        assert_eq!(fg(p.link_style), fg(d.link_style));
        assert_eq!(fg(p.code_style), fg(d.code_style));
        assert_eq!(p.rule_color, d.rule_color);
        assert_eq!(p.quote_bar_color, d.quote_bar_color);
    }

    #[test]
    fn catppuccin_heading_is_magenta_bold() {
        let t = Preset::Catppuccin.theme();
        assert_eq!(fg(t.heading_style), Some(AnsiColor::Magenta.into()));
        assert!(t
            .heading_style
            .get_effects()
            .contains(anstyle::Effects::BOLD));
    }

    #[test]
    fn dracula_link_is_brightcyan() {
        let t = Preset::Dracula.theme();
        assert_eq!(fg(t.link_style), Some(AnsiColor::BrightCyan.into()));
    }

    #[test]
    fn nord_heading_is_brightcyan_bold() {
        let t = Preset::Nord.theme();
        assert_eq!(fg(t.heading_style), Some(AnsiColor::BrightCyan.into()));
        assert!(t
            .heading_style
            .get_effects()
            .contains(anstyle::Effects::BOLD));
    }

    #[test]
    fn default_preset_is_catppuccin() {
        assert_eq!(Preset::default(), Preset::Catppuccin);
    }

    #[test]
    fn classic_syntax_map_matches_legacy_table() {
        let m = Preset::Classic.syntax_map();
        // Slot order: yellow, orange, red, magenta, violet, blue, cyan, green
        assert_eq!(
            m,
            [
                AnsiColor::Yellow,
                AnsiColor::BrightRed,
                AnsiColor::Red,
                AnsiColor::Magenta,
                AnsiColor::BrightMagenta,
                AnsiColor::Blue,
                AnsiColor::Cyan,
                AnsiColor::Green,
            ]
        );
    }

    #[test]
    fn catppuccin_syntax_map_softens_blue_to_cyan() {
        let m = Preset::Catppuccin.syntax_map();
        assert_eq!(m[0], AnsiColor::BrightYellow); // yellow slot
        assert_eq!(m[5], AnsiColor::Cyan); // blue → cyan (pastel)
        assert_eq!(m[7], AnsiColor::BrightGreen); // green slot
    }

    #[test]
    fn dracula_syntax_map_uses_bright_variants() {
        let m = Preset::Dracula.syntax_map();
        assert_eq!(m[2], AnsiColor::BrightRed); // red slot
        assert_eq!(m[5], AnsiColor::BrightCyan); // blue slot
        assert_eq!(m[7], AnsiColor::BrightGreen); // green slot
    }

    #[test]
    fn nord_pushes_blue_violet_to_cool_brights() {
        let m = Preset::Nord.syntax_map();
        assert_eq!(m[4], AnsiColor::BrightCyan); // violet slot
        assert_eq!(m[5], AnsiColor::BrightBlue); // blue → bright blue
    }
}
