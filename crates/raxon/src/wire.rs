//! Shared host wire protocol for platform adapters.
//!
//! Backends drain commands out of Rust; hosts also need to send input,
//! lifecycle, and platform-service results back in. This module keeps that
//! inbound shape versioned and serializable so Android JNI glue, browser JS,
//! and future desktop hosts do not invent incompatible event payloads.

#![allow(missing_docs)]
#![forbid(unsafe_code)]

use std::fmt;
use std::sync::Arc;

use base64::Engine as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::core::{Point, Rect};
use crate::dom::{
    Event, GesturePhase, Lifecycle, NetworkStatus, PermissionKind, PermissionStatus, PointerId,
    TextSelection, WidgetId,
};

/// Current JSON wire protocol version for host-originated events.
pub const WIRE_PROTOCOL_VERSION: u32 = 1;

/// Error returned while decoding or validating host wire payloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WireProtocolError {
    Json(String),
    UnsupportedVersion { expected: u32, found: u32 },
}

impl fmt::Display for WireProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WireProtocolError::Json(message) => write!(f, "invalid wire JSON: {message}"),
            WireProtocolError::UnsupportedVersion { expected, found } => {
                write!(
                    f,
                    "unsupported wire protocol version {found}; expected {expected}"
                )
            }
        }
    }
}

impl std::error::Error for WireProtocolError {}

impl From<serde_json::Error> for WireProtocolError {
    fn from(error: serde_json::Error) -> Self {
        WireProtocolError::Json(error.to_string())
    }
}

/// A batch of host-originated events sent across the platform boundary.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireEventBatch {
    pub protocol_version: u32,
    pub events: Vec<WireEvent>,
}

impl Default for WireEventBatch {
    fn default() -> Self {
        WireEventBatch {
            protocol_version: WIRE_PROTOCOL_VERSION,
            events: Vec::new(),
        }
    }
}

impl WireEventBatch {
    /// Creates a batch using the current wire protocol version.
    pub fn new(events: Vec<WireEvent>) -> Self {
        WireEventBatch {
            protocol_version: WIRE_PROTOCOL_VERSION,
            events,
        }
    }

    /// Decodes and validates a JSON event batch.
    pub fn decode_json(payload: &str) -> Result<Self, WireProtocolError> {
        let batch: WireEventBatch = serde_json::from_str(payload)?;
        batch.ensure_supported()?;
        Ok(batch)
    }

    /// Encodes this event batch as JSON.
    pub fn encode_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Ensures this batch uses the protocol version supported by this runtime.
    pub fn ensure_supported(&self) -> Result<(), WireProtocolError> {
        if self.protocol_version == WIRE_PROTOCOL_VERSION {
            Ok(())
        } else {
            Err(WireProtocolError::UnsupportedVersion {
                expected: WIRE_PROTOCOL_VERSION,
                found: self.protocol_version,
            })
        }
    }

    /// Converts this batch into engine events after version validation.
    pub fn into_events(self) -> Result<Vec<Event>, WireProtocolError> {
        self.ensure_supported()?;
        Ok(self.events.into_iter().map(WireEvent::into_event).collect())
    }
}

/// A serializable event sent by a platform host into the raxon engine.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WireEvent {
    Tap {
        target: u64,
    },
    DoubleTap {
        target: u64,
    },
    LongPress {
        target: u64,
    },
    PointerDown {
        target: u64,
        x: f32,
        y: f32,
        pointer: u64,
    },
    PointerMove {
        target: u64,
        x: f32,
        y: f32,
        pointer: u64,
    },
    PointerUp {
        target: u64,
        x: f32,
        y: f32,
        pointer: u64,
    },
    ScrollChanged {
        target: u64,
        offset_x: f32,
        offset_y: f32,
    },
    TextChanged {
        target: u64,
        value: String,
        selection_start: usize,
        selection_end: usize,
    },
    FocusChanged {
        target: u64,
        focused: bool,
    },
    ValueChanged {
        target: u64,
        value: f64,
    },
    PanChanged {
        target: u64,
        translation_x: f32,
        translation_y: f32,
        velocity_x: f32,
        velocity_y: f32,
        phase: WireGesturePhase,
    },
    Refresh {
        target: u64,
    },
    Submit {
        target: u64,
    },
    AccessibilityAction {
        target: u64,
        action: String,
    },
    BackPressed,
    KeyboardWillShow {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    KeyboardWillHide,
    AppLifecycle {
        lifecycle: WireLifecycle,
    },
    QrDetected {
        target: u64,
        value: String,
    },
    PinchChanged {
        target: u64,
        scale: f32,
        velocity: f32,
        phase: WireGesturePhase,
    },
    RotateChanged {
        target: u64,
        rotation: f32,
        velocity: f32,
        phase: WireGesturePhase,
    },
    DeepLink {
        url: String,
    },
    BiometricResult {
        success: bool,
        error: Option<String>,
    },
    PermissionChanged {
        permission: PermissionKind,
        status: PermissionStatus,
    },
    NetworkStatusChanged {
        status: NetworkStatus,
    },
    LocationUpdated {
        latitude: f64,
        longitude: f64,
        accuracy: f64,
    },
    LocationDenied,
    MotionUpdated {
        accel_x: Option<f64>,
        accel_y: Option<f64>,
        accel_z: Option<f64>,
        gyro_x: Option<f64>,
        gyro_y: Option<f64>,
        gyro_z: Option<f64>,
    },
    MediaPicked {
        images: Vec<WireBytes>,
    },
    MediaPickerCancelled,
    DocumentPicked {
        files: Vec<WirePickedDocument>,
    },
    BackgroundTaskStarted {
        identifier: String,
    },
}

