// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Observer hook.
//!
//! [`push_tty_with_observer`](crate::push_tty_with_observer) calls
//! `on_event(byte_offset, event)` before each pulldown-cmark event.
//! The default [`NoopObserver`] compiles away.

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
