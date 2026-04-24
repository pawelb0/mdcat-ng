// Copyright 2026 mdcat-ng contributors

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Source-format parsing.
//!
//! A [`SourceParser`] takes a string slice and yields a stream of
//! [`crate::events::Event`]s for the renderer. Markdown ships as the one
//! implementation today ([`markdown::MarkdownParser`]); other wiki
//! dialects can be added by implementing the trait against the same
//! event vocabulary.

use crate::events::Event;

pub mod markdown;

/// Parse a document into the renderer's event stream.
///
/// Implementations are borrow-based: events reference the input string
/// directly through [`CowStr`](crate::events::CowStr), so the returned
/// iterator must not outlive `input`.
pub trait SourceParser {
    /// Parse `input` into a borrowed stream of [`Event`]s.
    fn parse<'a>(&self, input: &'a str) -> Box<dyn Iterator<Item = Event<'a>> + 'a>;
}
