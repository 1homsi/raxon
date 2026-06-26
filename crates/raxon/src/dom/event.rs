//! The inbound half of the render seam: events flowing **from** the platform
//! back **into** the engine — the dual of [`Mutation`](crate::Mutation).
//!
//! A backend (UIKit/Android) translates native input and lifecycle callbacks
//! into [`Event`]s and pushes them through an [`EventSink`]. The engine drains
//! them on the UI thread and routes them to handlers registered on widgets,
//! where they typically become signal writes. `EventSink` is `Send` so platform
//! callbacks on any thread can enqueue; routing always happens on the UI thread.

use std::sync::mpsc::Sender;

use crate::core::{Point, Rect};

use super::mutation::WidgetId;

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
    /// A double tap on a widget.
    DoubleTap {
        /// Hit-tested widget.
        target: WidgetId,
    },
    /// A long press on a widget.
    LongPress {
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
    /// A pan/drag gesture updated. Fires repeatedly through the drag.
    PanChanged {
        /// The dragged widget.
        target: WidgetId,
        /// Cumulative translation from the gesture start, in points.
        translation: Point,
        /// Current drag velocity, in points/second.
        velocity: Point,
        /// Lifecycle phase of the gesture.
        phase: GesturePhase,
    },
    /// Pull-to-refresh was triggered on `target`.
    Refresh {
        /// The scroll view that triggered the refresh.
        target: WidgetId,
    },
    /// Return/Done key pressed in `target` text field.
    Submit {
        /// The text field where submit was triggered.
        target: WidgetId,
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
    /// A QR code was detected in the camera feed.
    QrDetected {
        /// The camera widget that detected the code.
        target: WidgetId,
        /// The decoded string value of the QR code.
        value: String,
    },
    /// A pinch gesture updated on `target`. Fires repeatedly through the gesture.
    PinchChanged {
        /// The pinched widget.
        target: WidgetId,
        /// Cumulative scale factor since gesture began (1.0 = no change).
        scale: f32,
        /// Scale velocity (scale units per second).
        velocity: f32,
        /// Lifecycle phase of the gesture.
        phase: GesturePhase,
    },
    /// A rotation gesture updated on `target`. Fires repeatedly through the gesture.
    RotateChanged {
        /// The rotated widget.
        target: WidgetId,
        /// Cumulative rotation in radians.
        rotation: f32,
        /// Rotation velocity (radians/second).
        velocity: f32,
        /// Lifecycle phase of the gesture.
        phase: GesturePhase,
    },
    /// The app received a deep link URL (from openURL: or continueUserActivity:).
    DeepLink {
        /// The full URL string.
        url: String,
    },
    /// Result of a biometric authentication attempt triggered by
    /// [`Mutation::AuthenticateBiometric`](crate::Mutation::AuthenticateBiometric).
    BiometricResult {
        /// Whether authentication succeeded.
        success: bool,
        /// An error message if authentication failed, or `None` on success.
        error: Option<String>,
    },
    /// GPS location update from CoreLocation.
    LocationUpdated {
        /// Latitude in degrees.
        latitude: f64,
        /// Longitude in degrees.
        longitude: f64,
        /// Horizontal accuracy in metres.
        accuracy: f64,
    },
    /// Location permission was denied by the user.
    LocationDenied,
    /// Motion sensor update (accelerometer and/or gyroscope).
    MotionUpdated {
        /// Accelerometer X axis (m/s²) — `None` if not requested.
        accel_x: Option<f64>,
        /// Accelerometer Y axis (m/s²) — `None` if not requested.
        accel_y: Option<f64>,
        /// Accelerometer Z axis (m/s²) — `None` if not requested.
        accel_z: Option<f64>,
        /// Gyroscope X axis (rad/s) — `None` if not requested.
        gyro_x: Option<f64>,
        /// Gyroscope Y axis (rad/s) — `None` if not requested.
        gyro_y: Option<f64>,
        /// Gyroscope Z axis (rad/s) — `None` if not requested.
        gyro_z: Option<f64>,
    },
    /// The user picked media items from the library.
    MediaPicked {
        /// Raw image bytes for each selected item (JPEG/PNG).
        images: Vec<std::sync::Arc<Vec<u8>>>,
    },
    /// The user cancelled the media picker without selecting anything.
    MediaPickerCancelled,
    /// The user picked one or more documents from the file picker.
    DocumentPicked {
        /// Each picked file as `(filename, bytes)`. Empty if the user cancelled.
        files: Vec<(String, Vec<u8>)>,
    },
    /// A background task was launched by the system (BGTaskScheduler).
    BackgroundTaskStarted {
        /// The identifier of the task that was launched.
        identifier: String,
    },
}

/// The lifecycle phase of a continuous gesture such as a pan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GesturePhase {
    /// The gesture has just started.
    Began,
    /// The gesture is in progress (translation updated).
    Changed,
    /// The gesture finished (finger lifted / recognizer ended or cancelled).
    Ended,
}

