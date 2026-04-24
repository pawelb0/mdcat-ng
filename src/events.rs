// Copyright Sebastian Wiesner <sebastian@swsnr.de>
// Copyright 2026 mdcat-ng contributors

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Document events consumed by the renderer.
//!
//! The renderer no longer names pulldown_cmark types directly. Any source
//! parser produces an iterator of [`Event`] through [`crate::parse::SourceParser`];
//! markdown is one implementation. The enum is currently a 1:1 shape match
//! for pulldown_cmark 0.13's `Event`/`Tag`/`TagEnd` so adding a second
//! format can extend this enum without rewriting the render state machine.
//!
//! [`CowStr`] is re-exported from pulldown_cmark: it is a plain
//! copy-on-write string representation with no CommonMark-specific
//! behaviour.

#![allow(missing_docs)]

pub use pulldown_cmark::CowStr;

/// Language-tagged code block kind.
#[derive(Clone, Debug, PartialEq)]
pub enum CodeBlockKind<'a> {
    Indented,
    Fenced(CowStr<'a>),
}

/// GFM alert kind carried on a block quote.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BlockQuoteKind {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

/// Document metadata block kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MetadataBlockKind {
    YamlStyle,
    PlusesStyle,
}

/// Heading level 1..=6.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum HeadingLevel {
    H1 = 1,
    H2,
    H3,
    H4,
    H5,
    H6,
}

/// Table column alignment.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Alignment {
    None,
    Left,
    Center,
    Right,
}

/// Link classification from the source document.
#[derive(Clone, Debug, PartialEq, Copy)]
pub enum LinkType {
    Inline,
    Reference,
    ReferenceUnknown,
    Collapsed,
    CollapsedUnknown,
    Shortcut,
    ShortcutUnknown,
    Autolink,
    Email,
    WikiLink { has_pothole: bool },
}

/// Start of a tagged element. Balanced with a corresponding [`TagEnd`].
#[derive(Clone, Debug, PartialEq)]
pub enum Tag<'a> {
    Paragraph,
    Heading {
        level: HeadingLevel,
        id: Option<CowStr<'a>>,
        classes: Vec<CowStr<'a>>,
        attrs: Vec<(CowStr<'a>, Option<CowStr<'a>>)>,
    },
    BlockQuote(Option<BlockQuoteKind>),
    CodeBlock(CodeBlockKind<'a>),
    HtmlBlock,
    List(Option<u64>),
    Item,
    FootnoteDefinition(CowStr<'a>),
    DefinitionList,
    DefinitionListTitle,
    DefinitionListDefinition,
    Table(Vec<Alignment>),
    TableHead,
    TableRow,
    TableCell,
    Emphasis,
    Strong,
    Strikethrough,
    Superscript,
    Subscript,
    Link {
        link_type: LinkType,
        dest_url: CowStr<'a>,
        title: CowStr<'a>,
        id: CowStr<'a>,
    },
    Image {
        link_type: LinkType,
        dest_url: CowStr<'a>,
        title: CowStr<'a>,
        id: CowStr<'a>,
    },
    MetadataBlock(MetadataBlockKind),
}

/// End of a tagged element.
///
/// Kept small (two bytes on 64-bit targets) so the [`Event`] enum stays cheap
/// to clone; mirrors pulldown_cmark's `TagEnd` split.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum TagEnd {
    Paragraph,
    Heading(HeadingLevel),
    BlockQuote(Option<BlockQuoteKind>),
    CodeBlock,
    HtmlBlock,
    /// `true` for ordered lists.
    List(bool),
    Item,
    FootnoteDefinition,
    DefinitionList,
    DefinitionListTitle,
    DefinitionListDefinition,
    Table,
    TableHead,
    TableRow,
    TableCell,
    Emphasis,
    Strong,
    Strikethrough,
    Superscript,
    Subscript,
    Link,
    Image,
    MetadataBlock(MetadataBlockKind),
}

/// A single document event produced by a [`SourceParser`](crate::parse::SourceParser).
#[derive(Clone, Debug, PartialEq)]
pub enum Event<'a> {
    Start(Tag<'a>),
    End(TagEnd),
    Text(CowStr<'a>),
    Code(CowStr<'a>),
    InlineMath(CowStr<'a>),
    DisplayMath(CowStr<'a>),
    Html(CowStr<'a>),
    InlineHtml(CowStr<'a>),
    FootnoteReference(CowStr<'a>),
    SoftBreak,
    HardBreak,
    Rule,
    TaskListMarker(bool),
}
