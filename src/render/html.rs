// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! HTML block rendering.
//!
//! When the parser yields an HTML block we emit it literally with the block
//! style. We split multi-line content on newlines so each line gets the
//! correct indent (the first line uses `initial_indent`; subsequent lines use
//! `indent`).

use std::io::Write;

use crate::error::RenderResult as Result;

use pulldown_cmark::CowStr;
use syntect::util::LinesWithEndings;

use super::state::{HtmlBlockAttrs, StateAndData, StateStack};
use super::write::{write_indent, write_styled};
use super::StateData;
use crate::Settings;

/// Render `Text` inside an HTML block, indenting each line.
pub(super) fn handle_text<'a, W: Write>(
    writer: &mut W,
    settings: &Settings,
    stack: StateStack,
    attrs: HtmlBlockAttrs,
    text: CowStr<'a>,
    data: StateData<'a>,
) -> Result<StateAndData<StateData<'a>>> {
    let HtmlBlockAttrs {
        indent,
        initial_indent,
        style,
    } = attrs;
    for (n, line) in LinesWithEndings::from(&text).enumerate() {
        let line_indent = if n == 0 { initial_indent } else { indent };
        write_indent(writer, line_indent)?;
        write_styled(writer, &settings.terminal_capabilities, &style, line)?;
    }
    stack
        .current(
            HtmlBlockAttrs {
                initial_indent: indent,
                indent,
                style,
            }
            .into(),
        )
        .and_data(data)
        .ok()
}

/// Render `Html` (a single inline HTML element) inside an HTML block.
///
/// Each line is independently indented — previously, multi-line HTML dropped
/// indentation on the 2nd+ lines (upstream TODO at former render.rs:820).
pub(super) fn handle_html<'a, W: Write>(
    writer: &mut W,
    settings: &Settings,
    stack: StateStack,
    attrs: HtmlBlockAttrs,
    html: CowStr<'a>,
    data: StateData<'a>,
) -> Result<StateAndData<StateData<'a>>> {
    let HtmlBlockAttrs {
        indent,
        initial_indent,
        style,
    } = attrs;
    for (n, line) in LinesWithEndings::from(&html).enumerate() {
        let line_indent = if n == 0 { initial_indent } else { indent };
        write_indent(writer, line_indent)?;
        write_styled(writer, &settings.terminal_capabilities, &style, line)?;
    }
    stack
        .current(
            HtmlBlockAttrs {
                initial_indent: indent,
                indent,
                style,
            }
            .into(),
        )
        .and_data(data)
        .ok()
}
