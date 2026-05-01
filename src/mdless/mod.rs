// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Interactive `mdless` viewer.
//!
//! [`run`] renders the document once with `push_tty`, enters the
//! alternate screen, and loops on crossterm events. Submodules
//! carry the real work: [`buffer`], [`keys`], [`view`], [`search`],
//! [`highlight`], [`toc`].

pub mod buffer;
pub mod highlight;
pub mod keys;
pub mod search;
pub mod toc;
pub mod view;

use std::collections::HashMap;
use std::io::{self, BufWriter, Write};
use std::path::Path;

use anyhow::{Context, Result};
use crossterm::cursor::{Hide, Show};
use crossterm::event::{self, Event};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, size, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::{execute, queue};

use buffer::{build, HeadingRecorder, RenderedDoc};
use keys::{Command, Decoder, SearchDirection};
use search::{CaseMode, Direction, SearchState};
use toc::Toc;
use view::View;

use crate::args::CommonArgs;
use crate::resources::ResourceUrlHandler;
use crate::terminal::capabilities::TerminalCapabilities;
use crate::{
    read_input, Environment, Multiplexer, Preset, Settings, SourceParser, TerminalProgram, Theme,
};

/// Options passed from the `mdless` CLI into the pager session.
#[derive(Debug, Clone, Default)]
pub struct MdlessOptions {
    /// Pattern to commit before the interactive loop starts.
    pub initial: Option<String>,
    /// Force case-sensitive matching.
    pub case_sensitive: bool,
    /// Interpret the pattern as a regex.
    pub regex: bool,
    /// Show a rendered-line number gutter on the left edge.
    pub line_numbers: bool,
}

/// Mutable pager state held across one session.
struct Session {
    doc: RenderedDoc,
    view: View,
    decoder: Decoder,
    /// Compiled search + matches from the last committed query.
    search: Option<SearchState>,
    /// Currently-active direction for `n` / `N` cycling.
    direction: SearchDirection,
    /// Pattern being typed. Empty unless the decoder is in search mode.
    input: String,
    /// Transient status text (search prompt, no-matches, error). Takes
    /// precedence over the default "line N/M" indicator for one frame.
    status: Option<String>,
    /// Regex mode + case mode for pattern compilation; populated from CLI.
    regex: bool,
    case: CaseMode,
    /// `Some` while the TOC modal owns the frame, `None` otherwise.
    toc: Option<Toc>,
    /// Named bookmark registers: `m a` stores, `'a` recalls. Lines are
    /// rendered-line indexes so bookmarks survive search jumps.
    bookmarks: HashMap<char, usize>,
}

impl Session {
    fn matches(&self) -> &[search::Match] {
        self.search.as_ref().map_or(&[][..], SearchState::all)
    }

    fn current_match(&self) -> Option<&search::Match> {
        self.search.as_ref().and_then(SearchState::current)
    }
}

/// Render `filename` and drive the interactive pager until the user quits.
///
/// Returns `0` on normal exit, non-zero on fatal errors (propagated via
/// [`anyhow::Result`] so the caller decides exit code / stderr shape).
pub fn run(
    filename: &str,
    parser: &dyn SourceParser,
    common: &CommonArgs,
    opts: MdlessOptions,
    resource_handler: &dyn ResourceUrlHandler,
) -> Result<i32> {
    let doc = render_doc(
        filename,
        parser,
        common,
        opts.line_numbers,
        resource_handler,
    )?;
    let (cols, rows) = size().unwrap_or((80, 24));

    let mut session = Session {
        doc,
        view: View::new(cols, rows).with_line_numbers(opts.line_numbers),
        decoder: Decoder::default(),
        search: None,
        direction: SearchDirection::Forward,
        input: String::new(),
        status: None,
        regex: opts.regex,
        case: if opts.case_sensitive {
            CaseMode::Sensitive
        } else {
            CaseMode::Smart
        },
        toc: None,
        bookmarks: HashMap::new(),
    };

    // Honour --search: commit the pattern before the event loop starts.
    if let Some(initial) = opts.initial {
        apply_search(&mut session, initial);
    }

    let _guard = TerminalGuard::enter()?;
    let mut out = BufWriter::new(io::stdout());
    draw(&session, &mut out)?;

    loop {
        match event::read()? {
            Event::Key(key) if key.kind == event::KeyEventKind::Press => {
                let cmd = session.decoder.feed(key);
                if dispatch_cmd(&mut session, cmd) {
                    break;
                }
                draw(&session, &mut out)?;
            }
            Event::Resize(cols, rows) => {
                session.view.resize(cols, rows, &session.doc);
                draw(&session, &mut out)?;
            }
            _ => {}
        }
    }
    Ok(0)
}