impl WireEvent {
    /// Decodes one host-originated event from JSON.
    pub fn decode_json(payload: &str) -> Result<Self, WireProtocolError> {
        Ok(serde_json::from_str(payload)?)
    }

    /// Encodes one host-originated event as JSON.
    pub fn encode_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Converts the serializable payload into the engine's event enum.
    pub fn into_event(self) -> Event {
        match self {
            WireEvent::Tap { target } => Event::Tap {
                target: widget(target),
            },
            WireEvent::DoubleTap { target } => Event::DoubleTap {
                target: widget(target),
            },
            WireEvent::LongPress { target } => Event::LongPress {
                target: widget(target),
            },
            WireEvent::PointerDown {
                target,
                x,
                y,
                pointer,
            } => Event::PointerDown {
                target: widget(target),
                position: Point::new(x, y),
                pointer: PointerId(pointer),
            },
            WireEvent::PointerMove {
                target,
                x,
                y,
                pointer,
            } => Event::PointerMove {
                target: widget(target),
                position: Point::new(x, y),
                pointer: PointerId(pointer),
            },
            WireEvent::PointerUp {
                target,
                x,
                y,
                pointer,
            } => Event::PointerUp {
                target: widget(target),
                position: Point::new(x, y),
                pointer: PointerId(pointer),
            },
            WireEvent::ScrollChanged {
                target,
                offset_x,
                offset_y,
            } => Event::ScrollChanged {
                target: widget(target),
                offset: Point::new(offset_x, offset_y),
            },
            WireEvent::TextChanged {
                target,
                value,
                selection_start,
                selection_end,
            } => Event::TextChanged {
                target: widget(target),
                value,
                selection: TextSelection {
                    start: selection_start,
                    end: selection_end,
                },
            },
            WireEvent::FocusChanged { target, focused } => Event::FocusChanged {
                target: widget(target),
                focused,
            },
            WireEvent::ValueChanged { target, value } => Event::ValueChanged {
                target: widget(target),
                value,
            },
            WireEvent::PanChanged {
                target,
                translation_x,
                translation_y,
                velocity_x,
                velocity_y,
                phase,
            } => Event::PanChanged {
                target: widget(target),
                translation: Point::new(translation_x, translation_y),
                velocity: Point::new(velocity_x, velocity_y),
                phase: phase.into(),
            },
            WireEvent::Refresh { target } => Event::Refresh {
                target: widget(target),
            },
            WireEvent::Submit { target } => Event::Submit {
                target: widget(target),
            },
            WireEvent::AccessibilityAction { target, action } => Event::AccessibilityAction {
                target: widget(target),
                action,
            },
            WireEvent::BackPressed => Event::BackPressed,
            WireEvent::KeyboardWillShow {
                x,
                y,
                width,
                height,
            } => Event::KeyboardWillShow {
                frame: Rect::new(x, y, width, height),
            },
            WireEvent::KeyboardWillHide => Event::KeyboardWillHide,
            WireEvent::AppLifecycle { lifecycle } => Event::AppLifecycle(lifecycle.into()),
            WireEvent::QrDetected { target, value } => Event::QrDetected {
                target: widget(target),
                value,
            },
            WireEvent::PinchChanged {
                target,
                scale,
                velocity,
                phase,
            } => Event::PinchChanged {
                target: widget(target),
                scale,
                velocity,
                phase: phase.into(),
            },
            WireEvent::RotateChanged {
                target,
                rotation,
                velocity,
                phase,
            } => Event::RotateChanged {
                target: widget(target),
                rotation,
                velocity,
                phase: phase.into(),
            },
            WireEvent::DeepLink { url } => Event::DeepLink { url },
            WireEvent::BiometricResult { success, error } => {
                Event::BiometricResult { success, error }
            }
            WireEvent::PermissionChanged { permission, status } => {
                Event::PermissionChanged { permission, status }
            }
            WireEvent::NetworkStatusChanged { status } => Event::NetworkStatusChanged { status },
            WireEvent::LocationUpdated {
                latitude,
                longitude,
                accuracy,
            } => Event::LocationUpdated {
                latitude,
                longitude,
                accuracy,
            },
            WireEvent::LocationDenied => Event::LocationDenied,
            WireEvent::MotionUpdated {
                accel_x,
                accel_y,
                accel_z,
                gyro_x,
                gyro_y,
                gyro_z,
            } => Event::MotionUpdated {
                accel_x,
                accel_y,
                accel_z,
                gyro_x,
                gyro_y,
                gyro_z,
            },
            WireEvent::MediaPicked { images } => Event::MediaPicked {
                images: images
                    .into_iter()
                    .map(|bytes| Arc::new(bytes.into_vec()))
                    .collect(),
            },
            WireEvent::MediaPickerCancelled => Event::MediaPickerCancelled,
            WireEvent::DocumentPicked { files } => Event::DocumentPicked {
                files: files
                    .into_iter()
                    .map(|file| (file.filename, file.bytes.into_vec()))
                    .collect(),
            },
            WireEvent::BackgroundTaskStarted { identifier } => {
                Event::BackgroundTaskStarted { identifier }
            }
        }
    }
}

