// Copyright 2020 Sebastian Wiesner <sebastian@swsnr.de>

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::cmp::max;
use std::io::{Result, Write};
use std::iter::zip;

use anstyle::Style;
use pulldown_cmark::{Alignment, BlockQuoteKind, CodeBlockKind, HeadingLevel};
use syntect::highlighting::HighlightState;
use syntect::parsing::{ParseState, ScopeStack};
use textwrap::core::{display_width, Word};
use textwrap::WordSeparator;

use crate::references::*;
use crate::render::data::{CurrentLine, CurrentTable, LinkReferenceDefinition, TableCell};
use crate::render::highlighting::highlighter;
use crate::render::state::*;
use crate::terminal::capabilities::{MarkCapability, StyleCapability, TerminalCapabilities};
use crate::terminal::osc::{clear_link, set_link_url};
use crate::terminal::TerminalSize;
use crate::theme::CombineStyle;
use crate::Theme;
use crate::{Environment, Settings};

pub fn write_indent<W: Write>(writer: &mut W, level: u16) -> Result<()> {
    write!(writer, "{}", " ".repeat(level as usize))
}

/// Write the start of a new line — `indent` spaces, optionally interrupted by
/// `▌ ` blockquote accent bars at each column in `quote_bar_cols` in
/// `theme.quote_bar_color`.
///
/// When `quote_bar_cols` is empty this behaves exactly like [`write_indent`].
/// Otherwise each bar is drawn at its column (sorted smallest first), with
/// plain spaces padding between bars and to the final `indent`. Nested
/// quotes stack, so three levels produce `▌ ▌ ▌ `.
pub fn write_line_start<W: Write>(
    writer: &mut W,
    capabilities: &TerminalCapabilities,
    theme: &Theme,
    indent: u16,
    quote_bar_cols: &[u16],
) -> Result<()> {
    if quote_bar_cols.is_empty() {
        return write_indent(writer, indent);
    }
    let style = Style::new().fg_color(Some(theme.quote_bar_color));
    let mut col: u16 = 0;
    for &bar_col in quote_bar_cols {
        if bar_col >= col {
            write_indent(writer, bar_col - col)?;
            col = bar_col;
        }
        // Each `▌ ` glyph occupies two display columns.
        write_styled(writer, capabilities, &style, "\u{258C} ")?;
        col += 2;
    }
    if col < indent {
        write_indent(writer, indent - col)?;
    }
    Ok(())
}

/// Emit the GFM alert label (e.g. `NOTE`, `WARNING`) inside a blockquote.
///
/// Writes one full line: the leading bar prefix for the enclosing
/// blockquote, then the uppercase label styled with a kind-specific
/// colour, then a newline. Called once when a blockquote starts with
/// a recognised [`BlockQuoteKind`]; subsequent paragraph content
/// renders normally.
pub fn write_alert_label<W: Write>(
    writer: &mut W,
    capabilities: &TerminalCapabilities,
    theme: &Theme,
    attrs: &StyledBlockAttrs,
    kind: BlockQuoteKind,
) -> Result<()> {
    write_line_start(
        writer,
        capabilities,
        theme,
        attrs.indent,
        &attrs.quote_bar_cols,
    )?;
    let (label, color) = match kind {
        BlockQuoteKind::Note => ("NOTE", anstyle::AnsiColor::Blue),
        BlockQuoteKind::Tip => ("TIP", anstyle::AnsiColor::Green),
        BlockQuoteKind::Important => ("IMPORTANT", anstyle::AnsiColor::Magenta),
        BlockQuoteKind::Warning => ("WARNING", anstyle::AnsiColor::Yellow),
        BlockQuoteKind::Caution => ("CAUTION", anstyle::AnsiColor::Red),
    };
    let style = Style::new().bold().fg_color(Some(color.into()));
    write_styled(writer, capabilities, &style, label)?;
    writeln!(writer)
}

