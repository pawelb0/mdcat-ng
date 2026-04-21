// Copyright 2026 Pawel Boguszewski
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Observer hook for the render pipeline.
//!
//! This file contains:
//!
//! - [`RenderObserver`] — the trait a caller implements to watch
//!   the render pipeline. Its single method receives each
//!   pulldown-cmark [`Event`] paired with the output writer's
//!   current byte offset, called *before* the event is rendered.
//! - [`NoopObserver`] — a zero-cost implementation that ignores
//!   every call. Used by the default [`push_tty`](crate::push_tty)
//!   path so callers without structural-position needs pay nothing.
//! - A blanket impl so `&mut O: RenderObserver` forwards to the
//!   underlying observer.
//!
//! How it fits:
//! [`push_tty_with_observer`](crate::push_tty_with_observer) runs
//! the standard render state machine, wrapped in a
//! [`CountingWriter`](super::CountingWriter) so the byte offset
//! stays exact across partial writes. Before each event is
//! dispatched it calls `observer.on_event(offset, event)`. The
//! interactive pager's `HeadingRecorder` is the main consumer —
//! it maps heading events to byte offsets so `]]` / `[[` and the
//! TOC modal can jump into the rendered document by line.

use pulldown_cmark::Event;

/// Observer invoked on every event the render state machine processes.
///
/// Implementations typically accumulate a side-table mapping output byte
/// offsets to structural events. The default [`NoopObserver`] discards
/// every call and compiles away under the optimiser.
pub trait RenderObserver {
    /// Called immediately before the event is rendered.
    ///
    /// `byte_offset` is the number of bytes written to the output so far.
    /// Observers must not mutate the event; they receive it by shared
    /// reference.
    fn on_event(&mut self, byte_offset: u64, event: &Event<'_>);
}

/// Observer that ignores every event.
///
/// Used by [`push_tty`](crate::push_tty) to avoid paying for the hook when
/// structural information is not needed.
#[derive(Debug, Default, Copy, Clone)]
pub struct NoopObserver;

impl RenderObserver for NoopObserver {
    #[inline]
    fn on_event(&mut self, _byte_offset: u64, _event: &Event<'_>) {}
}

impl<O: RenderObserver + ?Sized> RenderObserver for &mut O {
    fn on_event(&mut self, byte_offset: u64, event: &Event<'_>) {
        (**self).on_event(byte_offset, event);
    }
}
