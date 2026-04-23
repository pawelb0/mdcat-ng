// Copyright 2018-2020 Sebastian Wiesner <sebastian@swsnr.de>

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Command-line argument definitions for the `mdcat` multicall binary.
//!
//! The binary dispatches on its `argv[0]` basename: invoking it as
//! `mdcat` selects the `Command::Mdcat` variant, `mdless` selects
//! `Command::Mdless`. Flags common to both subcommands live on
//! `CommonArgs`; mode-specific flags hang off each enum variant.
//! `Command::paging_mode` maps the final flag state to a
//! `PagingMode` that drives the output layer in [`crate::cli`].

use clap::ValueHint;
use clap_complete::Shell;

/// `-h`/`--help` footer.
fn after_help() -> &'static str {
    "See 'man 1 mdcat' for more information.

Two binaries ship: mdcat prints to stdout, mdless opens the
interactive pager. Report issues at
<https://github.com/pawelb0/mdcat-ng>."
}

fn long_version() -> &'static str {
    concat!(
        env!("CARGO_PKG_VERSION"),
        "
Licensed under the Mozilla Public License, v. 2.0.
See <http://mozilla.org/MPL/2.0/>."
    )
}

/// Top-level clap parser. Wraps the multicall subcommand dispatch.
#[derive(Debug, clap::Parser)]
#[command(multicall = true)]
pub struct Args {
    /// Subcommand selected from `argv[0]` (`mdcat` or `mdless`).
    #[command(subcommand)]
    pub command: Command,
}

/// Subcommand selected by `argv[0]`.
#[derive(Debug, clap::Subcommand)]
pub enum Command {
    /// `mdcat`: render markdown to the terminal.
    #[command(version, about, after_help = after_help(), long_version = long_version())]
    Mdcat {
        /// Flags common to both subcommands.
        #[command(flatten)]
        args: CommonArgs,
        /// Pipe the rendered output through `$PAGER` / `less -r`.
        ///
        /// Disables image protocols for the duration of the pager
        /// session since most pagers mangle position-sensitive
        /// escapes.
        #[arg(short, long, overrides_with = "no_pager")]
        paginate: bool,
        /// Do not paginate output (default). Overrides a preceding `--paginate`.
        #[arg(short = 'P', long)]
        no_pager: bool,
    },
    /// `mdless`: open the interactive markdown-aware pager.
    #[command(version, about, after_help = after_help(), long_version = long_version())]
    Mdless {
        /// Flags common to both subcommands.
        #[command(flatten)]
        args: CommonArgs,
        /// Skip the pager and print to stdout, like `mdcat FILE`.
        #[arg(short = 'P', long, overrides_with_all = ["external_pager"])]
        no_pager: bool,
        /// Shell out to `$PAGER` / `less -r` instead of the built-in pager.
        ///
        /// Preserves the 2.x `mdless` behaviour for users who prefer
        /// their existing pager over the built-in interactive one.
        #[arg(long)]
        external_pager: bool,
        /// Pattern to jump to and highlight on startup (like typing `/PATTERN`).
        #[arg(long = "search", value_name = "PATTERN")]
        search: Option<String>,
        /// Force case-sensitive search (default is smart-case).
        #[arg(long)]
        case_sensitive: bool,
        /// Interpret the search pattern as a regex instead of a literal.
        #[arg(long)]
        regex: bool,
        /// Render to stdout without entering the pager; for the test harness.
        #[arg(long, hide = true)]
        render_only: bool,
        /// Show rendered-line numbers in a left gutter. Toggle live with `#`.
        #[arg(short = 'n', long = "line-numbers")]
        line_numbers: bool,
    },
}

/// How `mdcat` should deliver its rendered output to the user.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum PagingMode {
    /// Print to stdout and exit.
    None,
    /// Pipe through the external `$PAGER` / `less -r` child process.
    ExternalLess,
    /// Run the built-in interactive markdown-aware pager.
    Interactive,
}

