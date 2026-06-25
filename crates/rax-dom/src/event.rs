//! The inbound half of the render seam: events flowing **from** the platform
//! back **into** the engine — the dual of [`Mutation`](crate::Mutation).
//!
//! A backend (UIKit/Android) translates native input and lifecycle callbacks
//! into [`Event`]s and pushes them through an [`EventSink`]. The engine drains
//! them on the UI thread and routes them to handlers registered on widgets,
//! where they typically become signal writes. `EventSink` is `Send` so platform
//! callbacks on any thread can enqueue; routing always happens on the UI thread.

use std::sync::mpsc::Sender;

use rax_core::{Point, Rect};

use crate::mutation::WidgetId;

/// Identifies a single pointer/touch in a multi-touch sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PointerId(pub u64);

/// A text selection/caret range, in UTF-8 byte offsets into the field's value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextSelection {
    /// Anchor offset.
    pub start: usize,
    /// Caret offset (may be < `start` for backward selections).
    pub end: usize,
}

impl TextSelection {
    /// A collapsed caret at `offset`.
    pub fn caret(offset: usize) -> Self {
        TextSelection {
            start: offset,
            end: offset,
        }
    }
}

/// Application lifecycle transitions delivered by the platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lifecycle {
    /// Became active/foreground and interactive.
    Resumed,
    /// Lost focus but still visible (e.g. system dialog).
    Inactive,
    /// No longer visible.
    Backgrounded,
    /// About to be terminated.
    Terminating,
}

/// An input or lifecycle event entering the engine.
///
/// Targeted variants carry the `WidgetId` the platform hit-tested; untargeted
/// variants are app-global. New variants are added here and handled by backends
/// and app code — this enum is part of the frozen seam contract (audit R3).
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A discrete tap/click on a widget.
    Tap {
        /// Hit-tested widget.
        target: WidgetId,
    },
    /// A pointer pressed down.
    PointerDown {
        /// Hit-tested widget.
        target: WidgetId,
        /// Location in the widget's coordinate space.
        position: Point,
        /// Which pointer.
        pointer: PointerId,
    },
    /// A pointer moved while down.
    PointerMove {
        /// Hit-tested widget.
        target: WidgetId,
        /// Location in the widget's coordinate space.
        position: Point,
        /// Which pointer.
        pointer: PointerId,
    },
    /// A pointer released.
    PointerUp {
        /// Hit-tested widget.
        target: WidgetId,
        /// Location in the widget's coordinate space.
        position: Point,
        /// Which pointer.
        pointer: PointerId,
    },
    /// A scroll container's content offset changed.
    ScrollChanged {
        /// The scroll view.
        target: WidgetId,
        /// New content offset.
        offset: Point,
    },
    /// A text field's value/selection changed (controlled input).
    TextChanged {
        /// The text field.
        target: WidgetId,
        /// The full new value.
        value: String,
        /// The new selection/caret.
        selection: TextSelection,
    },
    /// A widget gained or lost focus.
    FocusChanged {
        /// The affected widget.
        target: WidgetId,
        /// Whether it is now focused.
        focused: bool,
    },
    /// A control's value changed (switch on/off as 0/1, slider position, etc.).
    ValueChanged {
        /// The control.
        target: WidgetId,
        /// New value.
        value: f64,
    },
    /// Android system back / iOS interactive-pop intent. App-global.
    BackPressed,
    /// The soft keyboard is about to appear, occupying `frame`. App-global.
    KeyboardWillShow {
        /// Keyboard frame in screen coordinates.
        frame: Rect,
    },
    /// The soft keyboard is about to hide. App-global.
    KeyboardWillHide,
    /// An application lifecycle transition. App-global.
    AppLifecycle(Lifecycle),
}

/// The discriminant used to register and match handlers, independent of an
/// event's payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventKind {
    /// [`Event::Tap`].
    Tap,
    /// [`Event::PointerDown`].
    PointerDown,
    /// [`Event::PointerMove`].
    PointerMove,
    /// [`Event::PointerUp`].
    PointerUp,
    /// [`Event::ScrollChanged`].
    ScrollChanged,
    /// [`Event::TextChanged`].
    TextChanged,
    /// [`Event::FocusChanged`].
    FocusChanged,
    /// [`Event::ValueChanged`].
    ValueChanged,
    /// [`Event::BackPressed`].
    BackPressed,
    /// [`Event::KeyboardWillShow`].
    KeyboardWillShow,
    /// [`Event::KeyboardWillHide`].
    KeyboardWillHide,
    /// [`Event::AppLifecycle`].
    AppLifecycle,
}

impl Event {
    /// The kind discriminant of this event.
    pub fn kind(&self) -> EventKind {
        match self {
            Event::Tap { .. } => EventKind::Tap,
            Event::PointerDown { .. } => EventKind::PointerDown,
            Event::PointerMove { .. } => EventKind::PointerMove,
            Event::PointerUp { .. } => EventKind::PointerUp,
            Event::ScrollChanged { .. } => EventKind::ScrollChanged,
            Event::TextChanged { .. } => EventKind::TextChanged,
            Event::FocusChanged { .. } => EventKind::FocusChanged,
            Event::ValueChanged { .. } => EventKind::ValueChanged,
            Event::BackPressed => EventKind::BackPressed,
            Event::KeyboardWillShow { .. } => EventKind::KeyboardWillShow,
            Event::KeyboardWillHide => EventKind::KeyboardWillHide,
            Event::AppLifecycle(_) => EventKind::AppLifecycle,
        }
    }

    /// The widget this event targets, if it is a targeted (hit-tested) event.
    /// `None` for app-global events.
    pub fn target(&self) -> Option<WidgetId> {
        match *self {
            Event::Tap { target }
            | Event::PointerDown { target, .. }
            | Event::PointerMove { target, .. }
            | Event::PointerUp { target, .. }
            | Event::ScrollChanged { target, .. }
            | Event::TextChanged { target, .. }
            | Event::FocusChanged { target, .. }
            | Event::ValueChanged { target, .. } => Some(target),
            Event::BackPressed
            | Event::KeyboardWillShow { .. }
            | Event::KeyboardWillHide
            | Event::AppLifecycle(_) => None,
        }
    }
}

/// A cloneable, `Send` handle a backend uses to push events into the engine.
///
/// Enqueues without routing; the engine drains and dispatches on the UI thread
/// (see [`Tree::drain_events`](crate::Tree::drain_events)).
#[derive(Clone)]
pub struct EventSink {
    tx: Sender<Event>,
}

impl EventSink {
    pub(crate) fn new(tx: Sender<Event>) -> Self {
        EventSink { tx }
    }

    /// Enqueues `event` for delivery. Dropped silently if the engine is gone.
    pub fn dispatch(&self, event: Event) {
        let _ = self.tx.send(event);
    }
}
