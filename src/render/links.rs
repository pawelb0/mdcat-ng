// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Link start/end event handling.
//!
//! When the terminal supports ANSI styling we emit OSC 8 hyperlinks inline;
//! otherwise we queue a pending link and render a numbered reference at the
//! end of the link text.

use std::io::Write;

use crate::error::RenderResult as Result;

use url::Url;

use crate::events::{CowStr, LinkType};

use super::data::CurrentLine;
use super::state::{InlineAttrs, InlineState, StackedState, StateAndData, StateStack};
use super::write::write_styled;
use super::StateData;
use crate::references::UrlBase;
use crate::terminal::capabilities::StyleCapability;
use crate::terminal::osc::set_link_url;
use crate::theme::CombineStyle;
use crate::{Environment, Settings};

/// Handle `Start(Link { ... })` inside an inline state.
#[allow(clippy::too_many_arguments)]
pub(super) fn start<'a, W: Write>(
    writer: &mut W,
    settings: &Settings,
    environment: &Environment,
    stack: StateStack,
    state: InlineState,
    attrs: InlineAttrs,
    link_type: LinkType,
    dest_url: CowStr<'a>,
    title: CowStr<'a>,
    data: StateData<'a>,
) -> Result<StateAndData<StateData<'a>>> {
    let maybe_link = settings
        .terminal_capabilities
        .style
        .filter(|s| *s == StyleCapability::Ansi)
        .and_then(|_| {
            if let LinkType::Email = link_type {
                // Turn email autolinks (i.e. <foo@example.com>) into mailto
                // inline links.
                Url::parse(&format!("mailto:{dest_url}")).ok()
            } else {
                environment.resolve_reference(&dest_url)
            }
        });

    let (link_state, data) = match maybe_link {
        None => (
            InlineState::InlineText,
            data.push_pending_link(link_type, dest_url, title),
        ),
        Some(url) => {
            let data = match data.current_line.trailing_space.as_ref() {
                Some(space) => {
                    // Flush trailing space before starting a link.
                    write!(writer, "{space}")?;
                    let length = data.current_line.length + 1;
                    data.current_line(CurrentLine {
                        length,
                        trailing_space: None,
                    })
                }
                None => data,
            };
            set_link_url(writer, url, &environment.hostname)?;
            (InlineState::InlineLink, data)
        }
    };

    let InlineAttrs {
        style,
        indent,
        quote_bar_cols,
    } = attrs.clone();
    stack
        .push(StackedState::Inline(state, attrs))
        .current(StackedState::Inline(
            link_state,
            InlineAttrs {
                indent,
                style: settings.theme.link_style.on_top_of(&style),
                quote_bar_cols,
            },
        ))
        .and_data(data)
        .ok()
}

/// Handle `End(TagEnd::Link)` inside `Inline(InlineText, _)` — emits the
/// numbered reference placeholder for links the terminal couldn't render
/// inline.
pub(super) fn end_reference<'a, W: Write>(
    writer: &mut W,
    settings: &Settings,
    stack: StateStack,
    attrs: InlineAttrs,
    data: StateData<'a>,
) -> Result<StateAndData<StateData<'a>>> {
    let (data, link) = data.pop_pending_link();
    match link.link_type {
        LinkType::Autolink | LinkType::Email => {
            // For email and autolinks the text is identical to the URL and was
            // already written when the link text was rendered.
            stack.pop().and_data(data).ok()
        }
        _ => {
            let (data, index) =
                data.add_link_reference(link.dest_url, link.title, settings.theme.link_style);
            write_styled(
                writer,
                &settings.terminal_capabilities,
                &settings.theme.link_style.on_top_of(&attrs.style),
                format!("[{index}]"),
            )?;
            stack.pop().and_data(data).ok()
        }
    }
}
