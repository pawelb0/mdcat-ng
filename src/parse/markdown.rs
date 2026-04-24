// Copyright 2018-2020 Sebastian Wiesner <sebastian@swsnr.de>
// Copyright 2026 mdcat-ng contributors

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Markdown [`SourceParser`] implementation.
//!
//! Wraps [`pulldown_cmark::Parser`] and maps its events into the
//! renderer-facing [`crate::events`] vocabulary. All extensions that
//! mdcat renders natively are enabled; see [`markdown_options`].

use pulldown_cmark::{Options, Parser};

use crate::events;
use crate::parse::SourceParser;

/// CommonMark + the GFM extensions mdcat renders natively.
///
/// CommonMark is the core spec. Task lists, strikethrough, and pipe
/// tables come from GitHub Flavored Markdown. Smart punctuation
/// replaces straight quotes and `--`/`...` with typographic
/// equivalents at parse time. GFM alert blockquotes (`> [!NOTE]`,
/// `> [!WARNING]`, …) are tagged with a [`events::BlockQuoteKind`]
/// that the renderer surfaces as a coloured label. Footnotes,
/// definition lists, and wiki links are rendered inline with a
/// matching bottom-of-document footnote section.
pub fn markdown_options() -> Options {
    Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TABLES
        | Options::ENABLE_SMART_PUNCTUATION
        | Options::ENABLE_GFM
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_DEFINITION_LIST
        | Options::ENABLE_WIKILINKS
}

/// [`SourceParser`] that parses CommonMark + GFM via pulldown_cmark.
#[derive(Debug, Default, Clone, Copy)]
pub struct MarkdownParser;

impl SourceParser for MarkdownParser {
    fn parse<'a>(&self, input: &'a str) -> Box<dyn Iterator<Item = events::Event<'a>> + 'a> {
        Box::new(Parser::new_ext(input, markdown_options()).map(events::Event::from))
    }
}

impl From<pulldown_cmark::HeadingLevel> for events::HeadingLevel {
    fn from(level: pulldown_cmark::HeadingLevel) -> Self {
        use pulldown_cmark::HeadingLevel::*;
        match level {
            H1 => events::HeadingLevel::H1,
            H2 => events::HeadingLevel::H2,
            H3 => events::HeadingLevel::H3,
            H4 => events::HeadingLevel::H4,
            H5 => events::HeadingLevel::H5,
            H6 => events::HeadingLevel::H6,
        }
    }
}

impl From<pulldown_cmark::Alignment> for events::Alignment {
    fn from(alignment: pulldown_cmark::Alignment) -> Self {
        use pulldown_cmark::Alignment::*;
        match alignment {
            None => events::Alignment::None,
            Left => events::Alignment::Left,
            Center => events::Alignment::Center,
            Right => events::Alignment::Right,
        }
    }
}

impl From<pulldown_cmark::LinkType> for events::LinkType {
    fn from(ty: pulldown_cmark::LinkType) -> Self {
        use pulldown_cmark::LinkType::*;
        match ty {
            Inline => events::LinkType::Inline,
            Reference => events::LinkType::Reference,
            ReferenceUnknown => events::LinkType::ReferenceUnknown,
            Collapsed => events::LinkType::Collapsed,
            CollapsedUnknown => events::LinkType::CollapsedUnknown,
            Shortcut => events::LinkType::Shortcut,
            ShortcutUnknown => events::LinkType::ShortcutUnknown,
            Autolink => events::LinkType::Autolink,
            Email => events::LinkType::Email,
            WikiLink { has_pothole } => events::LinkType::WikiLink { has_pothole },
        }
    }
}

impl From<pulldown_cmark::BlockQuoteKind> for events::BlockQuoteKind {
    fn from(kind: pulldown_cmark::BlockQuoteKind) -> Self {
        use pulldown_cmark::BlockQuoteKind::*;
        match kind {
            Note => events::BlockQuoteKind::Note,
            Tip => events::BlockQuoteKind::Tip,
            Important => events::BlockQuoteKind::Important,
            Warning => events::BlockQuoteKind::Warning,
            Caution => events::BlockQuoteKind::Caution,
        }
    }
}

impl From<pulldown_cmark::MetadataBlockKind> for events::MetadataBlockKind {
    fn from(kind: pulldown_cmark::MetadataBlockKind) -> Self {
        use pulldown_cmark::MetadataBlockKind::*;
        match kind {
            YamlStyle => events::MetadataBlockKind::YamlStyle,
            PlusesStyle => events::MetadataBlockKind::PlusesStyle,
        }
    }
}

