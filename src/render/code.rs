// Copyright 2026 Pawel Boguszewski

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Code block rendering for literal and syntax-highlighted blocks.
//!
//! Each line of a code block is rendered as `<indent>│ <code>` using the
//! theme's `code_block_border_color` for the left bar. There are no top or
//! bottom rules — the left bar itself is the visual marker.

use std::io::Write;

use anstyle::Style;
use pulldown_cmark::CowStr;
use syntect::highlighting::HighlightIterator;
use syntect::util::LinesWithEndings;

use textwrap::core::display_width;

use super::highlighting::{highlighter, write_as_ansi};
use super::state::{HighlightBlockAttrs, LiteralBlockAttrs, StateAndData, StateStack};
use super::write::{
    code_block_inner_width, write_code_block_bottom, write_code_line_suffix, write_indent,
    write_styled,
};
use super::StateData;
use crate::error::RenderResult as Result;
use crate::terminal::capabilities::TerminalCapabilities;
use crate::theme::Theme;
use crate::Settings;

/// Write the per-line left bar (`│ `) for a code block.
pub(super) fn write_code_line_prefix<W: Write>(
    writer: &mut W,
    capabilities: &TerminalCapabilities,
    theme: &Theme,
    indent: u16,
) -> std::io::Result<()> {
    write_indent(writer, indent)?;
    write_styled(
        writer,
        capabilities,
        &Style::new().fg_color(Some(theme.code_block_border_color)),
        "\u{2502} ",
    )
}

/// Write the per-line left bar plus the `↪ ` continuation marker used on
/// wrapped code-block rows. The marker is drawn in the border colour and
/// occupies two display cells so the caller must budget for it.
fn write_code_line_continuation_prefix<W: Write>(
    writer: &mut W,
    capabilities: &TerminalCapabilities,
    theme: &Theme,
    indent: u16,
) -> std::io::Result<()> {
    write_code_line_prefix(writer, capabilities, theme, indent)?;
    write_styled(
        writer,
        capabilities,
        &Style::new().fg_color(Some(theme.code_block_border_color)),
        "\u{21AA} ",
    )
}

/// Split `text` into chunks whose display width is at most `max_width`.
///
/// Returns a vector of byte-index ranges into `text`. Characters are kept
/// intact: if a single character is wider than `max_width` it gets its own
/// chunk (which will overflow, but that's the best we can do).
fn chunk_by_display_width(text: &str, max_width: usize) -> Vec<(usize, usize)> {
    if max_width == 0 || text.is_empty() {
        return vec![(0, text.len())];
    }
    let mut chunks = Vec::new();
    let mut start = 0usize;
    let mut width = 0usize;
    for (idx, ch) in text.char_indices() {
        let w = display_width(ch.encode_utf8(&mut [0; 4]));
        if width > 0 && width + w > max_width {
            chunks.push((start, idx));
            start = idx;
            width = 0;
        }
        width += w;
    }
    chunks.push((start, text.len()));
    chunks
}

/// Emit `Text` within a literal (un-highlighted) code block.
pub(super) fn handle_literal_text<'a, W: Write>(
    writer: &mut W,
    settings: &Settings,
    stack: StateStack,
    attrs: LiteralBlockAttrs,
    text: CowStr<'a>,
    data: StateData<'a>,
) -> Result<StateAndData<StateData<'a>>> {
    let LiteralBlockAttrs { indent, style, .. } = attrs;
    let inner_width = code_block_inner_width(&settings.terminal_size, indent) as usize;
    for line in LinesWithEndings::from(&text) {
        // Split off the trailing `\n` so we can insert the right-hand
        // border between the content and the newline.
        let trailing_newline = line.ends_with('\n');
        let content = if trailing_newline {
            &line[..line.len() - 1]
        } else {
            line
        };
        let needs_wrap = settings.wrap_code && display_width(content) > inner_width;
        if !needs_wrap {
            write_code_line_prefix(
                writer,
                &settings.terminal_capabilities,
                &settings.theme,
                indent,
            )?;
            write_styled(writer, &settings.terminal_capabilities, &style, content)?;
            let width = display_width(content) as u16;
            write_code_line_suffix(
                writer,
                &settings.terminal_capabilities,
                &settings.theme,
                &settings.terminal_size,
                indent,
                width,
            )?;
            continue;
        }

        // Wrap: first chunk uses the full inner width; continuation rows
        // reserve 2 cells for the `↪ ` marker.
        let cont_budget = inner_width.saturating_sub(2).max(1);
        // Take the first chunk at `inner_width`, then chunk the remainder
        // at `cont_budget` so the `↪ ` marker never pushes content past
        // the right border.
        let first_split = chunk_by_display_width(content, inner_width)
            .first()
            .copied()
            .unwrap_or((0, 0));
        let head_slice = &content[first_split.0..first_split.1];
        write_code_line_prefix(
            writer,
            &settings.terminal_capabilities,
            &settings.theme,
            indent,
        )?;
        write_styled(writer, &settings.terminal_capabilities, &style, head_slice)?;
        write_code_line_suffix(
            writer,
            &settings.terminal_capabilities,
            &settings.theme,
            &settings.terminal_size,
            indent,
            display_width(head_slice) as u16,
        )?;

        let rest = &content[first_split.1..];
        for (a, b) in chunk_by_display_width(rest, cont_budget) {
            if a == b {
                continue;
            }
            let slice = &rest[a..b];
            write_code_line_continuation_prefix(
                writer,
                &settings.terminal_capabilities,
                &settings.theme,
                indent,
            )?;
            write_styled(writer, &settings.terminal_capabilities, &style, slice)?;
            write_code_line_suffix(
                writer,
                &settings.terminal_capabilities,
                &settings.theme,
                &settings.terminal_size,
                indent,
                // +2 for the continuation marker we emitted above.
                display_width(slice) as u16 + 2,
            )?;
        }
    }
    stack.current(attrs.into()).and_data(data).ok()
}

