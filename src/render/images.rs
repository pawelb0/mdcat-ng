// Copyright 2026 Pawel Boguszewski

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Image start/end event handling.
//!
//! Inline images are tricky: if the terminal supports an image protocol we
//! render the image and enter `RenderedImage` to suppress the alt text; if not,
//! we fall back to either an inline link (when ANSI styling is available) or a
//! reference-style numbered link.

use std::io::Write;

use crate::error::RenderResult as Result;

use pulldown_cmark::{CowStr, LinkType};
use tracing::{event, Level};

use super::state::{InlineAttrs, InlineState, StackedState, State, StateAndData, StateStack};
use super::write::write_styled;
use super::StateData;
use crate::references::UrlBase;
use crate::resources::ResourceUrlHandler;
use crate::terminal::capabilities::StyleCapability;
use crate::terminal::osc::set_link_url;
use crate::theme::CombineStyle;
use crate::{Environment, Settings};

/// Handle `Start(Image { ... })` inside an inline state.
///
/// Fields of the Image tag are passed explicitly so the caller can destructure
/// once at the match site.
#[allow(clippy::too_many_arguments)]
pub(super) fn start<'a, W: Write>(
    writer: &mut W,
    settings: &Settings,
    environment: &Environment,
    resource_handler: &dyn ResourceUrlHandler,
    stack: StateStack,
    state: InlineState,
    attrs: InlineAttrs,
    link_type: LinkType,
    dest_url: CowStr<'a>,
    title: CowStr<'a>,
    data: StateData<'a>,
) -> Result<StateAndData<StateData<'a>>> {
    let InlineAttrs {
        style,
        indent,
        quote_bar_cols,
    } = attrs.clone();
    let resolved_link = environment.resolve_reference(&dest_url);
    let image_state = match (settings.terminal_capabilities.image, resolved_link) {
        (Some(capability), Some(ref url)) => {
            let result = if settings.multiplexer == crate::Multiplexer::None {
                capability.image_protocol().write_inline_image(
                    writer,
                    &resource_handler,
                    url,
                    settings.terminal_size,
                )
            } else {
                // Buffer the protocol's output, then wrap it in DCS
                // passthrough so the multiplexer forwards it verbatim.
                let mut buffer = Vec::with_capacity(4096);
                let inner = capability.image_protocol().write_inline_image(
                    &mut buffer,
                    &resource_handler,
                    url,
                    settings.terminal_size,
                );
                inner.and_then(|()| settings.multiplexer.write_passthrough(writer, &buffer))
            };
            result
                .map_err(|error| {
                    event!(
                        Level::ERROR,
                        ?error,
                        %url,
                        "failed to render image with capability {:?}: {:#}",
                        capability,
                        error
                    );
                    error
                })
                .map(|()| StackedState::RenderedImage)
                .ok()
        }
        (None, Some(url)) => {
            if let InlineState::InlineLink = state {
                event!(Level::WARN, url = %url, "Terminal does not support images, want to render image as link but cannot: Already inside a link");
                None
            } else {
                event!(Level::INFO, url = %url, "Terminal does not support images, rendering image as link");
                match settings.terminal_capabilities.style {
                    Some(StyleCapability::Ansi) => {
                        set_link_url(writer, url, &environment.hostname)?;
                        Some(StackedState::Inline(
                            InlineState::InlineLink,
                            InlineAttrs {
                                indent,
                                style: settings.theme.image_link_style.on_top_of(&style),
                                quote_bar_cols: quote_bar_cols.clone(),
                            },
                        ))
                    }
                    None => None,
                }
            }
        }
        (_, None) => None,
    };

    let (image_state, data) = match image_state {
        Some(s) => (s, data),
        None => {
            event!(
                Level::WARN,
                "Rendering image {} as inline text, without link",
                dest_url
            );
            // Inside an inline link keep the link style; we cannot nest links,
            // so clarify that clicking the link follows the link target, not
            // the image.
            let style = if let InlineState::InlineLink = state {
                style
            } else {
                settings.theme.image_link_style.on_top_of(&style)
            };
            let s = StackedState::Inline(
                InlineState::InlineText,
                InlineAttrs {
                    style,
                    indent,
                    quote_bar_cols,
                },
            );
            (s, data.push_pending_link(link_type, dest_url, title))
        }
    };
    stack
        .push(StackedState::Inline(state, attrs))
        .current(image_state)
        .and_data(data)
        .ok()
}

/// Handle `End(TagEnd::Image)` inside `Inline(InlineText, _)` — this emits the
/// numbered reference placeholder for an image that couldn't be rendered.
pub(super) fn end_reference<'a, W: Write>(
    writer: &mut W,
    settings: &Settings,
    stack: StateStack,
    attrs: InlineAttrs,
    data: StateData<'a>,
) -> Result<StateAndData<StateData<'a>>> {
    let (data, link) = data.pop_pending_link();
    let (data, index) =
        data.add_link_reference(link.dest_url, link.title, settings.theme.image_link_style);
    write_styled(
        writer,
        &settings.terminal_capabilities,
        // Always colour the reference to make clear it points to an image.
        &settings.theme.image_link_style.on_top_of(&attrs.style),
        format!("[{index}]"),
    )?;
    stack.pop().and_data(data).ok()
}

/// Handle events while we are inside a `RenderedImage` — nested images push
/// another dummy `RenderedImage` so the stack bookkeeping stays correct; image
/// end pops; everything else is swallowed.
pub(super) fn handle_rendered_image<'a>(
    stack: StateStack,
    data: StateData<'a>,
    event: pulldown_cmark::Event<'a>,
) -> Result<StateAndData<StateData<'a>>> {
    use pulldown_cmark::Event::{End, Start};
    use pulldown_cmark::Tag::Image;
    use pulldown_cmark::TagEnd;
    match event {
        Start(Image { .. }) => stack
            .push(StackedState::RenderedImage)
            .current(StackedState::RenderedImage)
            .and_data(data)
            .ok(),
        End(TagEnd::Image) => stack.pop().and_data(data).ok(),
        _ => State::Stacked(stack, StackedState::RenderedImage)
            .and_data(data)
            .ok(),
    }
}
