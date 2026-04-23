// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#![deny(warnings, clippy::all)]
#![forbid(unsafe_code)]

//! `mdless` binary: interactive markdown-aware pager.
//!
//! The clap multicall layer in [`mdcat::cli`] dispatches on `argv[0]`
//! basename, so entering here lands on the `Mdless` subcommand.

fn main() {
    mdcat::cli::run();
}
