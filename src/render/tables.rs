// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Table rendering state transitions.
//!
//! Every event that occurs while the top of the state stack is `TableBlock` is
//! routed through [`handle`]. Cell contents are accumulated in
//! [`StateData::current_table`] until the `TableEnd` event, at which point the
//! whole table is rendered to the writer.

use std::io::Write;

use crate::error::RenderResult as Result;

use crate::events::Event::{Code, End, InlineHtml, Start, Text};
use crate::events::Tag::{
    Emphasis, Image, Link, Strikethrough, Strong, TableCell, TableHead, TableRow,
};
use crate::events::{Event, TagEnd};

use super::data::{self, StateData};
use super::state::{StackedState, State, StateAndData, StateStack};
use super::write::write_table;
use crate::Settings;

/// Dispatch a single event while we are inside a `TableBlock`.
///
/// Returns the next `StateAndData` the outer dispatcher expects.
pub(super) fn handle<'a, W: Write>(
    writer: &mut W,
    settings: &Settings,
    stack: StateStack,
    data: StateData<'a>,
    event: Event<'a>,
) -> Result<StateAndData<StateData<'a>>> {
    match event {
        Start(TableHead) | Start(TableRow) | Start(TableCell) => {
            State::Stacked(stack, StackedState::TableBlock)
                .and_data(data)
                .ok()
        }
        End(TagEnd::TableHead) => {
            let current_table = data.current_table.end_head();
            let data = StateData {
                current_table,
                ..data
            };
            State::Stacked(stack, StackedState::TableBlock)
                .and_data(data)
                .ok()
        }
        End(TagEnd::TableRow) => {
            let current_table = data.current_table.end_row();
            let data = StateData {
                current_table,
                ..data
            };
            State::Stacked(stack, StackedState::TableBlock)
                .and_data(data)
                .ok()
        }
        End(TagEnd::TableCell) => {
            let current_table = data.current_table.end_cell();
            let data = StateData {
                current_table,
                ..data
            };
            State::Stacked(stack, StackedState::TableBlock)
                .and_data(data)
                .ok()
        }
        Text(text) | Code(text) => {
            let current_table = data.current_table.push_fragment(text);
            let data = StateData {
                current_table,
                ..data
            };
            State::Stacked(stack, StackedState::TableBlock)
                .and_data(data)
                .ok()
        }
        End(TagEnd::Table) => {
            write_table(
                writer,
                &settings.terminal_capabilities,
                &settings.theme,
                &settings.terminal_size,
                data.current_table,
            )?;
            let current_table = data::CurrentTable::empty();
            let data = StateData {
                current_table,
                ..data
            };
            stack.pop().and_data(data).ok()
        }
        // Ignore styled fragments inside a table cell; styled markup within
        // cells is tracked separately in https://github.com/swsnr/mdcat/issues
        // (formerly a TODO in render/data.rs).
        Start(Emphasis)
        | Start(Strong)
        | Start(Strikethrough)
        | Start(Link { .. })
        | Start(Image { .. })
        | End(TagEnd::Emphasis)
        | End(TagEnd::Strong)
        | End(TagEnd::Strikethrough)
        | End(TagEnd::Link)
        | End(TagEnd::Image)
        | InlineHtml(_) => State::Stacked(stack, StackedState::TableBlock)
            .and_data(data)
            .ok(),
        other => panic!("Event {other:?} impossible in state TableBlock"),
    }
}