/// Act on one decoded [`Command`]. Returns `true` when the pager should quit.
fn dispatch_cmd(s: &mut Session, cmd: Command) -> bool {
    if s.toc.is_some() {
        return dispatch_toc(s, cmd);
    }
    match cmd {
        Command::Noop => false,
        Command::Quit => true,
        Command::Redraw => false,
        Command::BeginSearch(dir) => {
            s.direction = dir;
            s.input.clear();
            s.status = Some(prompt_for(dir).to_string());
            false
        }
        Command::SearchChar(c) => {
            s.input.push(c);
            s.status = Some(format!("{}{}", prompt_for(s.direction), s.input));
            false
        }
        Command::SearchBackspace => {
            s.input.pop();
            s.status = Some(format!("{}{}", prompt_for(s.direction), s.input));
            false
        }
        Command::SearchCommit => {
            let pattern = std::mem::take(&mut s.input);
            if pattern.is_empty() {
                s.status = None;
            } else {
                apply_search(s, pattern);
            }
            false
        }
        Command::SearchCancel | Command::ClearHighlights => {
            s.search = None;
            s.input.clear();
            s.status = None;
            false
        }
        Command::SearchNext => {
            step_search(s, Direction::Forward);
            false
        }
        Command::SearchPrev => {
            step_search(s, Direction::Backward);
            false
        }
        Command::NextHeading => {
            jump_heading(s, Direction::Forward);
            false
        }
        Command::PrevHeading => {
            jump_heading(s, Direction::Backward);
            false
        }
        Command::ToggleToc => {
            s.toc = Some(Toc::new(&s.doc.headings));
            s.status = None;
            false
        }
        Command::SetBookmark(c) => {
            s.bookmarks.insert(c, s.view.top);
            s.status = Some(format!("bookmark {c} set at line {}", s.view.top + 1));
            false
        }
        Command::JumpBookmark(c) => {
            if let Some(&line) = s.bookmarks.get(&c) {
                s.view.jump_to(line, &s.doc);
                s.status = None;
            } else {
                s.status = Some(format!("no bookmark `{c}`"));
            }
            false
        }
        Command::ToggleLineNumbers => {
            s.view.line_numbers = !s.view.line_numbers;
            false
        }
        // `Enter` outside the TOC currently has no binding; ignore it.
        Command::TocActivate => false,
        other => {
            s.view.apply(other, &s.doc);
            false
        }
    }
}

/// Handle keys while the TOC modal is open.
///
/// Returns `true` when the quit command was issued. The modal consumes
/// navigation keystrokes for selection; `Enter` jumps to the highlighted
/// heading, `Esc`/`T`/`q` close without moving.
fn dispatch_toc(s: &mut Session, cmd: Command) -> bool {
    let total = s.doc.headings.len();
    match cmd {
        Command::Quit => true,
        Command::ToggleToc | Command::ClearHighlights | Command::SearchCancel => {
            s.toc = None;
            false
        }
        Command::ScrollDown(n) => {
            if let Some(t) = s.toc.as_mut() {
                t.step(isize::from(i16::try_from(n).unwrap_or(i16::MAX)), total);
            }
            false
        }
        Command::ScrollUp(n) => {
            if let Some(t) = s.toc.as_mut() {
                t.step(-isize::from(i16::try_from(n).unwrap_or(i16::MAX)), total);
            }
            false
        }
        Command::Home => {
            if let Some(t) = s.toc.as_mut() {
                t.selected = 0;
            }
            false
        }
        Command::End => {
            if let Some(t) = s.toc.as_mut() {
                t.selected = total.saturating_sub(1);
            }
            false
        }
        Command::TocActivate => {
            let target = s.toc.as_ref().and_then(|t| s.doc.headings.get(t.selected));
            if let Some(h) = target {
                let line = s.doc.line_for_styled_offset(h.styled_offset);
                s.view.scroll_to(line, &s.doc);
            }
            s.toc = None;
            false
        }
        _ => false,
    }
}

