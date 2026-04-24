// Copyright 2018-2020 Sebastian Wiesner <sebastian@swsnr.de>
// Copyright 2026 mdcat-ng contributors

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Markdown [`SourceParser`] — CommonMark + GFM via pulldown_cmark.

use pulldown_cmark::{Options, Parser};

use crate::events;
use crate::parse::SourceParser;

/// CommonMark + every GFM extension mdcat renders natively.
fn options() -> Options {
    Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TABLES
        | Options::ENABLE_SMART_PUNCTUATION
        | Options::ENABLE_GFM
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_DEFINITION_LIST
        | Options::ENABLE_WIKILINKS
        | Options::ENABLE_MATH
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS
        | Options::ENABLE_SUPERSCRIPT
        | Options::ENABLE_SUBSCRIPT
        | Options::ENABLE_HEADING_ATTRIBUTES
}

/// [`SourceParser`] wrapping pulldown_cmark.
#[derive(Debug, Default, Clone, Copy)]
pub struct MarkdownParser;

impl SourceParser for MarkdownParser {
    fn parse<'a>(&self, input: &'a str) -> Box<dyn Iterator<Item = events::Event<'a>> + 'a> {
        Box::new(Parser::new_ext(input, options()).map(events::Event::from))
    }
}

/// Generate `From<pd::Enum> for events::Enum` for enums whose variants
/// are all units and share names 1:1.
macro_rules! from_unit_enum {
    ($src:path, $dst:path, [$($variant:ident),+ $(,)?]) => {
        impl From<$src> for $dst {
            fn from(value: $src) -> Self {
                match value {
                    $(<$src>::$variant => <$dst>::$variant,)+
                }
            }
        }
    };
}

from_unit_enum!(
    pulldown_cmark::HeadingLevel,
    events::HeadingLevel,
    [H1, H2, H3, H4, H5, H6]
);
from_unit_enum!(
    pulldown_cmark::Alignment,
    events::Alignment,
    [None, Left, Center, Right]
);
from_unit_enum!(
    pulldown_cmark::BlockQuoteKind,
    events::BlockQuoteKind,
    [Note, Tip, Important, Warning, Caution]
);
from_unit_enum!(
    pulldown_cmark::MetadataBlockKind,
    events::MetadataBlockKind,
    [YamlStyle, PlusesStyle]
);

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
        use events::Tag as D;
        use pulldown_cmark::Tag::*;
        match tag {
            Paragraph => D::Paragraph,
            Heading {
                level,
                id,
                classes,
                attrs,
            } => D::Heading {
                level: level.into(),
                id,
                classes,
                attrs,
            },
            BlockQuote(kind) => D::BlockQuote(kind.map(Into::into)),
            CodeBlock(kind) => D::CodeBlock(kind.into()),
            HtmlBlock => D::HtmlBlock,
            List(start) => D::List(start),
            Item => D::Item,
            FootnoteDefinition(label) => D::FootnoteDefinition(label),
            DefinitionList => D::DefinitionList,
            DefinitionListTitle => D::DefinitionListTitle,
            DefinitionListDefinition => D::DefinitionListDefinition,
            Table(alignments) => D::Table(alignments.into_iter().map(Into::into).collect()),
            TableHead => D::TableHead,
            TableRow => D::TableRow,
            TableCell => D::TableCell,
            Emphasis => D::Emphasis,
            Strong => D::Strong,
            Strikethrough => D::Strikethrough,
            Superscript => D::Superscript,
            Subscript => D::Subscript,
            Link {
                link_type,
                dest_url,
                title,
                id,
            } => D::Link {
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
            } => D::Image {
                link_type: link_type.into(),
                dest_url,
                title,
                id,
            },
            MetadataBlock(kind) => D::MetadataBlock(kind.into()),
        }
    }
}

impl From<pulldown_cmark::TagEnd> for events::TagEnd {
    fn from(end: pulldown_cmark::TagEnd) -> Self {
        use events::TagEnd as D;
        use pulldown_cmark::TagEnd::*;
        match end {
            Paragraph => D::Paragraph,
            Heading(level) => D::Heading(level.into()),
            BlockQuote(kind) => D::BlockQuote(kind.map(Into::into)),
            CodeBlock => D::CodeBlock,
            HtmlBlock => D::HtmlBlock,
            List(ordered) => D::List(ordered),
            Item => D::Item,
            FootnoteDefinition => D::FootnoteDefinition,
            DefinitionList => D::DefinitionList,
            DefinitionListTitle => D::DefinitionListTitle,
            DefinitionListDefinition => D::DefinitionListDefinition,
            Table => D::Table,
            TableHead => D::TableHead,
            TableRow => D::TableRow,
            TableCell => D::TableCell,
            Emphasis => D::Emphasis,
            Strong => D::Strong,
            Strikethrough => D::Strikethrough,
            Superscript => D::Superscript,
            Subscript => D::Subscript,
            Link => D::Link,
            Image => D::Image,
            MetadataBlock(kind) => D::MetadataBlock(kind.into()),
        }
    }
}

impl<'a> From<pulldown_cmark::Event<'a>> for events::Event<'a> {
    fn from(event: pulldown_cmark::Event<'a>) -> Self {
        use events::Event as D;
        use pulldown_cmark::Event::*;
        match event {
            Start(tag) => D::Start(tag.into()),
            End(end) => D::End(end.into()),
            Text(s) => D::Text(s),
            Code(s) => D::Code(s),
            InlineMath(s) => D::InlineMath(s),
            DisplayMath(s) => D::DisplayMath(s),
            Html(s) => D::Html(s),
            InlineHtml(s) => D::InlineHtml(s),
            FootnoteReference(s) => D::FootnoteReference(s),
            SoftBreak => D::SoftBreak,
            HardBreak => D::HardBreak,
            Rule => D::Rule,
            TaskListMarker(checked) => D::TaskListMarker(checked),
        }
    }
}