impl Command {
    /// Resolve the active paging mode from the parsed flags. `--no-pager`
    /// always wins via clap's `overrides_with`, so by this point the flag
    /// combinations below are already mutually consistent.
    pub fn paging_mode(&self) -> PagingMode {
        match *self {
            Command::Mdcat { paginate: true, .. } => PagingMode::ExternalLess,
            Command::Mdcat { .. } => PagingMode::None,
            Command::Mdless { no_pager: true, .. }
            | Command::Mdless {
                render_only: true, ..
            } => PagingMode::None,
            Command::Mdless {
                external_pager: true,
                ..
            } => PagingMode::ExternalLess,
            Command::Mdless { .. } => PagingMode::Interactive,
        }
    }
}

impl PagingMode {
    /// `true` if *any* pager owns the terminal (external `less` or the
    /// built-in interactive pager). Used to decide whether we should emit
    /// image-protocol escapes, run active TTY probes, etc.
    pub fn is_paginated(self) -> bool {
        !matches!(self, PagingMode::None)
    }
}

impl std::ops::Deref for Command {
    type Target = CommonArgs;

    fn deref(&self) -> &Self::Target {
        match self {
            Command::Mdcat { args, .. } => args,
            Command::Mdless { args, .. } => args,
        }
    }
}

/// Flags shared by both `mdcat` and `mdless`.
#[derive(Debug, clap::Args)]
pub struct CommonArgs {
    /// Files to read.  If - read from standard input instead.
    #[arg(default_value="-", value_hint = ValueHint::FilePath)]
    pub filenames: Vec<String>,
    /// Disable all colours and other styles.
    #[arg(short = 'c', long, aliases=["nocolour", "no-color", "nocolor"])]
    pub no_colour: bool,
    /// Maximum number of columns to use for output.
    #[arg(long)]
    pub columns: Option<u16>,
    /// Deprecated: kept for 2.x compatibility. Local-only is now the default.
    #[arg(short = 'l', long = "local", hide = true)]
    pub local_only: bool,
    /// Fetch remote (HTTP/HTTPS) images for inline display.
    ///
    /// Off by default: remote fetches can be slow, and the tracking /
    /// SSRF surface they open isn't something a Markdown viewer should
    /// pay for silently. Pass this when you want images from URLs to
    /// render inline on a capable terminal.
    #[arg(long = "remote-images")]
    pub remote_images: bool,
    /// Exit immediately if any error occurs processing an input file.
    #[arg(long = "fail")]
    pub fail_fast: bool,
    /// Print detected terminal name and exit.
    #[arg(long = "detect-terminal")]
    pub detect_and_exit: bool,
    /// Skip terminal detection and only use ANSI formatting.
    #[arg(long = "ansi", conflicts_with = "no_colour")]
    pub ansi_only: bool,
    /// Skip active DA1 capability probing (which is on by default for interactive TTY output).
    #[arg(long = "no-probe-terminal")]
    pub no_probe_terminal: bool,
    /// Milliseconds to wait for a Primary Device Attributes (DA1) probe reply.
    ///
    /// Real terminals answer in 1-5 ms; the default gives a generous window
    /// for slow SSH sessions without noticeably delaying startup. Bump this
    /// if the probe silently falls through on a responsive-but-slow terminal.
    #[arg(long = "probe-timeout-ms", default_value_t = 50)]
    pub probe_timeout_ms: u64,
    /// Generate completions for a shell to standard output and exit.
    #[arg(long)]
    pub completions: Option<Shell>,
    /// Wrap code-block lines that exceed the terminal width instead of overflowing.
    #[arg(long = "wrap-code")]
    pub wrap_code: bool,
}

/// What resources mdcat may access.
#[derive(Debug, Copy, Clone)]
pub enum ResourceAccess {
    /// Only allow local resources.
    LocalOnly,
    /// Allow remote resources
    Remote,
}

impl CommonArgs {
    /// Whether remote resource access is permitted.
    ///
    /// Local-only is the default. `--remote-images` opts in. `--local`
    /// is kept as a no-op alias so 2.x command lines keep parsing.
    pub fn resource_access(&self) -> ResourceAccess {
        if self.remote_images && !self.local_only {
            ResourceAccess::Remote
        } else {
            ResourceAccess::LocalOnly
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Args;
    use clap::CommandFactory;

    #[test]
    fn verify_app() {
        Args::command().debug_assert();
    }
}