pub fn write_styled<W: Write, S: AsRef<str>>(
    writer: &mut W,
    capabilities: &TerminalCapabilities,
    style: &Style,
    text: S,
) -> Result<()> {
    match capabilities.style {
        None => write!(writer, "{}", text.as_ref()),
        Some(StyleCapability::Ansi) => write!(
            writer,
            "{}{}{}",
            style.render(),
            text.as_ref(),
            style.render_reset()
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn write_remaining_lines<W: Write>(
    writer: &mut W,
    capabilities: &TerminalCapabilities,
    theme: &Theme,
    style: &Style,
    indent: u16,
    quote_bar_cols: &[u16],
    mut buffer: String,
    next_lines: &[&[Word]],
    last_line: &[Word],
) -> Result<CurrentLine> {
    // Finish the previous line
    writeln!(writer)?;
    write_line_start(writer, capabilities, theme, indent, quote_bar_cols)?;
    // Now write all lines up to the last
    for line in next_lines {
        match line.split_last() {
            None => {
                // The line was empty, so there's nothing to do anymore.
            }
            Some((last, heads)) => {
                for word in heads {
                    buffer.push_str(word.word);
                    buffer.push_str(word.whitespace);
                }
                buffer.push_str(last.word);
                write_styled(writer, capabilities, style, &buffer)?;
                writeln!(writer)?;
                write_line_start(writer, capabilities, theme, indent, quote_bar_cols)?;
                buffer.clear();
            }
        };
    }

    // Now write the last line and keep track of its width
    match last_line.split_last() {
        None => {
            // The line was empty, so there's nothing to do anymore.
            Ok(CurrentLine::empty())
        }
        Some((last, heads)) => {
            for word in heads {
                buffer.push_str(word.word);
                buffer.push_str(word.whitespace);
            }
            buffer.push_str(last.word);
            write_styled(writer, capabilities, style, &buffer)?;
            Ok(CurrentLine {
                length: textwrap::core::display_width(&buffer) as u16,
                trailing_space: Some(last.whitespace.to_owned()),
            })
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn write_styled_and_wrapped<W: Write, S: AsRef<str>>(
    writer: &mut W,
    capabilities: &TerminalCapabilities,
    theme: &Theme,
    style: &Style,
    max_width: u16,
    indent: u16,
    quote_bar_cols: &[u16],
    current_line: CurrentLine,
    text: S,
) -> Result<CurrentLine> {
    let words = WordSeparator::UnicodeBreakProperties
        .find_words(text.as_ref())
        .collect::<Vec<_>>();
    match words.first() {
        // There were no words in the text so we just do nothing.
        None => Ok(current_line),
        Some(first_word) => {
            let current_width = current_line.length
                + indent
                + current_line
                    .trailing_space
                    .as_ref()
                    .map_or(0, |s| display_width(s.as_ref()) as u16);

            // If the current line is not empty and we can't even add the first first word of the text to it
            // then lets finish the line and start over.  If the current line is empty the word simply doesn't
            // fit into the terminal size so we must print it anyway.
            if 0 < current_line.length
                && max_width < current_width + display_width(first_word) as u16
            {
                writeln!(writer)?;
                write_line_start(writer, capabilities, theme, indent, quote_bar_cols)?;
                return write_styled_and_wrapped(
                    writer,
                    capabilities,
                    theme,
                    style,
                    max_width,
                    indent,
                    quote_bar_cols,
                    CurrentLine::empty(),
                    text,
                );
            }

            let widths = [
                // For the first line we need to subtract the length of the current line, and
                // the trailing space we need to add if we add more words to this line
                (max_width - current_width.min(max_width)) as f64,
                // For remaining lines we only need to account for the indent
                (max_width - indent) as f64,
            ];
            let lines = textwrap::wrap_algorithms::wrap_first_fit(&words, &widths);
            match lines.split_first() {
                None => {
                    // there was nothing to wrap so we continue as before
                    Ok(current_line)
                }
                Some((first_line, tails)) => {
                    let mut buffer = String::with_capacity(max_width as usize);

                    // Finish the current line
                    let new_current_line = match first_line.split_last() {
                        None => {
                            // The first line was empty, so there's nothing to do anymore.
                            current_line
                        }
                        Some((last, heads)) => {
                            if let Some(s) = current_line.trailing_space {
                                buffer.push_str(&s);
                            }
                            for word in heads {
                                buffer.push_str(word.word);
                                buffer.push_str(word.whitespace);
                            }
                            buffer.push_str(last.word);
                            let length =
                                current_line.length + textwrap::core::display_width(&buffer) as u16;
                            write_styled(writer, capabilities, style, &buffer)?;
                            buffer.clear();
                            CurrentLine {
                                length,
                                trailing_space: Some(last.whitespace.to_owned()),
                            }
                        }
                    };

                    // Now write the rest of the lines
                    match tails.split_last() {
                        None => {
                            // There are no more lines and we're done here.
                            //
                            // We arrive here when the text fragment we wrapped above was
                            // shorter than the max length of the current line, i.e. we're
                            // still continuing with the current line.
                            Ok(new_current_line)
                        }
                        Some((last_line, next_lines)) => write_remaining_lines(
                            writer,
                            capabilities,
                            theme,
                            style,
                            indent,
                            quote_bar_cols,
                            buffer,
                            next_lines,
                            last_line,
                        ),
                    }
                }
            }
        }
    }
}

pub fn write_mark<W: Write>(writer: &mut W, capabilities: &TerminalCapabilities) -> Result<()> {
    if let Some(mark) = capabilities.marks {
        match mark {
            MarkCapability::ITerm2(marks) => marks.set_mark(writer),
        }
    } else {
        Ok(())
    }
}

pub fn write_rule<W: Write>(
    writer: &mut W,
    capabilities: &TerminalCapabilities,
    theme: &Theme,
    length: u16,
) -> std::io::Result<()> {
    let rule = "\u{2550}".repeat(length as usize);
    write_styled(
        writer,
        capabilities,
        &Style::new().fg_color(Some(theme.rule_color)),
        rule,
    )
}

pub fn write_link_refs<W: Write>(
    writer: &mut W,
    environment: &Environment,
    capabilities: &TerminalCapabilities,
    links: Vec<LinkReferenceDefinition>,
) -> Result<()> {
    if !links.is_empty() {
        writeln!(writer)?;
        for link in links {
            write_styled(
                writer,
                capabilities,
                &link.style,
                format!("[{}]: ", link.index),
            )?;

            // If we can resolve the link try to write it as inline link to make the URL
            // clickable.  This mostly helps images inside inline links which we had to write as
            // reference links because we can't nest inline links.
            if let Some(url) = environment.resolve_reference(&link.target) {
                match &capabilities.style {
                    Some(StyleCapability::Ansi) => {
                        set_link_url(writer, url, &environment.hostname)?;
                        write_styled(writer, capabilities, &link.style, link.target)?;
                        clear_link(writer)?;
                    }
                    None => write_styled(writer, capabilities, &link.style, link.target)?,
                };
            } else {
                write_styled(writer, capabilities, &link.style, link.target)?;
            }

            if !link.title.is_empty() {
                write_styled(
                    writer,
                    capabilities,
                    &link.style,
                    format!(" {}", link.title),
                )?;
            }
            writeln!(writer)?;
        }
    };
    Ok(())
}

/// Inner width (between the left and right `│`) of a code block at the
/// given base indent on the current terminal.
pub fn code_block_inner_width(terminal_size: &TerminalSize, indent: u16) -> u16 {
    // Reserve 4 columns for the box chrome: `│ `, ` │`.
    terminal_size
        .columns
        .saturating_sub(indent)
        .saturating_sub(4)
        .max(1)
}

/// Write the top border of a code block: `╭─ lang ──────────╮`.
///
/// Total visible width matches the content row (`│ ` + inner + ` │` =
/// `inner + 4`). When there is no language the label segment is omitted.
pub fn write_code_block_top<W: Write>(
    writer: &mut W,
    capabilities: &TerminalCapabilities,
    theme: &Theme,
    terminal_size: &TerminalSize,
    indent: u16,
    language: Option<&str>,
) -> std::io::Result<()> {
    let style = Style::new().fg_color(Some(theme.code_block_border_color));
    write_indent(writer, indent)?;
    let inner = code_block_inner_width(terminal_size, indent) as usize;
    let label = language.filter(|s| !s.is_empty()).unwrap_or("");
    let label_piece = if label.is_empty() {
        String::new()
    } else {
        format!(" {label} ")
    };
    let label_w = display_width(&label_piece);
    // Full line: ╭(1) + ──(2) + label + fill + ╮(1) = inner + 4.
    let fill_count = (inner + 4).saturating_sub(4 + label_w);
    let fill: String = std::iter::repeat_n('\u{2500}', fill_count).collect();
    let line = format!("\u{256D}\u{2500}\u{2500}{label_piece}{fill}\u{256E}");
    write_styled(writer, capabilities, &style, line)?;
    writeln!(writer)
}

/// Write the bottom border of a code block: `╰─────────╯`.
pub fn write_code_block_bottom<W: Write>(
    writer: &mut W,
    capabilities: &TerminalCapabilities,
    theme: &Theme,
    terminal_size: &TerminalSize,
    indent: u16,
) -> std::io::Result<()> {
    let style = Style::new().fg_color(Some(theme.code_block_border_color));
    write_indent(writer, indent)?;
    let inner = code_block_inner_width(terminal_size, indent) as usize;
    // ╰(1) + body + ╯(1) = inner + 4  →  body = inner + 2.
    let body: String = std::iter::repeat_n('\u{2500}', inner + 2).collect();
    let line = format!("\u{2570}{body}\u{256F}");
    write_styled(writer, capabilities, &style, line)?;
    writeln!(writer)
}

/// Write the right-side of a code-block line: padding spaces to reach the
/// right border column, then ` │`, then a newline.
pub fn write_code_line_suffix<W: Write>(
    writer: &mut W,
    capabilities: &TerminalCapabilities,
    theme: &Theme,
    terminal_size: &TerminalSize,
    indent: u16,
    content_width: u16,
) -> std::io::Result<()> {
    let style = Style::new().fg_color(Some(theme.code_block_border_color));
    let inner = code_block_inner_width(terminal_size, indent);
    let padding = inner.saturating_sub(content_width) as usize;
    if padding > 0 {
        write!(writer, "{}", " ".repeat(padding))?;
    }
    write_styled(writer, capabilities, &style, " \u{2502}")?;
    writeln!(writer)
}

pub fn write_start_code_block<W: Write>(
    writer: &mut W,
    settings: &Settings,
    indent: u16,
    style: Style,
    block_kind: CodeBlockKind<'_>,
) -> Result<StackedState> {
    let language: Option<&str> = match &block_kind {
        CodeBlockKind::Fenced(name) if !name.is_empty() => Some(name.as_ref()),
        _ => None,
    };
    write_code_block_top(
        writer,
        &settings.terminal_capabilities,
        &settings.theme,
        &settings.terminal_size,
        indent,
        language,
    )?;

    match (&settings.terminal_capabilities.style, block_kind) {
        (Some(StyleCapability::Ansi), CodeBlockKind::Fenced(name)) if !name.is_empty() => {
            match settings.syntax_set.find_syntax_by_token(&name) {
                None => Ok(LiteralBlockAttrs {
                    indent,
                    style: settings.theme.code_style.on_top_of(&style),
                }
                .into()),
                Some(syntax) => {
                    let parse_state = ParseState::new(syntax);
                    let highlight_state = HighlightState::new(highlighter(), ScopeStack::new());
                    Ok(HighlightBlockAttrs {
                        parse_state,
                        highlight_state,
                        indent,
                    }
                    .into())
                }
            }
        }
        (_, _) => Ok(LiteralBlockAttrs {
            indent,
            style: settings.theme.code_style.on_top_of(&style),
        }
        .into()),
    }
}

// Signature stays `Result` because the call sites use `?` — heading styles
// may regain fallible setup later (e.g. jump-mark emission).
#[allow(clippy::unnecessary_wraps)]
pub fn write_start_heading<W: Write>(
    _writer: &mut W,
    _capabilities: &TerminalCapabilities,
    style: Style,
    _level: HeadingLevel,
) -> Result<StackedState> {
    // H1 and H2 get a trailing `═`/`─` underline (see `write_heading_rule`)
    // and the rest rely solely on the bold heading style for differentiation.
    // No prefix is emitted — `###`-style prefixes reads as raw markdown that
    // wasn't parsed.

    // Headlines never wrap, so indent doesn't matter
    Ok(StackedState::Inline(
        InlineState::InlineBlock,
        InlineAttrs {
            style,
            indent: 0,
            quote_bar_cols: Vec::new(),
        },
    ))
}

/// Write the trailing underline for an H1/H2 heading.
///
/// H1 uses `═`, H2 uses `─`; together with the bold heading style this
/// gives two distinct visual tiers. H3+ have no rule — they rely on the
/// `###` prefix for differentiation.
pub fn write_heading_rule<W: Write>(
    writer: &mut W,
    capabilities: &TerminalCapabilities,
    style: Style,
    level: HeadingLevel,
    indent: u16,
    terminal_size: &TerminalSize,
) -> std::io::Result<()> {
    let glyph = match level {
        HeadingLevel::H1 => '\u{2550}', // ═
        HeadingLevel::H2 => '\u{2500}', // ─
        _ => return Ok(()),
    };
    let length = terminal_size.columns.saturating_sub(indent);
    if length == 0 {
        return Ok(());
    }
    write_indent(writer, indent)?;
    let rule: String = std::iter::repeat_n(glyph, length as usize).collect();
    write_styled(writer, capabilities, &style, rule)?;
    writeln!(writer)
}

/// Display width that accounts for emoji presentation selectors.
///
/// `unicode-width` (used by `textwrap::core::display_width`) does not
/// count the variation selector `U+FE0F` — but when it follows a
/// symbol like `⚠` it forces emoji presentation, which most terminals
/// render two cells wide. We add one cell per selector to compensate
/// so tables and truncation honour the rendered width.
pub(crate) fn display_width_with_emoji(s: &str) -> usize {
    display_width(s) + s.chars().filter(|&c| c == '\u{FE0F}').count()
}

fn calculate_column_widths(table: &CurrentTable) -> Option<Vec<usize>> {
    let first_row = table.head.as_ref().or(table.rows.first())?;
    let mut widths = vec![0; first_row.cells.len()];
    let rows = table.head.iter().chain(table.rows.as_slice());
    for row in rows {
        let current = row.cells.as_slice().iter().map(|cell| {
            cell.fragments
                .as_slice()
                .iter()
                .map(|s| display_width_with_emoji(s))
                .sum::<usize>()
        });
        widths = zip(widths, current).map(|(a, b)| max(a, b)).collect();
    }
    Some(widths)
}

fn format_table_cell(cell: TableCell, width: usize, alignment: Alignment) -> String {
    use Alignment::*;
    let content = cell.fragments.join("");
    let content = truncate_to_width(&content, width);
    // Rust's `width$` format specifier counts `char`s, not display cells,
    // so we compute the padding ourselves to match the caller's idea of
    // display width (which accounts for emoji VS16 selectors).
    let displayed = display_width_with_emoji(&content);
    let pad = width.saturating_sub(displayed);
    let (left_pad, right_pad) = match alignment {
        Left | None => (0, pad),
        Right => (pad, 0),
        Center => (pad / 2, pad - pad / 2),
    };
    let spaces = |n| " ".repeat(n);
    format!(" {}{}{} ", spaces(left_pad), content, spaces(right_pad))
}

/// Truncate `s` so that its display width is at most `max_width`, adding an
/// ellipsis at the end if we cut anything off.
fn truncate_to_width(s: &str, max_width: usize) -> String {
    if display_width_with_emoji(s) <= max_width {
        return s.to_string();
    }
    if max_width == 0 {
        return String::new();
    }
    // Keep room for the trailing ellipsis (width 1).
    let budget = max_width.saturating_sub(1);
    let mut out = String::new();
    let mut width = 0usize;
    for ch in s.chars() {
        // Treat variation selector U+FE0F as adding one cell, matching
        // terminal emoji-presentation behaviour.
        let w = display_width(ch.encode_utf8(&mut [0; 4])) + usize::from(ch == '\u{FE0F}');
        if width + w > budget {
            break;
        }
        out.push(ch);
        width += w;
    }
    out.push('\u{2026}');
    out
}

/// Assemble a box-drawing rule across the table columns.
///
/// `left`, `mid`, and `right` are the three junction glyphs; the fill
/// glyph between them is `─`. Each segment is `column_width + 2`
/// horizontal glyphs wide to account for the single-space padding the
/// cells get from [`format_table_cell`].
fn write_box_rule<W: Write>(
    writer: &mut W,
    capabilities: &TerminalCapabilities,
    theme: &Theme,
    widths: &[usize],
    left: char,
    mid: char,
    right: char,
) -> Result<()> {
    let mut line = String::new();
    line.push(left);
    for (i, &width) in widths.iter().enumerate() {
        line.extend(std::iter::repeat_n('\u{2500}', width + 2));
        line.push(if i + 1 == widths.len() { right } else { mid });
    }
    write_styled(
        writer,
        capabilities,
        &Style::new().fg_color(Some(theme.rule_color)),
        line,
    )?;
    writeln!(writer)
}

fn write_box_sep<W: Write>(
    writer: &mut W,
    capabilities: &TerminalCapabilities,
    theme: &Theme,
) -> Result<()> {
    write_styled(
        writer,
        capabilities,
        &Style::new().fg_color(Some(theme.rule_color)),
        "\u{2502}",
    )
}

pub fn write_table<W: Write>(
    writer: &mut W,
    capabilities: &TerminalCapabilities,
    theme: &Theme,
    terminal_size: &TerminalSize,
    table: CurrentTable,
) -> Result<()> {
    let Some(widths) = calculate_column_widths(&table) else {
        return Ok(());
    };

    // Trim widths to fit the terminal if a single row would overflow. We
    // subtract 1 (outer borders) + widths.len() (column separators) + 2 per
    // column (cell padding) from the available columns, then scale down
    // the widest cell until the row fits.
    let chrome = 1 + widths.len() as u16 + 2 * widths.len() as u16;
    let max_inner = terminal_size.columns.saturating_sub(chrome);
    let widths = shrink_widths_to_fit(widths, max_inner as usize);

    write_box_rule(
        writer,
        capabilities,
        theme,
        &widths,
        '\u{250C}',
        '\u{252C}',
        '\u{2510}',
    )?;

    if let Some(head) = table.head {
        for ((cell, &width), &alignment) in zip(zip(head.cells, &widths), &table.alignments) {
            write_box_sep(writer, capabilities, theme)?;
            write_styled(
                writer,
                capabilities,
                &Style::new().bold(),
                format_table_cell(cell, width, alignment),
            )?;
        }
        write_box_sep(writer, capabilities, theme)?;
        writeln!(writer)?;
        write_box_rule(
            writer,
            capabilities,
            theme,
            &widths,
            '\u{251C}',
            '\u{253C}',
            '\u{2524}',
        )?;
    }

    for row in table.rows {
        for ((cell, &width), &alignment) in zip(zip(row.cells, &widths), &table.alignments) {
            write_box_sep(writer, capabilities, theme)?;
            write_styled(
                writer,
                capabilities,
                &Style::new(),
                format_table_cell(cell, width, alignment),
            )?;
        }
        write_box_sep(writer, capabilities, theme)?;
        writeln!(writer)?;
    }

    write_box_rule(
        writer,
        capabilities,
        theme,
        &widths,
        '\u{2514}',
        '\u{2534}',
        '\u{2518}',
    )
}

/// If the sum of column widths plus their padding exceeds the available
/// inner width, shrink the widest column(s) until we fit. No-op if we
/// already fit.
fn shrink_widths_to_fit(mut widths: Vec<usize>, max_inner: usize) -> Vec<usize> {
    loop {
        let total: usize = widths.iter().sum();
        if total <= max_inner || widths.iter().all(|&w| w == 0) {
            return widths;
        }
        // Shrink the widest column by 1.
        if let Some((i, _)) = widths.iter().enumerate().max_by_key(|(_, &w)| w) {
            if widths[i] > 0 {
                widths[i] -= 1;
            } else {
                return widths;
            }
        }
    }
}