/// The discriminant used to register and match handlers, independent of an
/// event's payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventKind {
    /// [`Event::Tap`].
    Tap,
    /// [`Event::DoubleTap`].
    DoubleTap,
    /// [`Event::LongPress`].
    LongPress,
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
    /// [`Event::PanChanged`].
    Pan,
    /// [`Event::Refresh`].
    Refresh,
    /// [`Event::Submit`].
    Submit,
    /// [`Event::BackPressed`].
    BackPressed,
    /// [`Event::KeyboardWillShow`].
    KeyboardWillShow,
    /// [`Event::KeyboardWillHide`].
    KeyboardWillHide,
    /// [`Event::AppLifecycle`].
    AppLifecycle,
    /// [`Event::QrDetected`].
    QrDetected,
    /// [`Event::PinchChanged`].
    Pinch,
    /// [`Event::DeepLink`].
    DeepLink,
    /// [`Event::BiometricResult`].
    BiometricResult,
    /// [`Event::RotateChanged`].
    RotateChanged,
    /// [`Event::LocationUpdated`].
    LocationUpdated,
    /// [`Event::LocationDenied`].
    LocationDenied,
    /// [`Event::MotionUpdated`].
    MotionUpdated,
    /// [`Event::MediaPicked`].
    MediaPicked,
    /// [`Event::MediaPickerCancelled`].
    MediaPickerCancelled,
    /// [`Event::DocumentPicked`].
    DocumentPicked,
    /// [`Event::BackgroundTaskStarted`].
    BackgroundTaskStarted,
}

impl Event {
    /// The kind discriminant of this event.
    pub fn kind(&self) -> EventKind {
        match self {
            Event::Tap { .. } => EventKind::Tap,
            Event::DoubleTap { .. } => EventKind::DoubleTap,
            Event::LongPress { .. } => EventKind::LongPress,
            Event::PointerDown { .. } => EventKind::PointerDown,
            Event::PointerMove { .. } => EventKind::PointerMove,
            Event::PointerUp { .. } => EventKind::PointerUp,
            Event::ScrollChanged { .. } => EventKind::ScrollChanged,
            Event::TextChanged { .. } => EventKind::TextChanged,
            Event::FocusChanged { .. } => EventKind::FocusChanged,
            Event::ValueChanged { .. } => EventKind::ValueChanged,
            Event::PanChanged { .. } => EventKind::Pan,
            Event::Refresh { .. } => EventKind::Refresh,
            Event::Submit { .. } => EventKind::Submit,
            Event::BackPressed => EventKind::BackPressed,
            Event::KeyboardWillShow { .. } => EventKind::KeyboardWillShow,
            Event::KeyboardWillHide => EventKind::KeyboardWillHide,
            Event::AppLifecycle(_) => EventKind::AppLifecycle,
            Event::QrDetected { .. } => EventKind::QrDetected,
            Event::PinchChanged { .. } => EventKind::Pinch,
            Event::DeepLink { .. } => EventKind::DeepLink,
            Event::BiometricResult { .. } => EventKind::BiometricResult,
            Event::RotateChanged { .. } => EventKind::RotateChanged,
            Event::LocationUpdated { .. } => EventKind::LocationUpdated,
            Event::LocationDenied => EventKind::LocationDenied,
            Event::MotionUpdated { .. } => EventKind::MotionUpdated,
            Event::MediaPicked { .. } => EventKind::MediaPicked,
            Event::MediaPickerCancelled => EventKind::MediaPickerCancelled,
            Event::DocumentPicked { .. } => EventKind::DocumentPicked,
            Event::BackgroundTaskStarted { .. } => EventKind::BackgroundTaskStarted,
        }
    }

    /// The widget this event targets, if it is a targeted (hit-tested) event.
    /// `None` for app-global events.
    pub fn target(&self) -> Option<WidgetId> {
        match *self {
            Event::Tap { target }
            | Event::DoubleTap { target }
            | Event::LongPress { target }
            | Event::PointerDown { target, .. }
            | Event::PointerMove { target, .. }
            | Event::PointerUp { target, .. }
            | Event::ScrollChanged { target, .. }
            | Event::TextChanged { target, .. }
            | Event::FocusChanged { target, .. }
            | Event::ValueChanged { target, .. }
            | Event::PanChanged { target, .. }
            | Event::Refresh { target }
            | Event::Submit { target } => Some(target),
            Event::QrDetected { target, .. } => Some(target),
            Event::PinchChanged { target, .. } => Some(target),
            Event::BackPressed
            | Event::KeyboardWillShow { .. }
            | Event::KeyboardWillHide
            | Event::AppLifecycle(_)
            | Event::DeepLink { .. }
            | Event::BiometricResult { .. }
            | Event::LocationUpdated { .. }
            | Event::LocationDenied
            | Event::MotionUpdated { .. }
            | Event::MediaPicked { .. }
            | Event::MediaPickerCancelled
            | Event::DocumentPicked { .. }
            | Event::BackgroundTaskStarted { .. } => None,
            Event::RotateChanged { target, .. } => Some(target),
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