/// Scroll to the next heading relative to the viewport top.
///
/// Forward skips the current heading if its line equals `view.top` so
/// repeated `]]` keystrokes walk down the document rather than sticking.
fn jump_heading(s: &mut Session, dir: Direction) {
    let top = s.view.top;
    let target = match dir {
        Direction::Forward => s
            .doc
            .headings
            .iter()
            .map(|h| s.doc.line_for_styled_offset(h.styled_offset))
            .find(|&line| line > top),
        Direction::Backward => s
            .doc
            .headings
            .iter()
            .rev()
            .map(|h| s.doc.line_for_styled_offset(h.styled_offset))
            .find(|&line| line < top),
    };
    if let Some(line) = target {
        s.view.scroll_to(line, &s.doc);
    } else {
        s.status = Some(match dir {
            Direction::Forward => "no next heading".to_string(),
            Direction::Backward => "no previous heading".to_string(),
        });
    }
}

/// Prompt prefix shown in the status bar while search input is active.
fn prompt_for(dir: SearchDirection) -> &'static str {
    match dir {
        SearchDirection::Forward => "/",
        SearchDirection::Backward => "?",
    }
}

/// Compile `pattern`, jump to the first match, update the status text.
fn apply_search(s: &mut Session, pattern: String) {
    let mut state = match SearchState::compile(&s.doc, &pattern, s.regex, s.case) {
        Ok(state) => state,
        Err(error) => {
            s.status = Some(format!("{error}"));
            return;
        }
    };
    let initial = match s.direction {
        SearchDirection::Forward => Direction::Forward,
        SearchDirection::Backward => Direction::Backward,
    };
    let jump = state
        .current()
        .map(|m| m.line)
        .or_else(|| state.step(initial).map(|(m, _)| m.line));
    let total = state.len();
    s.search = Some(state);
    s.status = Some(if total == 0 {
        format!("Pattern not found: {pattern}")
    } else {
        format!("{total} matches  n/N:next/prev  Esc:clear")
    });
    if let Some(line) = jump {
        s.view.scroll_to(line, &s.doc);
    }
}

/// Advance the match cursor and scroll so the new match is visible.
fn step_search(s: &mut Session, dir: Direction) {
    let Some(state) = s.search.as_mut() else {
        return;
    };
    if let Some((m, wrapped)) = state.step(dir) {
        if wrapped {
            s.status = Some("search wrapped".to_string());
        }
        s.view.scroll_to(m.line, &s.doc);
    }
}

/// Emit the next frame — body or TOC modal, depending on session state.
fn draw<W: Write>(s: &Session, out: &mut W) -> io::Result<()> {
    match s.toc.as_ref() {
        Some(toc) => s.view.draw_toc(out, &s.doc.headings, toc),
        None => s.view.draw(
            out,
            &s.doc,
            s.matches(),
            s.current_match(),
            s.status.as_deref(),
        ),
    }
}

/// Cap rendered lines at ~120 columns on wide terminals; prose and code
/// fences past that wrap into a wall of text. `--columns` overrides.
const MAX_RENDER_COLS: u16 = 120;

