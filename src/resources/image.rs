// Copyright Sebastian Wiesner <sebastian@swsnr.de>

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Inline image handling

use std::io::Write;

use url::Url;

use crate::{ResourceUrlHandler, TerminalSize};

/// An implementation of an inline image protocol.
pub trait InlineImageProtocol {
    /// Write an inline image to `writer`.
    ///
    /// `url` is the URL pointing to the image was obtained from.  If the underlying terminal does
    /// not support URLs directly the protocol should use `resource_handler` to load the image data
    /// from `url`.
    ///
    /// `size` denotes the dimensions of the current terminal, to be used as indication for the
    /// size the image should be rendered at.
    ///
    /// Implementations are encouraged to return an IO error with [`std::io::ErrorKind::Unsupported`]
    /// if either the underlying terminal does not support images currently or if it does not
    /// support the given image format.
    fn write_inline_image(
        &self,
        writer: &mut dyn Write,
        resource_handler: &dyn ResourceUrlHandler,
        url: &Url,
        terminal_size: TerminalSize,
    ) -> std::io::Result<()>;
}

/// Downsize an image to the given terminal size.
///
/// If the `image` is larger than the amount of columns in the given terminal `size` attempt to
/// downsize the image to fit into the given columns.
///
/// Return the downsized image if `image` was larger than the column limit and `size` defined the
/// terminal size in pixels.
///
/// Return `None` if `size` does not specify the cell size, or if `image` is already smaller than
/// the column limit.
#[cfg(feature = "image-processing")]
pub fn downsize_to_columns(
    image: &image::DynamicImage,
    size: TerminalSize,
) -> Option<image::DynamicImage> {
    use image::{imageops::FilterType, GenericImageView};
    use tracing::{event, Level};
    let win_size = size.pixels?;
    event!(
        Level::DEBUG,
        "Terminal size {:?}; image is {:?}",
        size,
        image.dimensions()
    );
    let (image_width, _) = image.dimensions();
    if win_size.x < image_width {
        Some(image.resize(win_size.x, win_size.y, FilterType::Nearest))
    } else {
        None
    }
}

/// Decode `mime_data` into a [`image::DynamicImage`].
///
/// SVG input is first rasterised to PNG via [`crate::resources::svg::render_svg_to_png`].
/// Other formats are dispatched through the `image` crate, using the declared MIME
/// essence as a hint, or falling back to automatic format detection when no hint is
/// available.
///
/// Returns an [`std::io::Error`] whose source is the underlying decode failure so
/// callers can surface a meaningful message.
#[cfg(feature = "image-processing")]
pub fn decode_image(
    mime_data: &crate::resources::MimeData,
) -> std::io::Result<image::DynamicImage> {
    use image::ImageFormat;
    use std::io::Error;

    if mime_data.mime_type_essence() == Some("image/svg+xml") {
        let png = crate::resources::svg::render_svg_to_png(&mime_data.data)?;
        return image::load_from_memory_with_format(&png, ImageFormat::Png)
            .map_err(|err| Error::other(format!("rendered SVG was not valid PNG: {err}")));
    }

    let format_hint = mime_data
        .mime_type_essence()
        .and_then(image::ImageFormat::from_mime_type);
    match format_hint {
        Some(format) => image::load_from_memory_with_format(&mime_data.data, format)
            .map_err(|err| Error::other(format!("image decode failed: {err}"))),
        None => image::load_from_memory(&mime_data.data)
            .map_err(|err| Error::other(format!("image decode failed: {err}"))),
    }
}