impl<'a> From<pulldown_cmark::CodeBlockKind<'a>> for events::CodeBlockKind<'a> {
    fn from(kind: pulldown_cmark::CodeBlockKind<'a>) -> Self {
        match kind {
            pulldown_cmark::CodeBlockKind::Indented => events::CodeBlockKind::Indented,
            pulldown_cmark::CodeBlockKind::Fenced(name) => events::CodeBlockKind::Fenced(name),
        }
    }
}

impl<'a> From<pulldown_cmark::Tag<'a>> for events::Tag<'a> {
    fn from(tag: pulldown_cmark::Tag<'a>) -> Self {
        use pulldown_cmark::Tag::*;
        match tag {
            Paragraph => events::Tag::Paragraph,
            Heading {
                level,
                id,
                classes,
                attrs,
            } => events::Tag::Heading {
                level: level.into(),
                id,
                classes,
                attrs,
            },
            BlockQuote(kind) => events::Tag::BlockQuote(kind.map(Into::into)),
            CodeBlock(kind) => events::Tag::CodeBlock(kind.into()),
            HtmlBlock => events::Tag::HtmlBlock,
            List(start) => events::Tag::List(start),
            Item => events::Tag::Item,
            FootnoteDefinition(label) => events::Tag::FootnoteDefinition(label),
            DefinitionList => events::Tag::DefinitionList,
            DefinitionListTitle => events::Tag::DefinitionListTitle,
            DefinitionListDefinition => events::Tag::DefinitionListDefinition,
            Table(alignments) => {
                events::Tag::Table(alignments.into_iter().map(Into::into).collect())
            }
            TableHead => events::Tag::TableHead,
            TableRow => events::Tag::TableRow,
            TableCell => events::Tag::TableCell,
            Emphasis => events::Tag::Emphasis,
            Strong => events::Tag::Strong,
            Strikethrough => events::Tag::Strikethrough,
            Superscript => events::Tag::Superscript,
            Subscript => events::Tag::Subscript,
            Link {
                link_type,
                dest_url,
                title,
                id,
            } => events::Tag::Link {
                link_type: link_type.into(),
                dest_url,
                title,
                id,
            },
            Image {
                link_type,
                dest_url,
                title,
                id,
            } => events::Tag::Image {
                link_type: link_type.into(),
                dest_url,
                title,
                id,
            },
            MetadataBlock(kind) => events::Tag::MetadataBlock(kind.into()),
        }
    }
}

impl From<pulldown_cmark::TagEnd> for events::TagEnd {
    fn from(end: pulldown_cmark::TagEnd) -> Self {
        use pulldown_cmark::TagEnd::*;
        match end {
            Paragraph => events::TagEnd::Paragraph,
            Heading(level) => events::TagEnd::Heading(level.into()),
            BlockQuote(kind) => events::TagEnd::BlockQuote(kind.map(Into::into)),
            CodeBlock => events::TagEnd::CodeBlock,
            HtmlBlock => events::TagEnd::HtmlBlock,
            List(ordered) => events::TagEnd::List(ordered),
            Item => events::TagEnd::Item,
            FootnoteDefinition => events::TagEnd::FootnoteDefinition,
            DefinitionList => events::TagEnd::DefinitionList,
            DefinitionListTitle => events::TagEnd::DefinitionListTitle,
            DefinitionListDefinition => events::TagEnd::DefinitionListDefinition,
            Table => events::TagEnd::Table,
            TableHead => events::TagEnd::TableHead,
            TableRow => events::TagEnd::TableRow,
            TableCell => events::TagEnd::TableCell,
            Emphasis => events::TagEnd::Emphasis,
            Strong => events::TagEnd::Strong,
            Strikethrough => events::TagEnd::Strikethrough,
            Superscript => events::TagEnd::Superscript,
            Subscript => events::TagEnd::Subscript,
            Link => events::TagEnd::Link,
            Image => events::TagEnd::Image,
            MetadataBlock(kind) => events::TagEnd::MetadataBlock(kind.into()),
        }
    }
}

impl<'a> From<pulldown_cmark::Event<'a>> for events::Event<'a> {
    fn from(event: pulldown_cmark::Event<'a>) -> Self {
        use pulldown_cmark::Event::*;
        match event {
            Start(tag) => events::Event::Start(tag.into()),
            End(end) => events::Event::End(end.into()),
            Text(s) => events::Event::Text(s),
            Code(s) => events::Event::Code(s),
            InlineMath(s) => events::Event::InlineMath(s),
            DisplayMath(s) => events::Event::DisplayMath(s),
            Html(s) => events::Event::Html(s),
            InlineHtml(s) => events::Event::InlineHtml(s),
            FootnoteReference(s) => events::Event::FootnoteReference(s),
            SoftBreak => events::Event::SoftBreak,
            HardBreak => events::Event::HardBreak,
            Rule => events::Event::Rule,
            TaskListMarker(checked) => events::Event::TaskListMarker(checked),
        }
    }
}
