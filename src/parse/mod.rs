// Copyright 2026 mdcat-ng contributors

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Source-format parsing.
//!
//! Markdown ships as the only implementation; other wiki dialects slot
//! in by implementing [`SourceParser`] against [`crate::events`].

use crate::events::Event;

pub mod markdown;

/// Parse a document into the renderer's event stream.
pub trait SourceParser {
    /// Parse `input` into events borrowing from it.
    fn parse<'a>(&self, input: &'a str) -> Box<dyn Iterator<Item = Event<'a>> + 'a>;
}
