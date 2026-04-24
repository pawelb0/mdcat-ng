// Copyright 2018-2020 Sebastian Wiesner <sebastian@swsnr.de>

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! mdcat: render markdown to TTYs.
//!
//! This crate exposes both the command-line interface entry points and the core rendering
//! library (previously published as `pulldown-cmark-mdcat`). See [`push_tty`] for the main
//! library entry point, and [`process_file`] for the CLI-level helper that reads markdown
//! from a file and renders it to the given [`Output`].
//!
//! ## Features
//!
//! - `default` enables `svg` and `image-processing`.
//!
//! - `svg` includes support for rendering SVG images to PNG for terminals which do not support SVG
//!   images natively.  This feature adds a dependency on `resvg`.
//!
//! - `image-processing` enables processing of pixel images before rendering.  This feature adds
//!   a dependency on `image`.  If disabled mdcat will not be able to render inline images on some
//!   terminals, or render images incorrectly or at wrong sizes on other terminals.
//!
//!   Do not disable this feature unless you are sure that you won't use inline images, or accept
//!   incomplete rendering of images.  Please do not report issues with inline images with this
//!   feature disabled.

#![deny(warnings, missing_docs, clippy::all)]
#![forbid(unsafe_code)]

use std::fs::File;
use std::io::{stdin, BufWriter, Error, ErrorKind, Read, Result, Write};
use std::path::{Path, PathBuf};

use anyhow::Context;
use gethostname::gethostname;
use pulldown_cmark::{Event, Options, Parser};
use syntect::parsing::SyntaxSet;
use tracing::{event, instrument, Level};
use url::Url;

pub use crate::error::{RenderError, RenderResult};
pub use crate::render::{NoopObserver, RenderObserver};
pub use crate::resources::ResourceUrlHandler;
pub use crate::terminal::capabilities::TerminalCapabilities;
pub use crate::terminal::{Multiplexer, TerminalProgram, TerminalSize};
pub use crate::theme::Theme;

mod error;
pub mod events;
pub mod mdless;
mod references;
pub mod resources;
pub mod terminal;
mod theme;

mod render;

/// Argument parsing for mdcat.
pub mod args;
/// Shared CLI entry point for the `mdcat` and `mdless` binaries.
pub mod cli;
/// Output handling for mdcat.
pub mod output;

use crate::args::ResourceAccess;
use crate::output::Output;
use crate::resources::{CurlResourceHandler, DispatchingResourceHandler, FileResourceHandler};

/// Default read size limit for resources (100 MiB).
pub static DEFAULT_RESOURCE_READ_LIMIT: u64 = 104_857_600;

/// HTTP `User-Agent` header for remote resource fetches.
const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

/// CommonMark + the GFM extensions mdcat renders natively.
///
/// CommonMark is the core spec. Task lists, strikethrough, and pipe
/// tables come from GitHub Flavored Markdown. Smart punctuation
/// replaces straight quotes and `--`/`...` with typographic
/// equivalents at parse time. GFM alert blockquotes (`> [!NOTE]`,
/// `> [!WARNING]`, …) are tagged with a [`pulldown_cmark::BlockQuoteKind`]
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

/// Settings for markdown rendering.
#[derive(Debug)]
pub struct Settings<'a> {
    /// Capabilities of the terminal mdcat writes to.
    pub terminal_capabilities: TerminalCapabilities,
    /// The size of the terminal mdcat writes to.
    pub terminal_size: TerminalSize,
    /// Detected terminal multiplexer (tmux/screen), if any.
    ///
    /// When non-`None`, image protocol output is wrapped in DCS passthrough
    /// so the multiplexer forwards it to the real terminal.
    pub multiplexer: Multiplexer,
    /// Syntax set for syntax highlighting of code blocks.
    pub syntax_set: &'a SyntaxSet,
    /// Colour theme for mdcat
    pub theme: Theme,
    /// Wrap code-block lines that exceed the terminal width instead of
    /// overflowing the right border.
    pub wrap_code: bool,
}

/// The environment to render markdown in.
#[derive(Debug)]
pub struct Environment {
    /// The base URL to resolve relative URLs with.
    pub base_url: Url,
    /// The local host name.
    pub hostname: String,
}

