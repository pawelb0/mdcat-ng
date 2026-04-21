// Copyright 2026 Pawel Boguszewski
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Byte-counting `Write` adapter.
//!
//! This file contains [`CountingWriter`], a wrapper around any
//! `std::io::Write` that tracks the number of bytes successfully
//! written. Partial writes are handled correctly via the default
//! `write_all` implementation so the counter stays exact even when
//! the underlying writer only accepts part of a buffer at a time.
//!
//! How it fits: [`push_tty_with_observer`](crate::push_tty_with_observer)
//! wraps its caller's writer in a `CountingWriter` before entering
//! the render loop. Before each pulldown-cmark event dispatches,
//! the observer receives the writer's current byte count alongside
//! the event. The pager's `HeadingRecorder` uses those offsets to
//! map headings onto output bytes so `]]` / `[[` and the TOC modal
//! can jump by line. Nothing outside the render pipeline needs to
//! know about this type.

use std::io::{self, Write};

/// Wraps a [`Write`] implementation and counts bytes successfully written.
///
/// A partial write increments the counter by the number of bytes the inner
/// writer accepted; the rest are retried on the next call, so the counter
/// always reflects the exact cursor position.
pub struct CountingWriter<W> {
    inner: W,
    bytes: u64,
}

impl<W: Write> CountingWriter<W> {
    /// Wrap `inner`. The counter starts at zero.
    pub fn new(inner: W) -> Self {
        Self { inner, bytes: 0 }
    }

    /// Bytes successfully written since construction.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
}

impl<W: Write> Write for CountingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.bytes += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }

    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        // Forward to the default impl rather than to `self.inner.write_all`
        // so every partial write bumps the counter via our `write` impl.
        let mut remaining = buf;
        while !remaining.is_empty() {
            match self.write(remaining) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "failed to write whole buffer",
                    ));
                }
                Ok(n) => remaining = &remaining[n..],
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_bytes_across_writes() {
        let mut w = CountingWriter::new(Vec::new());
        w.write_all(b"hello").unwrap();
        assert_eq!(w.bytes(), 5);
        w.write_all(b", world").unwrap();
        assert_eq!(w.bytes(), 12);
        assert_eq!(w.inner, b"hello, world");
    }

    #[test]
    fn partial_writes_increment_correctly() {
        // A writer that only accepts 3 bytes per call. After write_all the
        // counter should still match the buffer length because we loop.
        struct Partial(Vec<u8>);
        impl Write for Partial {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                let n = buf.len().min(3);
                self.0.extend_from_slice(&buf[..n]);
                Ok(n)
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut w = CountingWriter::new(Partial(Vec::new()));
        w.write_all(b"hello, world").unwrap();
        assert_eq!(w.bytes(), 12);
    }
}