impl From<Event> for WireEvent {
    fn from(event: Event) -> Self {
        match event {
            Event::Tap { target } => WireEvent::Tap {
                target: target.to_u64(),
            },
            Event::DoubleTap { target } => WireEvent::DoubleTap {
                target: target.to_u64(),
            },
            Event::LongPress { target } => WireEvent::LongPress {
                target: target.to_u64(),
            },
            Event::PointerDown {
                target,
                position,
                pointer,
            } => WireEvent::PointerDown {
                target: target.to_u64(),
                x: position.x,
                y: position.y,
                pointer: pointer.0,
            },
            Event::PointerMove {
                target,
                position,
                pointer,
            } => WireEvent::PointerMove {
                target: target.to_u64(),
                x: position.x,
                y: position.y,
                pointer: pointer.0,
            },
            Event::PointerUp {
                target,
                position,
                pointer,
            } => WireEvent::PointerUp {
                target: target.to_u64(),
                x: position.x,
                y: position.y,
                pointer: pointer.0,
            },
            Event::ScrollChanged { target, offset } => WireEvent::ScrollChanged {
                target: target.to_u64(),
                offset_x: offset.x,
                offset_y: offset.y,
            },
            Event::TextChanged {
                target,
                value,
                selection,
            } => WireEvent::TextChanged {
                target: target.to_u64(),
                value,
                selection_start: selection.start,
                selection_end: selection.end,
            },
            Event::FocusChanged { target, focused } => WireEvent::FocusChanged {
                target: target.to_u64(),
                focused,
            },
            Event::ValueChanged { target, value } => WireEvent::ValueChanged {
                target: target.to_u64(),
                value,
            },
            Event::PanChanged {
                target,
                translation,
                velocity,
                phase,
            } => WireEvent::PanChanged {
                target: target.to_u64(),
                translation_x: translation.x,
                translation_y: translation.y,
                velocity_x: velocity.x,
                velocity_y: velocity.y,
                phase: phase.into(),
            },
            Event::Refresh { target } => WireEvent::Refresh {
                target: target.to_u64(),
            },
            Event::Submit { target } => WireEvent::Submit {
                target: target.to_u64(),
            },
            Event::AccessibilityAction { target, action } => WireEvent::AccessibilityAction {
                target: target.to_u64(),
                action,
            },
            Event::BackPressed => WireEvent::BackPressed,
            Event::KeyboardWillShow { frame } => WireEvent::KeyboardWillShow {
                x: frame.origin.x,
                y: frame.origin.y,
                width: frame.size.width,
                height: frame.size.height,
            },
            Event::KeyboardWillHide => WireEvent::KeyboardWillHide,
            Event::AppLifecycle(lifecycle) => WireEvent::AppLifecycle {
                lifecycle: lifecycle.into(),
            },
            Event::QrDetected { target, value } => WireEvent::QrDetected {
                target: target.to_u64(),
                value,
            },
            Event::PinchChanged {
                target,
                scale,
                velocity,
                phase,
            } => WireEvent::PinchChanged {
                target: target.to_u64(),
                scale,
                velocity,
                phase: phase.into(),
            },
            Event::RotateChanged {
                target,
                rotation,
                velocity,
                phase,
            } => WireEvent::RotateChanged {
                target: target.to_u64(),
                rotation,
                velocity,
                phase: phase.into(),
            },
            Event::DeepLink { url } => WireEvent::DeepLink { url },
            Event::BiometricResult { success, error } => {
                WireEvent::BiometricResult { success, error }
            }
            Event::PermissionChanged { permission, status } => {
                WireEvent::PermissionChanged { permission, status }
            }
            Event::NetworkStatusChanged { status } => WireEvent::NetworkStatusChanged { status },
            Event::LocationUpdated {
                latitude,
                longitude,
                accuracy,
            } => WireEvent::LocationUpdated {
                latitude,
                longitude,
                accuracy,
            },
            Event::LocationDenied => WireEvent::LocationDenied,
            Event::MotionUpdated {
                accel_x,
                accel_y,
                accel_z,
                gyro_x,
                gyro_y,
                gyro_z,
            } => WireEvent::MotionUpdated {
                accel_x,
                accel_y,
                accel_z,
                gyro_x,
                gyro_y,
                gyro_z,
            },
            Event::MediaPicked { images } => WireEvent::MediaPicked {
                images: images
                    .into_iter()
                    .map(|bytes| WireBytes::new(bytes.as_ref().clone()))
                    .collect(),
            },
            Event::MediaPickerCancelled => WireEvent::MediaPickerCancelled,
            Event::DocumentPicked { files } => WireEvent::DocumentPicked {
                files: files
                    .into_iter()
                    .map(|(filename, bytes)| WirePickedDocument {
                        filename,
                        bytes: WireBytes::new(bytes),
                    })
                    .collect(),
            },
            Event::BackgroundTaskStarted { identifier } => {
                WireEvent::BackgroundTaskStarted { identifier }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireGesturePhase {
    Began,
    Changed,
    Ended,
}

impl From<WireGesturePhase> for GesturePhase {
    fn from(phase: WireGesturePhase) -> Self {
        match phase {
            WireGesturePhase::Began => GesturePhase::Began,
            WireGesturePhase::Changed => GesturePhase::Changed,
            WireGesturePhase::Ended => GesturePhase::Ended,
        }
    }
}

impl From<GesturePhase> for WireGesturePhase {
    fn from(phase: GesturePhase) -> Self {
        match phase {
            GesturePhase::Began => WireGesturePhase::Began,
            GesturePhase::Changed => WireGesturePhase::Changed,
            GesturePhase::Ended => WireGesturePhase::Ended,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireLifecycle {
    Resumed,
    Inactive,
    Backgrounded,
    Terminating,
}

impl From<WireLifecycle> for Lifecycle {
    fn from(lifecycle: WireLifecycle) -> Self {
        match lifecycle {
            WireLifecycle::Resumed => Lifecycle::Resumed,
            WireLifecycle::Inactive => Lifecycle::Inactive,
            WireLifecycle::Backgrounded => Lifecycle::Backgrounded,
            WireLifecycle::Terminating => Lifecycle::Terminating,
        }
    }
}

impl From<Lifecycle> for WireLifecycle {
    fn from(lifecycle: Lifecycle) -> Self {
        match lifecycle {
            Lifecycle::Resumed => WireLifecycle::Resumed,
            Lifecycle::Inactive => WireLifecycle::Inactive,
            Lifecycle::Backgrounded => WireLifecycle::Backgrounded,
            Lifecycle::Terminating => WireLifecycle::Terminating,
        }
    }
}

/// Binary payload encoded as base64 inside JSON wire messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireBytes(Vec<u8>);

impl WireBytes {
    /// Creates a new binary wire payload.
    pub fn new(bytes: Vec<u8>) -> Self {
        WireBytes(bytes)
    }

    /// Returns the raw bytes without consuming the payload.
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }

    /// Consumes the payload and returns the raw bytes.
    pub fn into_vec(self) -> Vec<u8> {
        self.0
    }
}

impl From<Vec<u8>> for WireBytes {
    fn from(bytes: Vec<u8>) -> Self {
        WireBytes::new(bytes)
    }
}

impl From<WireBytes> for Vec<u8> {
    fn from(bytes: WireBytes) -> Self {
        bytes.into_vec()
    }
}

impl Serialize for WireBytes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&base64::engine::general_purpose::STANDARD.encode(&self.0))
    }
}

impl<'de> Deserialize<'de> for WireBytes {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map(WireBytes)
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WirePickedDocument {
    pub filename: String,
    pub bytes: WireBytes,
}

fn widget(bits: u64) -> WidgetId {
    WidgetId::from_u64(bits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_event_json_round_trips_to_engine_event() {
        let target = WidgetId::from_u64(0x0000_0007_0000_0002);
        let event = WireEvent::TextChanged {
            target: target.to_u64(),
            value: "hello".to_string(),
            selection_start: 5,
            selection_end: 5,
        };

        let encoded = event.encode_json().expect("event encodes");
        let decoded = WireEvent::decode_json(&encoded).expect("event decodes");

        assert_eq!(
            decoded.into_event(),
            Event::TextChanged {
                target,
                value: "hello".to_string(),
                selection: TextSelection::caret(5),
            }
        );
    }

    #[test]
    fn accessibility_action_event_json_round_trips_to_engine_event() {
        let target = WidgetId::from_u64(0x0000_0012_0000_0003);
        let event = WireEvent::AccessibilityAction {
            target: target.to_u64(),
            action: "Archive".to_string(),
        };

        let encoded = event.encode_json().expect("event encodes");
        let decoded = WireEvent::decode_json(&encoded).expect("event decodes");

        assert_eq!(
            decoded.into_event(),
            Event::AccessibilityAction {
                target,
                action: "Archive".to_string(),
            }
        );
    }

    #[test]
    fn permission_event_json_round_trips_to_engine_event() {
        let event = WireEvent::PermissionChanged {
            permission: PermissionKind::Camera,
            status: PermissionStatus::Granted,
        };

        let encoded = event.encode_json().expect("event encodes");
        let decoded = WireEvent::decode_json(&encoded).expect("event decodes");

        assert_eq!(
            decoded.into_event(),
            Event::PermissionChanged {
                permission: PermissionKind::Camera,
                status: PermissionStatus::Granted,
            }
        );
    }

    #[test]
    fn network_status_event_json_round_trips_to_engine_event() {
        let event = WireEvent::NetworkStatusChanged {
            status: NetworkStatus::Offline,
        };

        let encoded = event.encode_json().expect("event encodes");
        let decoded = WireEvent::decode_json(&encoded).expect("event decodes");

        assert_eq!(
            decoded.into_event(),
            Event::NetworkStatusChanged {
                status: NetworkStatus::Offline,
            }
        );
    }

    #[test]
    fn batch_rejects_unsupported_protocol_versions() {
        let batch = WireEventBatch {
            protocol_version: WIRE_PROTOCOL_VERSION + 1,
            events: Vec::new(),
        };

        assert_eq!(
            batch.into_events(),
            Err(WireProtocolError::UnsupportedVersion {
                expected: WIRE_PROTOCOL_VERSION,
                found: WIRE_PROTOCOL_VERSION + 1,
            })
        );
    }

    #[test]
    fn batch_json_decodes_events_in_order() {
        let target = WidgetId::from_u64(0x0000_0010_0000_0001);
        let batch = WireEventBatch::new(vec![
            WireEvent::PointerDown {
                target: target.to_u64(),
                x: 12.0,
                y: 14.0,
                pointer: 9,
            },
            WireEvent::PointerUp {
                target: target.to_u64(),
                x: 12.0,
                y: 14.0,
                pointer: 9,
            },
        ]);

        let encoded = batch.encode_json().expect("batch encodes");
        let decoded = WireEventBatch::decode_json(&encoded).expect("batch decodes");
        let events = decoded.into_events().expect("version is supported");

        assert!(matches!(events[0], Event::PointerDown { .. }));
        assert!(matches!(events[1], Event::PointerUp { .. }));
    }

    #[test]
    fn binary_payloads_encode_as_base64_strings() {
        let event = WireEvent::DocumentPicked {
            files: vec![WirePickedDocument {
                filename: "scan.pdf".to_string(),
                bytes: WireBytes::new(vec![0, 1, 2, 255]),
            }],
        };

        let encoded = event.encode_json().expect("event encodes");
        assert!(encoded.contains("\"AAEC/w==\""));
        assert!(!encoded.contains("[0,1,2,255]"));

        let decoded = WireEvent::decode_json(&encoded).expect("event decodes");
        match decoded {
            WireEvent::DocumentPicked { files } => {
                assert_eq!(files[0].filename, "scan.pdf");
                assert_eq!(files[0].bytes.as_slice(), &[0, 1, 2, 255]);
            }
            _ => panic!("expected document event"),
        }
    }
}
