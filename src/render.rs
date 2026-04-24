// Copyright Sebastian Wiesner <sebastian@swsnr.de>

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Rendering algorithm.

use std::io::prelude::*;

use crate::error::RenderResult as Result;

use anstyle::{Effects, Style};
use textwrap::core::display_width;

use crate::events::{Event, Event::*, Tag, Tag::*, TagEnd};
use tracing::{event, instrument, Level};

use crate::resources::ResourceUrlHandler;
use crate::theme::CombineStyle;
use crate::{Environment, Settings};

mod code;
mod counted;
mod data;
mod highlighting;
mod html;
mod images;
mod links;
mod observer;
mod state;
mod tables;
mod write;

use state::*;
use write::*;

use crate::render::data::{CurrentLine, CurrentTable};
use crate::render::state::MarginControl::NoMargin;
use crate::terminal::osc::clear_link;
pub use counted::CountingWriter;
pub use data::StateData;
pub use observer::{NoopObserver, RenderObserver};
pub use state::State;
pub use state::StateAndData;

#[instrument(level = "trace", skip(writer, settings, environment, resource_handler))]
pub fn write_event<'a, W: Write>(
    writer: &mut W,
    settings: &Settings,
    environment: &Environment,
    resource_handler: &dyn ResourceUrlHandler,
    state: State,
    data: StateData<'a>,
    event: Event<'a>,
) -> Result<StateAndData<StateData<'a>>> {
    use self::InlineState::*;
    use self::ListItemState::*;
    use self::StackedState::*;
    use State::*;

    event!(Level::TRACE, event = ?event, "rendering");
    match (state, event) {
        // Top level items
        (TopLevel(attrs), Start(Paragraph)) => {
            if attrs.margin_before != NoMargin {
                writeln!(writer)?;
            }
            State::stack_onto(TopLevelAttrs::margin_before())
                .current(Inline(InlineText, InlineAttrs::default()))
                .and_data(data)
                .ok()
        }
        (TopLevel(attrs), Start(Tag::HtmlBlock)) => {
            if attrs.margin_before != NoMargin {
                writeln!(writer)?;
            }
            // We render HTML literally
            State::stack_onto(TopLevelAttrs::margin_before())
                .current(
                    HtmlBlockAttrs {
                        indent: 0,
                        initial_indent: 0,
                        style: settings.theme.html_block_style,
                    }
                    .into(),
                )
                .and_data(data)
                .ok()
        }
        (TopLevel(attrs), Start(Heading { level, .. })) => {
            let (data, links) = data.take_link_references();
            write_link_refs(writer, environment, &settings.terminal_capabilities, links)?;
            if attrs.margin_before != NoMargin {
                writeln!(writer)?;
            }
            write_mark(writer, &settings.terminal_capabilities)?;

            State::stack_onto(TopLevelAttrs::margin_before())
                .current(write_start_heading(
                    writer,
                    &settings.terminal_capabilities,
                    settings.theme.heading_style,
                    level,
                )?)
                .and_data(data)
                .ok()
        }
        (TopLevel(attrs), Start(BlockQuote(kind))) => {
            if attrs.margin_before != NoMargin {
                writeln!(writer)?;
            }
            let quote_attrs = StyledBlockAttrs::default()
                .block_quote()
                .without_margin_before();
            if let Some(k) = kind {
                write_alert_label(
                    writer,
                    &settings.terminal_capabilities,
                    &settings.theme,
                    &quote_attrs,
                    k,
                )?;
            }
            State::stack_onto(TopLevelAttrs::margin_before())
                // We've written a block-level margin already, so the first
                // block inside the styled block should add another margin.
                .current(quote_attrs.into())
                .and_data(data)
                .ok()
        }
        (TopLevel(attrs), Rule) => {
            if attrs.margin_before != NoMargin {
                writeln!(writer)?;
            }
            write_rule(
                writer,
                &settings.terminal_capabilities,
                &settings.theme,
                settings.terminal_size.columns,
            )?;
            writeln!(writer)?;
            TopLevel(TopLevelAttrs::margin_before()).and_data(data).ok()
        }
        (TopLevel(attrs), Start(CodeBlock(kind))) => {
            if attrs.margin_before != NoMargin {
                writeln!(writer)?;
            }

            State::stack_onto(TopLevelAttrs::margin_before())
                .current(write_start_code_block(
                    writer,
                    settings,
                    0,
                    Style::new(),
                    kind,
                )?)
                .and_data(data)
                .ok()
        }
        (TopLevel(attrs), Start(List(start))) => {
            if attrs.margin_before != NoMargin {
                writeln!(writer)?;
            }
            let kind = start.map_or(ListItemKind::Unordered, |start| {
                ListItemKind::Ordered(start)
            });

            State::stack_onto(TopLevelAttrs::margin_before())
                .current(Inline(ListItem(kind, StartItem), InlineAttrs::default()))
                .and_data(data)
                .ok()
        }
        (TopLevel(attrs), Start(Table(alignments))) => {
            if attrs.margin_before != NoMargin {
                writeln!(writer)?;
            }
            let current_table = CurrentTable {
                alignments,
                ..data.current_table
            };
            let data = StateData {
                current_table,
                ..data
            };
            State::stack_onto(TopLevelAttrs::margin_before())
                .current(TableBlock)
                .and_data(data)
                .ok()
        }

        // Nested blocks with style, e.g. paragraphs in quotes, etc.
        (Stacked(stack, StyledBlock(attrs)), Start(Paragraph)) => {
            if attrs.margin_before != NoMargin {
                writeln!(writer)?;
            }
            write_line_start(
                writer,
                &settings.terminal_capabilities,
                &settings.theme,
                attrs.indent,
                attrs.quote_bar_cols.as_slice(),
            )?;
            let inline = InlineAttrs::from(&attrs);
            stack
                .push(attrs.with_margin_before().into())
                .current(Inline(InlineText, inline))
                .and_data(data)
                .ok()
        }
        (Stacked(stack, StyledBlock(attrs)), Start(Tag::HtmlBlock)) => {
            if attrs.margin_before != NoMargin {
                writeln!(writer)?;
            }
            let state = HtmlBlockAttrs {
                indent: attrs.indent,
                initial_indent: attrs.indent,
                style: settings.theme.html_block_style.on_top_of(&attrs.style),
            }
            .into();
            stack
                .push(attrs.with_margin_before().into())
                .current(state)
                .and_data(data)
                .ok()
        }
        (Stacked(stack, StyledBlock(attrs)), Start(BlockQuote(kind))) => {
            if attrs.margin_before != NoMargin {
                writeln!(writer)?;
            }
            let quote_attrs = attrs.clone().without_margin_before().block_quote();
            if let Some(k) = kind {
                write_alert_label(
                    writer,
                    &settings.terminal_capabilities,
                    &settings.theme,
                    &quote_attrs,
                    k,
                )?;
            }
            stack
                .push(attrs.with_margin_before().into())
                .current(quote_attrs.into())
                .and_data(data)
                .ok()
        }
        (Stacked(stack, StyledBlock(attrs)), Rule) => {
            if attrs.margin_before != NoMargin {
                writeln!(writer)?;
            }
            write_indent(writer, attrs.indent)?;
            write_rule(
                writer,
                &settings.terminal_capabilities,
                &settings.theme,
                settings.terminal_size.columns - attrs.indent,
            )?;
            writeln!(writer)?;
            stack
                .current(attrs.with_margin_before().into())
                .and_data(data)
                .ok()
        }
        (Stacked(stack, StyledBlock(attrs)), Start(Heading { level, .. })) => {
            if attrs.margin_before != NoMargin {
                writeln!(writer)?;
            }
            write_indent(writer, attrs.indent)?;

            // We deliberately don't mark headings which aren't top-level.
            let style = attrs.style;
            stack
                .push(attrs.with_margin_before().into())
                .current(write_start_heading(
                    writer,
                    &settings.terminal_capabilities,
                    settings.theme.heading_style.on_top_of(&style),
                    level,
                )?)
                .and_data(data)
                .ok()
        }
        (Stacked(stack, StyledBlock(attrs)), Start(List(start))) => {
            if attrs.margin_before != NoMargin {
                writeln!(writer)?;
            }
            let kind = start.map_or(ListItemKind::Unordered, |start| {
                ListItemKind::Ordered(start)
            });
            let inline = InlineAttrs::from(&attrs);
            stack
                .push(attrs.with_margin_before().into())
                .current(Inline(ListItem(kind, StartItem), inline))
                .and_data(data)
                .ok()
        }
        (Stacked(stack, StyledBlock(attrs)), Start(CodeBlock(kind))) => {
            if attrs.margin_before != NoMargin {
                writeln!(writer)?;
            }
            let StyledBlockAttrs { indent, style, .. } = attrs;
            stack
                .push(attrs.into())
                .current(write_start_code_block(
                    writer, settings, indent, style, kind,
                )?)
                .and_data(data)
                .ok()
        }

        // Lists
        (Stacked(stack, Inline(ListItem(kind, state), attrs)), Start(Item)) => {
            let InlineAttrs {
                indent,
                style,
                quote_bar_cols,
                ..
            } = attrs;
            if state == ItemBlock {
                // Add margin
                writeln!(writer)?;
            }
            write_line_start(
                writer,
                &settings.terminal_capabilities,
                &settings.theme,
                indent,
                quote_bar_cols.as_slice(),
            )?;
            let indent = match kind {
                ListItemKind::Unordered => {
                    write!(writer, "\u{2022} ")?;
                    indent + 2
                }
                ListItemKind::Ordered(no) => {
                    write!(writer, "{no:>2}. ")?;
                    indent + 4
                }
            };
            stack
                .current(Inline(
                    ListItem(kind, StartItem),
                    InlineAttrs {
                        style,
                        indent,
                        quote_bar_cols,
                    },
                ))
                .and_data(data.current_line(CurrentLine {
                    length: indent,
                    trailing_space: None,
                }))
                .ok()
        }
        (Stacked(stack, Inline(ListItem(kind, state), attrs)), Start(Paragraph)) => {
            if state != StartItem {
                // Write margin, unless we're at the start of the list item in which case the first line of the
                // paragraph should go right beside the item bullet.
                writeln!(writer)?;
                write_indent(writer, attrs.indent)?;
            }
            stack
                .push(Inline(ListItem(kind, ItemBlock), attrs.clone()))
                .current(Inline(InlineText, attrs))
                .and_data(data)
                .ok()
        }
        (Stacked(stack, Inline(ListItem(kind, state), attrs)), Start(Tag::HtmlBlock)) => {
            let InlineAttrs { indent, style, .. } = attrs;
            let initial_indent = if state == StartItem {
                0
            } else {
                writeln!(writer)?;
                indent
            };
            stack
                .push(Inline(ListItem(kind, ItemBlock), attrs))
                .current(
                    HtmlBlockAttrs {
                        style: settings.theme.html_block_style.on_top_of(&style),
                        indent,
                        initial_indent,
                    }
                    .into(),
                )
                .and_data(data)
                .ok()
        }
        (Stacked(stack, Inline(ListItem(kind, _), attrs)), Start(CodeBlock(ck))) => {
            writeln!(writer)?;
            let InlineAttrs { indent, style, .. } = attrs;
            stack
                .push(Inline(ListItem(kind, ItemBlock), attrs))
                .current(write_start_code_block(writer, settings, indent, style, ck)?)
                .and_data(data)
                .ok()
        }
        (Stacked(stack, Inline(ListItem(kind, _), attrs)), Rule) => {
            writeln!(writer)?;
            write_indent(writer, attrs.indent)?;
            write_rule(
                writer,
                &settings.terminal_capabilities,
                &settings.theme,
                settings.terminal_size.columns - attrs.indent,
            )?;
            writeln!(writer)?;
            stack
                .current(Inline(ListItem(kind, ItemBlock), attrs))
                .and_data(data)
                .ok()
        }
        (Stacked(stack, Inline(ListItem(kind, state), attrs)), Start(Heading { level, .. })) => {
            if state != StartItem {
                writeln!(writer)?;
                write_indent(writer, attrs.indent)?;
            }
            // We deliberately don't mark headings which aren't top-level.
            let style = attrs.style;
            stack
                .push(Inline(ListItem(kind, ItemBlock), attrs))
                .current(write_start_heading(
                    writer,
                    &settings.terminal_capabilities,
                    settings.theme.heading_style.on_top_of(&style),
                    level,
                )?)
                .and_data(data)
                .ok()
        }
        (Stacked(stack, Inline(ListItem(kind, _), attrs)), Start(List(start))) => {
            writeln!(writer)?;
            let nested_kind = start.map_or(ListItemKind::Unordered, |start| {
                ListItemKind::Ordered(start)
            });
            stack
                .push(Inline(ListItem(kind, ItemBlock), attrs.clone()))
                .current(Inline(ListItem(nested_kind, StartItem), attrs))
                .and_data(data)
                .ok()
        }
        (Stacked(stack, Inline(ListItem(kind, _), attrs)), Start(BlockQuote(qkind))) => {
            writeln!(writer)?;
            let block_quote = StyledBlockAttrs::from(&attrs)
                .without_margin_before()
                .block_quote();
            if let Some(k) = qkind {
                write_alert_label(
                    writer,
                    &settings.terminal_capabilities,
                    &settings.theme,
                    &block_quote,
                    k,
                )?;
            }
            stack
                .push(Inline(ListItem(kind, ItemBlock), attrs))
                .current(block_quote.into())
                .and_data(data)
                .ok()
        }
        (Stacked(stack, Inline(ListItem(kind, state), attrs)), End(TagEnd::Item)) => {
            let InlineAttrs {
                indent,
                style,
                quote_bar_cols,
                ..
            } = attrs;
            let data = if state == ItemBlock {
                data
            } else {
                // End the inline text of this item
                writeln!(writer)?;
                data.current_line(CurrentLine::empty())
            };
            // Decrease indent back to the level where we can write the next item bullet, and increment the list item number.
            let (indent, kind) = match kind {
                ListItemKind::Unordered => (indent - 2, ListItemKind::Unordered),
                ListItemKind::Ordered(no) => (indent - 4, ListItemKind::Ordered(no + 1)),
            };
            stack
                .current(Inline(
                    ListItem(kind, state),
                    InlineAttrs {
                        style,
                        indent,
                        quote_bar_cols,
                    },
                ))
                .and_data(data)
                .ok()
        }

        // Literal blocks without highlighting — see render::code.
        (Stacked(stack, LiteralBlock(attrs)), Text(text)) => {
            code::handle_literal_text(writer, settings, stack, attrs, text, data)
        }
        (Stacked(stack, LiteralBlock(attrs)), End(TagEnd::CodeBlock)) => {
            code::handle_literal_end(writer, settings, stack, data, attrs.indent)
        }
        // HTML block contents — see render::html.
        (Stacked(stack, HtmlBlock(attrs)), Text(text)) => {
            html::handle_text(writer, settings, stack, attrs, text, data)
        }
        (Stacked(stack, HtmlBlock(attrs)), Html(html)) => {
            html::handle_html(writer, settings, stack, attrs, html, data)
        }

        // Highlighted code blocks — see render::code.
        (Stacked(stack, HighlightBlock(attrs)), Text(text)) => {
            code::handle_highlight_text(writer, settings, stack, attrs, text, data)
        }
        (Stacked(stack, HighlightBlock(attrs)), End(TagEnd::CodeBlock)) => {
            code::handle_literal_end(writer, settings, stack, data, attrs.indent)
        }

        // Inline markup
        (Stacked(stack, Inline(state, attrs)), Start(Emphasis)) => {
            let indent = attrs.indent;
            let quote_bar_cols = attrs.quote_bar_cols.clone();
            let style = attrs.style;
            let effects = style.get_effects();
            let style =
                style.effects(effects.set(Effects::ITALIC, !effects.contains(Effects::ITALIC)));
            stack
                .push(Inline(state, attrs))
                .current(Inline(
                    state,
                    InlineAttrs {
                        style,
                        indent,
                        quote_bar_cols,
                    },
                ))
                .and_data(data)
                .ok()
        }
        (Stacked(stack, Inline(state, attrs)), Start(Strong)) => {
            let indent = attrs.indent;
            let quote_bar_cols = attrs.quote_bar_cols.clone();
            let style = attrs.style.bold();
            stack
                .push(Inline(state, attrs))
                .current(Inline(
                    state,
                    InlineAttrs {
                        style,
                        indent,
                        quote_bar_cols,
                    },
                ))
                .and_data(data)
                .ok()
        }
        (Stacked(stack, Inline(state, attrs)), Start(Strikethrough)) => {
            let indent = attrs.indent;
            let quote_bar_cols = attrs.quote_bar_cols.clone();
            let style = attrs.style.strikethrough();
            stack
                .push(Inline(state, attrs))
                .current(Inline(
                    state,
                    InlineAttrs {
                        style,
                        indent,
                        quote_bar_cols,
                    },
                ))
                .and_data(data)
                .ok()
        }
        (
            Stacked(stack, Inline(_, _)),
            End(TagEnd::Strong | TagEnd::Emphasis | TagEnd::Strikethrough),
        ) => stack.pop().and_data(data).ok(),
        (Stacked(stack, Inline(state, attrs)), Code(code)) => {
            let current_line = write_styled_and_wrapped(
                writer,
                &settings.terminal_capabilities,
                &settings.theme,
                &settings.theme.code_style.on_top_of(&attrs.style),
                settings.terminal_size.columns,
                attrs.indent,
                attrs.quote_bar_cols.as_slice(),
                data.current_line,
                code,
            )?;
            let data = StateData {
                current_line,
                ..data
            };
            Ok(stack.current(Inline(state, attrs)).and_data(data))
        }

        (Stacked(stack, Inline(state, attrs)), InlineHtml(html)) => {
            let current_line = write_styled_and_wrapped(
                writer,
                &settings.terminal_capabilities,
                &settings.theme,
                &settings.theme.inline_html_style.on_top_of(&attrs.style),
                settings.terminal_size.columns,
                attrs.indent,
                attrs.quote_bar_cols.as_slice(),
                data.current_line,
                html,
            )?;
            let data = StateData {
                current_line,
                ..data
            };
            Ok(stack.current(Inline(state, attrs)).and_data(data))
        }
        (Stacked(stack, Inline(inline, attrs)), TaskListMarker(checked)) => {
            let marker = if checked { "\u{2611}" } else { "\u{2610}" };
            write_styled(
                writer,
                &settings.terminal_capabilities,
                &attrs.style,
                marker,
            )?;
            let length = data.current_line.length + display_width(marker) as u16;
            Ok(stack
                .current(Inline(inline, attrs))
                .and_data(data.current_line(CurrentLine {
                    length,
                    trailing_space: Some(" ".to_owned()),
                })))
        }
        // Inline line breaks
        (Stacked(stack, Inline(state, attrs)), SoftBreak) => {
            let length = data.current_line.length;

            Ok(stack
                .current(Inline(state, attrs))
                .and_data(data.current_line(CurrentLine {
                    length,
                    trailing_space: Some(" ".to_owned()),
                })))
        }
        (Stacked(stack, Inline(state, attrs)), HardBreak) => {
            writeln!(writer)?;
            write_line_start(
                writer,
                &settings.terminal_capabilities,
                &settings.theme,
                attrs.indent,
                attrs.quote_bar_cols.as_slice(),
            )?;

            Ok(stack
                .current(Inline(state, attrs))
                .and_data(data.current_line(CurrentLine::empty())))
        }
        // Inline text
        (Stacked(stack, Inline(ListItem(kind, ItemBlock), attrs)), Text(text)) => {
            // Fresh text after a new block, so indent again.
            write_line_start(
                writer,
                &settings.terminal_capabilities,
                &settings.theme,
                attrs.indent,
                attrs.quote_bar_cols.as_slice(),
            )?;
            let current_line = write_styled_and_wrapped(
                writer,
                &settings.terminal_capabilities,
                &settings.theme,
                &attrs.style,
                settings.terminal_size.columns,
                attrs.indent,
                attrs.quote_bar_cols.as_slice(),
                data.current_line,
                text,
            )?;
            Ok(stack
                .current(Inline(ListItem(kind, ItemText), attrs))
                .and_data(StateData {
                    current_line,
                    ..data
                }))
        }
        // Inline blocks don't wrap
        (Stacked(stack, Inline(InlineBlock, attrs)), Text(text)) => {
            write_styled(writer, &settings.terminal_capabilities, &attrs.style, text)?;
            Ok(stack.current(Inline(InlineBlock, attrs)).and_data(data))
        }
        (Stacked(stack, Inline(state, attrs)), Text(text)) => {
            let current_line = write_styled_and_wrapped(
                writer,
                &settings.terminal_capabilities,
                &settings.theme,
                &attrs.style,
                settings.terminal_size.columns,
                attrs.indent,
                attrs.quote_bar_cols.as_slice(),
                data.current_line,
                text,
            )?;
            Ok(stack.current(Inline(state, attrs)).and_data(StateData {
                current_line,
                ..data
            }))
        }
        // Ending inline text
        (Stacked(stack, Inline(_, _)), End(TagEnd::Paragraph)) => {
            writeln!(writer)?;
            Ok(stack
                .pop()
                .and_data(data.current_line(CurrentLine::empty())))
        }
        (Stacked(stack, Inline(_, attrs)), End(TagEnd::Heading(level))) => {
            writeln!(writer)?;
            write_heading_rule(
                writer,
                &settings.terminal_capabilities,
                settings.theme.heading_style,
                level,
                attrs.indent,
                &settings.terminal_size,
            )?;
            Ok(stack
                .pop()
                .and_data(data.current_line(CurrentLine::empty())))
        }

        // Links — see render::links.
        (
            Stacked(stack, Inline(state, attrs)),
            Start(Link {
                link_type,
                dest_url,
                title,
                ..
            }),
        ) => links::start(
            writer,
            settings,
            environment,
            stack,
            state,
            attrs,
            link_type,
            dest_url,
            title,
            data,
        ),
        (Stacked(stack, Inline(InlineText, attrs)), End(TagEnd::Link)) => {
            links::end_reference(writer, settings, stack, attrs, data)
        }

        // Images — see render::images.
        (
            Stacked(stack, Inline(state, attrs)),
            Start(Image {
                dest_url,
                title,
                link_type,
                ..
            }),
        ) => images::start(
            writer,
            settings,
            environment,
            resource_handler,
            stack,
            state,
            attrs,
            link_type,
            dest_url,
            title,
            data,
        ),
        (Stacked(stack, Inline(InlineText, attrs)), End(TagEnd::Image)) => {
            images::end_reference(writer, settings, stack, attrs, data)
        }
        (Stacked(stack, RenderedImage), event) => images::handle_rendered_image(stack, data, event),

        // End any kind of inline link, either a proper link, or an image written out as inline link
        (Stacked(stack, Inline(InlineLink, _)), End(TagEnd::Link | TagEnd::Image)) => {
            clear_link(writer)?;
            stack.pop().and_data(data).ok()
        }

        // Tables — see render::tables.
        (Stacked(stack, TableBlock), event) => tables::handle(writer, settings, stack, data, event),

        // Footnote reference inside inline text: `[^label]` rendered
        // bold so it stands out without looking like a raw link.
        (Stacked(stack, Inline(istate, attrs)), FootnoteReference(label)) => {
            let style = attrs.style.bold();
            write_styled(
                writer,
                &settings.terminal_capabilities,
                &style,
                format!("[^{label}]"),
            )?;
            Ok(stack.current(Inline(istate, attrs)).and_data(data))
        }

        // Footnote definition block: write the label as a bold prefix
        // line, then stack an indented block so the body paragraphs
        // render under it. Works identically whether the definition
        // appears at the top level or nested inside another block.
        (TopLevel(attrs), Start(Tag::FootnoteDefinition(label))) => {
            if attrs.margin_before != NoMargin {
                writeln!(writer)?;
            }
            write_styled(
                writer,
                &settings.terminal_capabilities,
                &Style::new().bold(),
                format!("[^{label}]:"),
            )?;
            writeln!(writer)?;
            let body = StyledBlockAttrs::default().indented(4);
            State::stack_onto(TopLevelAttrs::margin_before())
                .current(body.into())
                .and_data(data)
                .ok()
        }
        (Stacked(stack, _), End(TagEnd::FootnoteDefinition)) => stack.pop().and_data(data).ok(),

        // Definition list container: each title renders bold on its
        // own line; each definition block is indented two columns.
        (TopLevel(attrs), Start(Tag::DefinitionList)) => {
            if attrs.margin_before != NoMargin {
                writeln!(writer)?;
            }
            State::stack_onto(TopLevelAttrs::margin_before())
                .current(StyledBlockAttrs::default().into())
                .and_data(data)
                .ok()
        }
        (Stacked(stack, _), End(TagEnd::DefinitionList)) => stack.pop().and_data(data).ok(),
        (Stacked(stack, StyledBlock(attrs)), Start(Tag::DefinitionListTitle)) => {
            let inline = InlineAttrs {
                style: attrs.style.bold(),
                indent: attrs.indent,
                quote_bar_cols: attrs.quote_bar_cols.clone(),
            };
            write_line_start(
                writer,
                &settings.terminal_capabilities,
                &settings.theme,
                attrs.indent,
                &attrs.quote_bar_cols,
            )?;
            stack
                .push(attrs.into())
                .current(Inline(InlineText, inline))
                .and_data(data)
                .ok()
        }
        (Stacked(stack, Inline(_, _)), End(TagEnd::DefinitionListTitle)) => {
            writeln!(writer)?;
            Ok(stack
                .pop()
                .and_data(data.current_line(CurrentLine::empty())))
        }
        (Stacked(stack, StyledBlock(attrs)), Start(Tag::DefinitionListDefinition)) => {
            // Definitions contain raw inline text (no Paragraph wrapping),
            // so we enter Inline state directly and draw the indent
            // ourselves before the first character.
            let body = attrs.clone().indented(2);
            let inline = InlineAttrs {
                style: body.style,
                indent: body.indent,
                quote_bar_cols: body.quote_bar_cols.clone(),
            };
            write_line_start(
                writer,
                &settings.terminal_capabilities,
                &settings.theme,
                body.indent,
                &body.quote_bar_cols,
            )?;
            stack
                .push(attrs.into())
                .current(Inline(InlineText, inline))
                .and_data(data)
                .ok()
        }
        (Stacked(stack, Inline(_, _)), End(TagEnd::DefinitionListDefinition)) => {
            writeln!(writer)?;
            Ok(stack
                .pop()
                .and_data(data.current_line(CurrentLine::empty())))
        }

        // Unconditional returns to previous states
        (Stacked(stack, _), End(TagEnd::BlockQuote(_) | TagEnd::List(_) | TagEnd::HtmlBlock)) => {
            stack.pop().and_data(data).ok()
        }

        // Events we don't recognise in this state.
        //
        // Historically this arm panicked. We trace-log and no-op instead so
        // future pulldown-cmark events (e.g. new tag kinds) degrade to
        // plain-text rendering rather than aborting the whole document. A
        // genuine state-machine bug will still show up as garbled output
        // during development.
        (s, e) => {
            event!(
                Level::DEBUG,
                state = ?s,
                event = ?e,
                "no handler for (state, event); skipping",
            );
            s.and_data(data).ok()
        }
    }
}

#[instrument(level = "trace", skip(writer, settings, environment))]
pub fn finish<W: Write>(
    writer: &mut W,
    settings: &Settings,
    environment: &Environment,
    state: State,
    data: StateData<'_>,
) -> Result<()> {
    match state {
        State::TopLevel(_) => {
            event!(
                Level::TRACE,
                "Writing {} pending link definitions",
                data.pending_link_definitions.len()
            );
            write_link_refs(
                writer,
                environment,
                &settings.terminal_capabilities,
                data.pending_link_definitions,
            )?;
            Ok(())
        }
        State::Stacked(..) => {
            panic!("Must finish in state TopLevel but got: {state:?}");
        }
    }
}
