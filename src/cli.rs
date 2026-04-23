// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Shared CLI entry point for both binaries.
//!
//! `src/main.rs` and `src/bin/mdless.rs` are shims into [`run`].
//! clap's multicall mode picks the subcommand from `argv[0]`.

use clap::{CommandFactory, Parser};
use clap_complete::generate;

use crate::args::{Args, PagingMode};
use crate::output::Output;
use crate::{
    create_resource_handler, process_file, Multiplexer, Settings, TerminalProgram, TerminalSize,
    Theme,
};
use syntect::parsing::SyntaxSet;
use tracing::{event, Level};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::EnvFilter;

/// Parse arguments, detect the terminal, dispatch. Exits the process.
pub fn run() -> ! {
    let filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::OFF.into())
        .with_env_var("MDCAT_LOG")
        .from_env_lossy();
    tracing_subscriber::fmt::Subscriber::builder()
        .pretty()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();

    let args = Args::parse().command;
    event!(target: "mdcat::main", Level::TRACE, ?args, "mdcat arguments");

    if let Some(shell) = args.completions {
        let binary = match args {
            crate::args::Command::Mdcat { .. } => "mdcat",
            crate::args::Command::Mdless { .. } => "mdless",
        };
        let mut command = Args::command();
        let subcommand = command.find_subcommand_mut(binary).unwrap();
        generate(shell, subcommand, binary, &mut std::io::stdout());
        std::process::exit(0);
    }

    let stdout_is_tty = std::io::IsTerminal::is_terminal(&std::io::stdout());
    let paging_mode = args.paging_mode();

    let terminal = if args.no_colour {
        TerminalProgram::Dumb
    } else if paging_mode.is_paginated() || args.ansi_only {
        TerminalProgram::Ansi
    } else if !stdout_is_tty {
        TerminalProgram::Dumb
    } else {
        TerminalProgram::detect()
    };

    let multiplexer = Multiplexer::detect();

    let probed = if !args.no_probe_terminal
        && terminal == TerminalProgram::Ansi
        && stdout_is_tty
        && !paging_mode.is_paginated()
    {
        crate::terminal::probe_da1(std::time::Duration::from_millis(args.probe_timeout_ms))
    } else {
        None
    };

    if args.detect_and_exit {
        println!("Terminal: {terminal}");
        if multiplexer != Multiplexer::None {
            println!("Multiplexer: {multiplexer:?}");
        }
        if let Some(attrs) = probed {
            println!("Probed: sixel={}", attrs.sixel);
        }
        std::process::exit(0);
    }

    if paging_mode == PagingMode::Interactive {
        std::process::exit(run_interactive_mdless(&args));
    }

    #[cfg(windows)]
    anstyle_query::windows::enable_ansi_colors();

    // Leave ~2 columns of breathing room on the right edge, except on
    // pathologically narrow terminals where every column counts.
    let base = TerminalSize::detect().unwrap_or_default();
    let max_columns = args.columns.unwrap_or(if base.columns > 20 {
        base.columns - 2
    } else {
        base.columns
    });
    let terminal_size = base.with_max_columns(max_columns);

    let exit_code = match Output::new(paging_mode == PagingMode::ExternalLess) {
        Ok(mut output) => {
            #[cfg_attr(not(feature = "sixel"), allow(unused_mut))]
            let mut capabilities = terminal.capabilities();
            #[cfg(feature = "sixel")]
            if probed.is_some_and(|attrs| attrs.sixel) && capabilities.image.is_none() {
                use crate::terminal::capabilities::{sixel::SixelProtocol, ImageCapability};
                capabilities.image = Some(ImageCapability::Sixel(SixelProtocol));
            }
            let settings = Settings {
                terminal_capabilities: capabilities,
                terminal_size,
                multiplexer,
                syntax_set: &SyntaxSet::load_defaults_newlines(),
                theme: Theme::default(),
                wrap_code: args.wrap_code,
            };
            event!(
                target: "mdcat::main",
                Level::TRACE,
                ?settings.terminal_size,
                ?settings.terminal_capabilities,
                "settings"
            );
            let resource_handler = create_resource_handler(args.resource_access()).unwrap();
            args.filenames
                .iter()
                .try_fold(0, |code, filename| {
                    process_file(
                        filename,
                        &settings,
                        args.resource_access(),
                        &resource_handler,
                        &mut output,
                    )
                    .map(|()| code)
                    .or_else(|error| {
                        eprintln!("Error: {filename}: {error}");
                        if args.fail_fast {
                            Err(error)
                        } else {
                            Ok(1)
                        }
                    })
                })
                .unwrap_or(1)
        }
        Err(error) => {
            eprintln!("Error: {error:#}");
            128
        }
    };
    event!(target: "mdcat::main", Level::TRACE, "Exiting with final exit code {}", exit_code);
    std::process::exit(exit_code);
}

/// Run the interactive `mdless` pager for the first filename.
///
/// Extra filenames are ignored with a warning since the interactive
/// pager only buffers one document at a time.
fn run_interactive_mdless(args: &crate::args::Command) -> i32 {
    let resource_handler =
        create_resource_handler(args.resource_access()).expect("resource handler");
    let filename = args.filenames.first().map_or("-", String::as_str);
    if args.filenames.len() > 1 {
        eprintln!(
            "mdless: only the first file is shown interactively; {} more ignored",
            args.filenames.len() - 1,
        );
    }
    let opts = match args {
        crate::args::Command::Mdless {
            search,
            case_sensitive,
            regex,
            line_numbers,
            ..
        } => crate::mdless::MdlessOptions {
            initial: search.clone(),
            case_sensitive: *case_sensitive,
            regex: *regex,
            line_numbers: *line_numbers,
        },
        crate::args::Command::Mdcat { .. } => crate::mdless::MdlessOptions::default(),
    };
    match crate::mdless::run(filename, args, opts, &resource_handler) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("Error: {filename}: {error:#}");
            1
        }
    }
}