impl Environment {
    /// Create an environment for the local host with the given `base_url`.
    ///
    /// Take the local hostname from `gethostname`.
    pub fn for_localhost(base_url: Url) -> Result<Self> {
        gethostname()
            .into_string()
            .map_err(|raw| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("gethostname() returned invalid unicode data: {raw:?}"),
                )
            })
            .map(|hostname| Environment { base_url, hostname })
    }

    /// Create an environment for a local directory.
    ///
    /// Convert the directory to a directory URL, and obtain the hostname from `gethostname`.
    ///
    /// `base_dir` must be an absolute path; return an IO error with `ErrorKind::InvalidInput`
    /// otherwise.
    pub fn for_local_directory<P: AsRef<Path>>(base_dir: &P) -> Result<Self> {
        Url::from_directory_path(base_dir)
            .map_err(|()| {
                Error::new(
                    ErrorKind::InvalidInput,
                    format!(
                        "Base directory {} must be an absolute path",
                        base_dir.as_ref().display()
                    ),
                )
            })
            .and_then(Self::for_localhost)
    }
}

/// Write markdown to a TTY.
///
/// Iterate over the Markdown AST `events`, format each one, and send the
/// result to `writer` using `settings` and `environment` to drive styling
/// and resource access.
///
/// `push_tty` tries to limit output to the configured terminal columns,
/// but does not guarantee staying within the column limit (long words,
/// images, and inline code can overflow).
///
/// Delegates to [`push_tty_with_observer`] with a [`NoopObserver`]. Callers
/// that need structural information about the rendered output — heading
/// positions, link ranges, and so on — should use that variant directly.
#[instrument(level = "debug", skip_all, fields(environment.hostname = environment.hostname.as_str(), environment.base_url = &environment.base_url.as_str()))]
pub fn push_tty<'a, 'e, W, I>(
    settings: &Settings,
    environment: &Environment,
    resource_handler: &dyn ResourceUrlHandler,
    writer: &'a mut W,
    events: I,
) -> RenderResult<()>
where
    I: Iterator<Item = Event<'e>>,
    W: Write,
{
    push_tty_with_observer(
        settings,
        environment,
        resource_handler,
        writer,
        events,
        &mut NoopObserver,
    )
}

/// Render Markdown to a TTY while handing every event to an observer.
///
/// Same semantics as [`push_tty`]. On each iteration the observer is
/// called with the output byte offset and the event about to be rendered,
/// so the observer can build a side-table of structural positions (which
/// heading starts at which output byte, where each link anchors, etc.).
///
/// The output writer is wrapped in a byte-counting adapter so the
/// observer sees exact cursor positions even when the underlying writer
/// performs partial writes.
pub fn push_tty_with_observer<'a, 'e, W, I, O>(
    settings: &Settings,
    environment: &Environment,
    resource_handler: &dyn ResourceUrlHandler,
    writer: &'a mut W,
    events: I,
    observer: &mut O,
) -> RenderResult<()>
where
    I: Iterator<Item = Event<'e>>,
    W: Write,
    O: RenderObserver + ?Sized,
{
    use render::*;

    let mut counted = CountingWriter::new(writer);
    let mut current = StateAndData(State::default(), StateData::default());
    for event in events {
        observer.on_event(counted.bytes(), &event);
        let StateAndData(state, data) = current;
        current = write_event(
            &mut counted,
            settings,
            environment,
            &resource_handler,
            state,
            data,
            event,
        )?;
    }
    let StateAndData(final_state, final_data) = current;
    finish(&mut counted, settings, environment, final_state, final_data)
}

/// Read input for `filename`.
///
/// If `filename` is `-` read from standard input, otherwise try to open and
/// read the given file.
pub fn read_input<T: AsRef<str>>(filename: T) -> anyhow::Result<(PathBuf, String)> {
    let cd = std::env::current_dir()?;
    let mut buffer = String::new();

    if filename.as_ref() == "-" {
        stdin().read_to_string(&mut buffer)?;
        Ok((cd, buffer))
    } else {
        let mut source = File::open(filename.as_ref())?;
        source.read_to_string(&mut buffer)?;
        let base_dir = cd
            .join(filename.as_ref())
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or(cd);
        Ok((base_dir, buffer))
    }
}