/// Render the document once with image protocols disabled and return the
/// pager's in-memory buffer.
fn render_doc(
    filename: &str,
    parser: &dyn SourceParser,
    common: &CommonArgs,
    line_numbers: bool,
    resource_handler: &dyn ResourceUrlHandler,
) -> Result<RenderedDoc> {
    let (base_dir, input) = read_input(filename)?;
    let events = parser.parse(&input);
    let env =
        Environment::for_local_directory(&base_dir).context("build environment for mdless")?;

    let (cols, _rows) = size().unwrap_or((80, 24));
    // Reserve the gutter footprint up front so code-block frames,
    // tables, and rules never spill past the right edge of the
    // viewport once the gutter steals its columns.
    let reserved = if line_numbers { view::GUTTER } else { 2 };
    let columns = common
        .columns
        .unwrap_or_else(|| cols.saturating_sub(reserved).clamp(20, MAX_RENDER_COLS));
    let terminal_size = crate::TerminalSize::default().with_max_columns(columns);

    let syntax_set = syntect::parsing::SyntaxSet::load_defaults_newlines();
    let settings = Settings {
        terminal_capabilities: ansi_without_images(),
        terminal_size,
        multiplexer: Multiplexer::None,
        syntax_set: &syntax_set,
        theme: Theme::default(),
        syntax_color_map: Preset::Classic.syntax_map(),
        wrap_code: common.wrap_code,
    };

    let mut styled = Vec::with_capacity(input.len() * 2);
    let mut recorder = HeadingRecorder::default();
    crate::push_tty_with_observer(
        &settings,
        &env,
        resource_handler,
        &mut styled,
        events,
        &mut recorder,
    )
    .with_context(|| format!("rendering {}", Path::new(filename).display()))?;

    Ok(build(styled, recorder.finish()))
}

/// ANSI styling + OSC 8 links, no image protocols.
fn ansi_without_images() -> TerminalCapabilities {
    let mut caps = TerminalProgram::Ansi.capabilities();
    caps.image = None;
    caps
}

/// RAII guard that enters the alternate screen + raw mode on construction
/// and restores the terminal on drop, even on panic.
struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode().context("enable raw mode")?;
        execute!(io::stdout(), EnterAlternateScreen, Hide).context("enter alternate screen")?;
        install_panic_hook();
        Ok(Self)
    }
}

