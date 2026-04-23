// Copyright 2025 mdcat contributors

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Typed errors for the rendering library.
//!
//! [`RenderError`] distinguishes IO failures from image-protocol and
//! SVG rasterisation failures so callers can pattern-match on the
//! failure category without probing an `ErrorKind`. `std::io::Error`
//! still drives the state machine internally and auto-wraps via
//! `#[from]`, so `?` on an `io::Result` works directly in any
//! function returning [`RenderResult`].

use thiserror::Error;

/// Errors that occur while rendering markdown to a terminal.
///
/// This is the error type for the library entry point [`push_tty`][crate::push_tty]
/// and the render module's internal helpers. Callers typically don't need to
/// distinguish the variants — printing the error (including its `Display`
/// implementation, which quotes the inner cause) is enough.
#[derive(Debug, Error)]
pub enum RenderError {
    /// Writing to the output, reading from a resource, or another
    /// underlying I/O operation failed.
    #[error("I/O error")]
    Io(#[from] std::io::Error),

    /// A terminal image protocol failed to encode or emit an image.
    ///
    /// `protocol` names the protocol ("kitty", "iterm2", "sixel",
    /// "terminology") so the caller can see which capability broke.
    #[error("image protocol {protocol} failed: {source}")]
    ImageProtocol {
        /// Name of the image protocol that failed.
        protocol: &'static str,
        /// Underlying cause, carried as an opaque error to avoid leaking
        /// per-protocol error types into the public API.
        #[source]
        source: anyhow::Error,
    },

    /// Rasterising an SVG to PNG via the `svg` feature failed.
    #[error("SVG rendering failed: {0}")]
    Svg(#[source] anyhow::Error),
}

/// `Result` type used throughout the rendering library.
pub type RenderResult<T> = std::result::Result<T, RenderError>;