/// Close a literal code block (`End(TagEnd::CodeBlock)`) — draws the box
/// bottom (`╰────────╯`).
pub(super) fn handle_literal_end<'a, W: Write>(
    writer: &mut W,
    settings: &Settings,
    stack: StateStack,
    data: StateData<'a>,
    indent: u16,
) -> Result<StateAndData<StateData<'a>>> {
    write_code_block_bottom(
        writer,
        &settings.terminal_capabilities,
        &settings.theme,
        &settings.terminal_size,
        indent,
    )?;
    stack.pop().and_data(data).ok()
}

/// Emit `Text` within a syntax-highlighted code block.
pub(super) fn handle_highlight_text<'a, W: Write>(
    writer: &mut W,
    settings: &Settings,
    stack: StateStack,
    mut attrs: HighlightBlockAttrs,
    text: CowStr<'a>,
    data: StateData<'a>,
) -> Result<StateAndData<StateData<'a>>> {
    let inner_width = code_block_inner_width(&settings.terminal_size, attrs.indent) as usize;
    for line in LinesWithEndings::from(&text) {
        let trailing_newline = line.ends_with('\n');
        let content = if trailing_newline {
            &line[..line.len() - 1]
        } else {
            line
        };

        let ops = attrs
            .parse_state
            .parse_line(content, settings.syntax_set)
            .expect("syntect parsing shouldn't fail in mdcat");
        // Collect segments so we can either emit them directly or rewrap
        // without losing highlighting state.
        let segments: Vec<(syntect::highlighting::Style, &str)> =
            HighlightIterator::new(&mut attrs.highlight_state, &ops, content, highlighter())
                .collect();
        let line_width: usize = segments.iter().map(|(_, s)| display_width(s)).sum();
        let needs_wrap = settings.wrap_code && line_width > inner_width;

        if !needs_wrap {
            write_code_line_prefix(
                writer,
                &settings.terminal_capabilities,
                &settings.theme,
                attrs.indent,
            )?;
            write_as_ansi(writer, segments.into_iter())?;
            write_code_line_suffix(
                writer,
                &settings.terminal_capabilities,
                &settings.theme,
                &settings.terminal_size,
                attrs.indent,
                line_width as u16,
            )?;
            continue;
        }

        // Wrap by splitting the segment stream into rows that each fit
        // inside `row_budget` display cells. The first row uses the full
        // inner width; continuation rows reserve 2 cells for `↪ `.
        let rows = chunk_highlighted_segments(&segments, inner_width);
        for (row_idx, row) in rows.iter().enumerate() {
            if row_idx == 0 {
                write_code_line_prefix(
                    writer,
                    &settings.terminal_capabilities,
                    &settings.theme,
                    attrs.indent,
                )?;
            } else {
                write_code_line_continuation_prefix(
                    writer,
                    &settings.terminal_capabilities,
                    &settings.theme,
                    attrs.indent,
                )?;
            }
            let row_width: usize = row.iter().map(|(_, s)| display_width(s)).sum();
            write_as_ansi(writer, row.iter().map(|(s, t)| (*s, *t)))?;
            let extra = if row_idx == 0 { 0 } else { 2 };
            write_code_line_suffix(
                writer,
                &settings.terminal_capabilities,
                &settings.theme,
                &settings.terminal_size,
                attrs.indent,
                row_width as u16 + extra,
            )?;
        }
    }
    stack.current(attrs.into()).and_data(data).ok()
}

/// Distribute a sequence of highlighted `(Style, &str)` segments across
/// rows such that no row's display width exceeds the per-row budget.
///
/// The first row gets `first_budget` cells; subsequent rows get
/// `first_budget - 2` cells to leave room for the `↪ ` continuation
/// marker. Individual characters are preserved — a segment wider than the
/// budget gets split at character boundaries.
fn chunk_highlighted_segments<'a, S: Copy>(
    segments: &[(S, &'a str)],
    first_budget: usize,
) -> Vec<Vec<(S, &'a str)>> {
    let cont_budget = first_budget.saturating_sub(2).max(1);
    let mut rows: Vec<Vec<(S, &'a str)>> = vec![Vec::new()];
    let mut row_width = 0usize;
    let budget = |rows_len: usize| {
        if rows_len == 1 {
            first_budget
        } else {
            cont_budget
        }
    };

    for &(style, seg) in segments {
        let mut remaining = seg;
        while !remaining.is_empty() {
            let cap = budget(rows.len()).saturating_sub(row_width);
            if cap == 0 {
                rows.push(Vec::new());
                row_width = 0;
                continue;
            }
            // Take as many leading chars as fit in `cap`.
            let mut end = 0usize;
            let mut w = 0usize;
            for (idx, ch) in remaining.char_indices() {
                let cw = display_width(ch.encode_utf8(&mut [0; 4]));
                if w > 0 && w + cw > cap {
                    end = idx;
                    break;
                }
                w += cw;
                end = idx + ch.len_utf8();
                if w >= cap {
                    break;
                }
            }
            if end == 0 {
                // Couldn't fit even one char; force-push this char to a
                // new row.
                rows.push(Vec::new());
                row_width = 0;
                continue;
            }
            let (head, tail) = remaining.split_at(end);
            rows.last_mut().unwrap().push((style, head));
            row_width += w;
            remaining = tail;
            if !remaining.is_empty() {
                rows.push(Vec::new());
                row_width = 0;
            }
        }
    }
    if rows.last().is_some_and(|r| r.is_empty()) && rows.len() > 1 {
        rows.pop();
    }
    rows
}