/// Restore the terminal on panic so a crashed pager doesn't strand the
/// user in raw mode on the alternate screen.
///
/// Chains to the previous hook after cleanup so panics still print.
fn install_panic_hook() {
    use std::sync::Once;
    static HOOK: Once = Once::new();
    HOOK.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let mut out = io::stdout();
            let _ = queue!(out, Show, LeaveAlternateScreen);
            let _ = out.flush();
            let _ = disable_raw_mode();
            previous(info);
        }));
    });
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // Best-effort cleanup; we're already tearing down.
        let mut out = io::stdout();
        let _ = queue!(out, Show, LeaveAlternateScreen);
        let _ = out.flush();
        let _ = disable_raw_mode();
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::buffer::build;
    use super::keys::{Command, Decoder};
    use super::view::View;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn doc(lines: &[&str]) -> super::RenderedDoc {
        let styled = lines
            .iter()
            .flat_map(|l| l.as_bytes().iter().copied().chain(std::iter::once(b'\n')))
            .collect();
        build(styled, Vec::new())
    }

    /// End-to-end: scroll down with `j`, page down with Space, quit with `q`.
    /// Assert the resulting viewport top matches the expected sequence.
    #[test]
    fn scripted_keystrokes_drive_viewport() {
        let d = doc(&[
            "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine",
        ]);
        let mut v = View::new(80, 4); // 3 body rows
        let mut dec = Decoder::default();

        let script = [
            (KeyCode::Char('j'), 1),
            (KeyCode::Char('j'), 2),
            (KeyCode::Char(' '), 5),
            (KeyCode::Char('k'), 4),
            (KeyCode::Char('G'), 7),
            (KeyCode::Char('g'), 7), // first g — Noop
            (KeyCode::Char('g'), 0), // second g — Home
        ];
        for (code, expected_top) in script {
            let cmd = dec.feed(KeyEvent::new(code, KeyModifiers::NONE));
            v.apply(cmd, &d);
            assert_eq!(v.top, expected_top, "after {code:?}");
        }

        let quit_cmd = dec.feed(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(v.apply(quit_cmd, &d));
    }

    /// Resize while scrolled past the new end clamps back into range.
    #[test]
    fn resize_clamp_preserves_visibility() {
        let d = doc(&["a"; 20]);
        let mut v = View::new(80, 10);
        v.apply(Command::End, &d); // top = 11
        v.resize(80, 80, &d); // body_rows 79 → can show whole doc
        assert_eq!(v.top, 0);
    }

    /// Build a session wrapping hand-crafted styled bytes + headings so
    /// heading-jump / TOC tests don't need to run the full renderer.
    ///
    /// View is 80x5 (4 body rows) so small doc fixtures can still test
    /// `scroll_to` without being clamped to `top = 0`.
    fn session_with_headings(
        lines: &[&str],
        headings: Vec<super::buffer::HeadingEntry>,
    ) -> super::Session {
        let styled: Vec<u8> = lines
            .iter()
            .flat_map(|l| l.as_bytes().iter().copied().chain(std::iter::once(b'\n')))
            .collect();
        let doc = build(styled, headings);
        super::Session {
            doc,
            view: View::new(80, 5),
            decoder: Decoder::default(),
            search: None,
            direction: super::SearchDirection::Forward,
            input: String::new(),
            status: None,
            regex: false,
            case: super::CaseMode::Smart,
            toc: None,
            bookmarks: HashMap::new(),
        }
    }

    /// Press a single-character key and process it through `dispatch`.
    fn press(s: &mut super::Session, c: char) {
        let cmd = s
            .decoder
            .feed(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        let _ = super::dispatch_cmd(s, cmd);
    }

    /// `]]` scrolls forward past the viewport top to the next heading.
    #[test]
    fn double_bracket_jumps_to_next_heading() {
        let lines = (0..20)
            .map(|i| if i == 5 { "## Sub" } else { "body" })
            .collect::<Vec<_>>();
        // One heading at rendered line 5, plain offset = sum of prior line lengths.
        let offset = (0..5).map(|_| "body".len() + 1).sum::<usize>();
        let headings = vec![super::buffer::HeadingEntry {
            level: 2,
            text: "Sub".to_string(),
            styled_offset: offset,
        }];
        let mut s = session_with_headings(&lines, headings);
        super::jump_heading(&mut s, super::Direction::Forward);
        // scroll_to places target near top with a two-line breadcrumb.
        assert_eq!(s.view.top, 3);
    }

    /// Pressing `T` opens the TOC and selects the first heading.
    #[test]
    fn t_opens_toc_modal() {
        let headings = vec![
            super::buffer::HeadingEntry {
                level: 1,
                text: "Intro".to_string(),
                styled_offset: 0,
            },
            super::buffer::HeadingEntry {
                level: 2,
                text: "Body".to_string(),
                styled_offset: 20,
            },
        ];
        let lines = ["# Intro", "x", "x", "x", "x", "## Body", "x"];
        let mut s = session_with_headings(&lines, headings);
        press(&mut s, 'T');
        assert!(s.toc.is_some());
        assert_eq!(s.toc.unwrap().selected, 0);
    }

    /// Inside the TOC, `j` advances the selection; `Enter` jumps and closes.
    #[test]
    fn toc_navigation_jumps_to_selected_heading() {
        // Plain layout: "# First\n" (0..8), "a\n" (8..10), "b\n" (10..12),
        // "c\n" (12..14), "d\n" (14..16), "# Second\n" (16..25), "e\n" (25..27).
        let headings = vec![
            super::buffer::HeadingEntry {
                level: 1,
                text: "First".to_string(),
                styled_offset: 0,
            },
            super::buffer::HeadingEntry {
                level: 1,
                text: "Second".to_string(),
                styled_offset: 16,
            },
        ];
        let lines = ["# First", "a", "b", "c", "d", "# Second", "e"];
        let mut s = session_with_headings(&lines, headings);
        press(&mut s, 'T');
        press(&mut s, 'j');
        let cmd = s
            .decoder
            .feed(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let _ = super::dispatch_cmd(&mut s, cmd);
        assert!(s.toc.is_none(), "TOC should close after activation");
        // scroll_to subtracts 2 for breadcrumb; heading line 5 → top = 3.
        assert_eq!(s.view.top, 3);
    }

    /// `m a` saves the current top; `'a` jumps back to the exact line.
    #[test]
    fn bookmark_roundtrip_restores_view_top() {
        let lines: Vec<&str> = (0..20).map(|_| "line").collect();
        let mut s = session_with_headings(&lines, Vec::new());
        // Start with a wider view that can actually scroll.
        s.view = View::new(80, 10);

        s.view.top = 7;
        press(&mut s, 'm');
        press(&mut s, 'a');
        // Scroll away, then jump back via the bookmark.
        s.view.top = 0;
        press(&mut s, '\'');
        press(&mut s, 'a');
        assert_eq!(s.view.top, 7);
    }
}
