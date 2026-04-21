// Copyright 2025 mdcat contributors
// Copyright 2026 Pawel Boguszewski

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Sixel graphics protocol for terminals like foot, mlterm, Windows Terminal
//! and others that understand the venerable DEC sixel format.
//!
//! This implementation decodes the source image (PNG/JPEG/GIF/… via the
//! `image` crate), optionally downscales it to fit the terminal columns,
//! then runs `icy_sixel` for palette quantization and sixel encoding.
//!
//! Only available when the `sixel` feature is enabled (default on).

use std::io::{Error, Result, Write};

use icy_sixel::SixelImage;
use image::GenericImageView;
use tracing::{event, instrument, Level};

use crate::resources::image::{decode_image, downsize_to_columns, InlineImageProtocol};
use crate::resources::{MimeData, ResourceUrlHandler};
use crate::terminal::size::TerminalSize;

/// Marker type for the Sixel image protocol.
///
/// See [Sixel graphics](https://en.wikipedia.org/wiki/Sixel) and the
/// `icy_sixel` crate for the encoding details.
#[derive(Debug, Copy, Clone)]
pub struct SixelProtocol;

impl InlineImageProtocol for SixelProtocol {
    #[instrument(skip(self, writer, resource_handler, terminal_size))]
    fn write_inline_image(
        &self,
        writer: &mut dyn Write,
        resource_handler: &dyn ResourceUrlHandler,
        url: &url::Url,
        terminal_size: TerminalSize,
    ) -> Result<()> {
        let mime_data = resource_handler.read_resource(url)?;
        event!(Level::DEBUG, "Received mime type {:?}", mime_data.mime_type);
        let sixel = encode(&mime_data, terminal_size)?;
        writer.write_all(sixel.as_bytes())
    }
}

fn encode(mime_data: &MimeData, terminal_size: TerminalSize) -> Result<String> {
    let image = decode_image(mime_data)?;
    let image = downsize_to_columns(&image, terminal_size).unwrap_or(image);
    let (width, height) = image.dimensions();
    let rgba = image.to_rgba8().into_raw();

    SixelImage::from_rgba(rgba, width as usize, height as usize)
        .encode()
        .map_err(|err| Error::other(format!("sixel encode failed: {err}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::MimeData;

    #[test]
    fn encodes_a_tiny_png_to_sixel() {
        // 2×2 red/green/blue/white PNG
        let png = std::fs::read("sample/rust-logo-128x128.png").expect("sample PNG missing");
        let data = MimeData {
            mime_type: Some(mime::IMAGE_PNG),
            data: png,
        };
        let out = encode(&data, TerminalSize::default()).expect("encode");
        // DCS introducer is `ESC P` (possibly with parameters) followed by a
        // final byte `q` that switches into sixel mode. ST is `ESC \`.
        assert!(
            out.starts_with("\x1bP"),
            "expected DCS introducer at the start of sixel output"
        );
        assert!(out.contains('q'), "expected sixel final byte `q` in output");
        assert!(out.ends_with("\x1b\\"), "expected ST terminator");
    }
}