/// Process a single file.
///
/// Read from `filename` and render the contents to `output`.
#[instrument(skip(output, settings, resource_handler), level = "debug")]
pub fn process_file(
    filename: &str,
    settings: &Settings,
    access: ResourceAccess,
    resource_handler: &dyn ResourceUrlHandler,
    output: &mut Output,
) -> anyhow::Result<()> {
    let (base_dir, input) = read_input(filename)?;
    event!(
        Level::TRACE,
        "Read input, using {} as base directory",
        base_dir.display()
    );
    let env = Environment::for_local_directory(&base_dir)?;
    // Collect the event stream so the remote-image prefetch can run
    // before the render loop. On `--local` runs (the default) the
    // wrapper degenerates to a no-op passthrough.
    let events: Vec<_> = Parser::new_ext(&input, markdown_options()).collect();
    let caching = match access {
        ResourceAccess::Remote => resources::prefetch_and_wrap(
            &events,
            &env,
            USER_AGENT,
            DEFAULT_RESOURCE_READ_LIMIT,
            resource_handler,
        ),
        ResourceAccess::LocalOnly => {
            resources::CachingResourceHandler::passthrough(resource_handler)
        }
    };
    let resource_handler: &dyn ResourceUrlHandler = &caching;

    let mut sink = BufWriter::new(output.writer());
    let outcome = push_tty(
        settings,
        &env,
        resource_handler,
        &mut sink,
        events.into_iter(),
    )
    .and_then(|()| {
        event!(Level::TRACE, "Finished rendering, flushing output");
        sink.flush().map_err(RenderError::from)
    });
    match outcome {
        Ok(()) => Ok(()),
        Err(RenderError::Io(ref io)) if io.kind() == ErrorKind::BrokenPipe => {
            event!(Level::TRACE, "Ignoring broken pipe");
            Ok(())
        }
        Err(error) => {
            event!(Level::ERROR, ?error, "Failed to process file: {:#}", error);
            Err(error.into())
        }
    }
}

/// Create the resource handler for mdcat.
pub fn create_resource_handler(
    access: ResourceAccess,
) -> anyhow::Result<DispatchingResourceHandler> {
    let mut resource_handlers: Vec<Box<dyn ResourceUrlHandler>> = vec![Box::new(
        FileResourceHandler::new(DEFAULT_RESOURCE_READ_LIMIT),
    )];
    if let ResourceAccess::Remote = access {
        // libcurl's process-wide init runs here, not at CLI entry, so
        // `--local` invocations skip the cost entirely.
        curl::init();
        event!(target: "mdcat::main", Level::DEBUG, "HTTP client with user agent {USER_AGENT}");
        let client = CurlResourceHandler::create(DEFAULT_RESOURCE_READ_LIMIT, USER_AGENT)
            .context("build HTTP client")?;
        resource_handlers.push(Box::new(client));
    }
    Ok(DispatchingResourceHandler::new(resource_handlers))
}

#[cfg(test)]
mod tests {
    use pulldown_cmark::Parser;

    use crate::resources::NoopResourceHandler;

    use super::*;

    mod observer {
        use pulldown_cmark::{Event, Options, Parser, Tag};

        use super::*;

        /// Observer that records the `(kind, byte_offset)` of each
        /// interesting structural event. Used only in tests.
        #[derive(Default)]
        struct Recorder {
            entries: Vec<(String, u64)>,
        }

        impl RenderObserver for Recorder {
            fn on_event(&mut self, byte_offset: u64, event: &Event<'_>) {
                let kind = match event {
                    Event::Start(Tag::Heading { level, .. }) => format!("start_h{}", *level as u8),
                    Event::End(end) => format!("end_{end:?}"),
                    Event::Text(t) => format!("text:{t}"),
                    _ => return,
                };
                self.entries.push((kind, byte_offset));
            }
        }

        #[test]
        fn observer_sees_heading_events_with_increasing_offsets() {
            let markdown = "# First\n\n## Second\n\nbody\n";
            let parser = Parser::new_ext(markdown, Options::empty());
            let mut sink: Vec<u8> = Vec::new();
            let env =
                Environment::for_local_directory(&std::env::current_dir().expect("cwd available"))
                    .expect("env");
            let settings = Settings {
                terminal_capabilities: TerminalProgram::Dumb.capabilities(),
                terminal_size: TerminalSize::default(),
                multiplexer: Multiplexer::default(),
                syntax_set: &SyntaxSet::default(),
                theme: Theme::default(),
                wrap_code: false,
            };
            let mut recorder = Recorder::default();

            push_tty_with_observer(
                &settings,
                &env,
                &NoopResourceHandler,
                &mut sink,
                parser,
                &mut recorder,
            )
            .expect("render");

            // We expect to see both headings appear in order, the H1 before
            // the H2, and each heading's offset must be <= the offset of
            // the heading that follows it.
            let headings: Vec<_> = recorder
                .entries
                .iter()
                .filter(|(k, _)| k.starts_with("start_h"))
                .collect();
            assert_eq!(
                headings.len(),
                2,
                "saw {} headings in {:?}",
                headings.len(),
                recorder.entries
            );
            assert_eq!(headings[0].0, "start_h1");
            assert_eq!(headings[1].0, "start_h2");
            assert!(
                headings[0].1 <= headings[1].1,
                "H1 offset {} should not exceed H2 offset {}",
                headings[0].1,
                headings[1].1
            );
        }

        #[test]
        fn noop_observer_matches_plain_push_tty_byte_for_byte() {
            let markdown = "# Title\n\nSome *emphasis* and `code`.\n\n- item one\n- item two\n";
            let env =
                Environment::for_local_directory(&std::env::current_dir().expect("cwd available"))
                    .expect("env");
            let settings = Settings {
                terminal_capabilities: TerminalProgram::Ansi.capabilities(),
                terminal_size: TerminalSize::default(),
                multiplexer: Multiplexer::default(),
                syntax_set: &SyntaxSet::default(),
                theme: Theme::default(),
                wrap_code: false,
            };

            let mut plain: Vec<u8> = Vec::new();
            push_tty(
                &settings,
                &env,
                &NoopResourceHandler,
                &mut plain,
                Parser::new_ext(markdown, Options::empty()),
            )
            .expect("plain");

            let mut observed: Vec<u8> = Vec::new();
            push_tty_with_observer(
                &settings,
                &env,
                &NoopResourceHandler,
                &mut observed,
                Parser::new_ext(markdown, Options::empty()),
                &mut NoopObserver,
            )
            .expect("observed");

            // The observer hook must not perturb rendering; the two paths
            // produce identical output.
            assert_eq!(plain, observed);
        }
    }

    fn render_string(input: &str, settings: &Settings) -> RenderResult<String> {
        let source = Parser::new(input);
        let mut sink = Vec::new();
        let env =
            Environment::for_local_directory(&std::env::current_dir().expect("Working directory"))?;
        push_tty(settings, &env, &NoopResourceHandler, &mut sink, source)?;
        Ok(String::from_utf8_lossy(&sink).into())
    }

    fn render_string_dumb(markup: &str) -> RenderResult<String> {
        render_string(
            markup,
            &Settings {
                syntax_set: &SyntaxSet::default(),
                terminal_capabilities: TerminalProgram::Dumb.capabilities(),
                terminal_size: TerminalSize::default(),
                multiplexer: Multiplexer::default(),
                theme: Theme::default(),
                wrap_code: false,
            },
        )
    }

    mod layout {
        use super::render_string_dumb;
        use insta::assert_snapshot;

        #[test]
        #[allow(non_snake_case)]
        fn GH_49_format_no_colour_simple() {
            assert_eq!(
                render_string_dumb("_lorem_ **ipsum** dolor **sit** _amet_").unwrap(),
                "lorem ipsum dolor sit amet\n",
            )
        }

        #[test]
        fn begins_with_rule() {
            assert_snapshot!(render_string_dumb("----").unwrap())
        }

        #[test]
        fn begins_with_block_quote() {
            assert_snapshot!(render_string_dumb("> Hello World").unwrap());
        }

        #[test]
        fn rule_in_block_quote() {
            assert_snapshot!(render_string_dumb(
                "> Hello World

> ----"
            )
            .unwrap());
        }

        #[test]
        fn heading_in_block_quote() {
            assert_snapshot!(render_string_dumb(
                "> Hello World

> # Hello World"
            )
            .unwrap())
        }

        #[test]
        fn heading_levels() {
            assert_snapshot!(render_string_dumb(
                "
# First

## Second

### Third"
            )
            .unwrap())
        }

        #[test]
        fn autolink_creates_no_reference() {
            assert_eq!(
                render_string_dumb("Hello <http://example.com>").unwrap(),
                "Hello http://example.com\n"
            )
        }

        #[test]
        fn flush_ref_links_before_toplevel_heading() {
            assert_snapshot!(render_string_dumb(
                "> Hello [World](http://example.com/world)

> # No refs before this headline

# But before this"
            )
            .unwrap())
        }

        #[test]
        fn flush_ref_links_at_end() {
            assert_snapshot!(render_string_dumb(
                "Hello [World](http://example.com/world)

# Headline

Hello [Donald](http://example.com/Donald)"
            )
            .unwrap())
        }
    }

    mod disabled_features {
        use insta::assert_snapshot;

        use super::render_string_dumb;

        #[test]
        #[allow(non_snake_case)]
        fn GH_155_do_not_choke_on_footnotes() {
            assert_snapshot!(render_string_dumb(
                "A footnote [^1]

[^1: We do not support footnotes."
            )
            .unwrap())
        }
    }
}
