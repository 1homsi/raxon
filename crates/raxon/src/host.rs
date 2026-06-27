//! Shared host-session lifecycle for generated platform glue.
//!
//! Android JNI bindings, browser JavaScript shims, and future desktop hosts all
//! need to drive the same loop: deliver host events, advance the app, and drain
//! platform commands. This module keeps that orchestration in one place so each
//! platform backend only supplies its command encoder and event dispatcher.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fmt;

use crate::core::Size;
use crate::wire::WireProtocolError;

/// Current JSON bridge protocol version for host-session requests.
pub const HOST_BRIDGE_PROTOCOL_VERSION: u32 = 1;

/// Error returned by host-session JSON entry points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostSessionError {
    /// A generated host binding sent malformed request JSON.
    RequestJson(String),
    /// A generated host binding used an unsupported bridge version.
    UnsupportedBridgeVersion {
        /// Version supported by this runtime.
        expected: u32,
        /// Version sent by the generated host binding.
        found: u32,
    },
    /// A generated host binding referenced a session handle that no longer exists.
    UnknownSession {
        /// Opaque host-session handle.
        handle: u64,
    },
    /// A host-originated event batch could not be decoded or was unsupported.
    Event(WireProtocolError),
    /// A platform command batch could not be encoded.
    CommandJson(String),
    /// A host bridge response could not be encoded.
    ResponseJson(String),
}

impl fmt::Display for HostSessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HostSessionError::RequestJson(message) => {
                write!(f, "invalid host request JSON: {message}")
            }
            HostSessionError::UnsupportedBridgeVersion { expected, found } => {
                write!(
                    f,
                    "unsupported host bridge protocol version {found}; expected {expected}"
                )
            }
            HostSessionError::UnknownSession { handle } => {
                write!(f, "unknown host session handle {handle}")
            }
            HostSessionError::Event(error) => write!(f, "{error}"),
            HostSessionError::CommandJson(message) => {
                write!(f, "failed to encode host command batch: {message}")
            }
            HostSessionError::ResponseJson(message) => {
                write!(f, "failed to encode host response: {message}")
            }
        }
    }
}

impl std::error::Error for HostSessionError {}

impl From<WireProtocolError> for HostSessionError {
    fn from(error: WireProtocolError) -> Self {
        HostSessionError::Event(error)
    }
}

impl From<serde_json::Error> for HostSessionError {
    fn from(error: serde_json::Error) -> Self {
        HostSessionError::CommandJson(error.to_string())
    }
}

/// A platform driver that can be advanced through a shared host transport loop.
pub trait HostDriver {
    /// Advances the app by one frame.
    fn tick(&mut self);

    /// Updates the host viewport in logical pixels.
    fn set_viewport(&mut self, viewport: Size);

    /// Decodes and enqueues a versioned JSON event batch from the host.
    fn dispatch_wire_event_batch_json(&self, payload: &str) -> Result<(), WireProtocolError>;

    /// Drains pending platform commands as a host-facing JSON batch.
    fn drain_command_batch_json(&self) -> Result<String, serde_json::Error>;
}

/// A running app session owned by generated platform glue.
pub struct HostSession<D> {
    driver: D,
}

impl<D> HostSession<D> {
    /// Creates a host session around a platform-specific driver.
    pub fn new(driver: D) -> Self {
        HostSession { driver }
    }

    /// Returns shared access to the platform driver.
    pub fn driver(&self) -> &D {
        &self.driver
    }

    /// Returns mutable access to the platform driver.
    pub fn driver_mut(&mut self) -> &mut D {
        &mut self.driver
    }

    /// Consumes the session and returns the wrapped platform driver.
    pub fn into_driver(self) -> D {
        self.driver
    }
}

impl<D: HostDriver> HostSession<D> {
    /// Advances one frame without draining commands.
    pub fn tick(&mut self) {
        self.driver.tick();
    }

    /// Updates the viewport in logical pixels.
    pub fn set_viewport(&mut self, viewport: Size) {
        self.driver.set_viewport(viewport);
    }

    /// Updates the viewport from raw logical dimensions.
    pub fn set_viewport_size(&mut self, width: f32, height: f32) {
        self.set_viewport(Size::new(width, height));
    }

    /// Enqueues one decoded host event batch for delivery on the next tick.
    pub fn dispatch_event_batch_json(&self, payload: &str) -> Result<(), WireProtocolError> {
        self.driver.dispatch_wire_event_batch_json(payload)
    }

    /// Drains pending commands without advancing a frame.
    pub fn drain_command_batch_json(&self) -> Result<String, serde_json::Error> {
        self.driver.drain_command_batch_json()
    }

    /// Advances one frame, then drains the resulting platform command batch.
    pub fn tick_and_drain_command_batch_json(&mut self) -> Result<String, serde_json::Error> {
        self.tick();
        self.drain_command_batch_json()
    }

    /// Delivers host events, advances one frame, and drains platform commands.
    pub fn dispatch_events_tick_and_drain_command_batch_json(
        &mut self,
        payload: &str,
    ) -> Result<String, HostSessionError> {
        self.dispatch_event_batch_json(payload)?;
        Ok(self.tick_and_drain_command_batch_json()?)
    }

    /// Resizes the viewport, advances one frame, and drains platform commands.
    pub fn resize_tick_and_drain_command_batch_json(
        &mut self,
        width: f32,
        height: f32,
    ) -> Result<String, serde_json::Error> {
        self.set_viewport_size(width, height);
        self.tick_and_drain_command_batch_json()
    }
}

/// Opaque session id passed through generated platform bindings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HostSessionHandle(u64);

impl HostSessionHandle {
    /// Creates a handle from a raw host-owned id.
    pub const fn from_raw(raw: u64) -> Self {
        HostSessionHandle(raw)
    }

    /// Returns the raw id suitable for JNI, wasm, or C ABI boundaries.
    pub const fn to_raw(self) -> u64 {
        self.0
    }
}

/// Owns host sessions behind stable opaque handles.
///
/// Generated JNI/JS glue can keep one registry per app process or wasm module,
/// hand `HostSessionHandle::to_raw()` across the platform boundary, and route
/// all subsequent resize/event/tick/drain calls back through this type.
pub struct HostSessionRegistry<D> {
    next_handle: u64,
    sessions: BTreeMap<HostSessionHandle, HostSession<D>>,
}

impl<D> Default for HostSessionRegistry<D> {
    fn default() -> Self {
        Self::new()
    }
}

impl<D> HostSessionRegistry<D> {
    /// Creates an empty host-session registry.
    pub fn new() -> Self {
        HostSessionRegistry {
            next_handle: 1,
            sessions: BTreeMap::new(),
        }
    }

    /// Inserts an already-mounted session and returns its opaque handle.
    pub fn insert_session(&mut self, session: HostSession<D>) -> HostSessionHandle {
        let handle = self.allocate_handle();
        self.sessions.insert(handle, session);
        handle
    }

    /// Inserts a platform driver by wrapping it in a [`HostSession`].
    pub fn insert_driver(&mut self, driver: D) -> HostSessionHandle {
        self.insert_session(HostSession::new(driver))
    }

    /// Removes a session, returning it to Rust if the handle was valid.
    pub fn remove(&mut self, handle: HostSessionHandle) -> Option<HostSession<D>> {
        self.sessions.remove(&handle)
    }

    /// Returns whether `handle` names a live session.
    pub fn contains(&self, handle: HostSessionHandle) -> bool {
        self.sessions.contains_key(&handle)
    }

    /// Number of live sessions.
    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    /// Whether no sessions are registered.
    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    /// Shared access to a live session.
    pub fn get(&self, handle: HostSessionHandle) -> Option<&HostSession<D>> {
        self.sessions.get(&handle)
    }

    /// Mutable access to a live session.
    pub fn get_mut(&mut self, handle: HostSessionHandle) -> Option<&mut HostSession<D>> {
        self.sessions.get_mut(&handle)
    }

    fn allocate_handle(&mut self) -> HostSessionHandle {
        loop {
            let raw = self.next_handle.max(1);
            self.next_handle = raw.checked_add(1).unwrap_or(1);
            let handle = HostSessionHandle(raw);
            if !self.sessions.contains_key(&handle) {
                return handle;
            }
        }
    }
}

impl<D: HostDriver> HostSessionRegistry<D> {
    /// Advances one session by a frame.
    pub fn tick(&mut self, handle: HostSessionHandle) -> Result<(), HostSessionError> {
        self.session_mut(handle)?.tick();
        Ok(())
    }

    /// Updates one session viewport in logical pixels.
    pub fn set_viewport_size(
        &mut self,
        handle: HostSessionHandle,
        width: f32,
        height: f32,
    ) -> Result<(), HostSessionError> {
        self.session_mut(handle)?.set_viewport_size(width, height);
        Ok(())
    }

    /// Enqueues a JSON event batch for one session.
    pub fn dispatch_event_batch_json(
        &self,
        handle: HostSessionHandle,
        payload: &str,
    ) -> Result<(), HostSessionError> {
        self.session(handle)?.dispatch_event_batch_json(payload)?;
        Ok(())
    }

    /// Drains pending command JSON for one session.
    pub fn drain_command_batch_json(
        &self,
        handle: HostSessionHandle,
    ) -> Result<String, HostSessionError> {
        Ok(self.session(handle)?.drain_command_batch_json()?)
    }

    /// Advances one session, then drains pending command JSON.
    pub fn tick_and_drain_command_batch_json(
        &mut self,
        handle: HostSessionHandle,
    ) -> Result<String, HostSessionError> {
        Ok(self
            .session_mut(handle)?
            .tick_and_drain_command_batch_json()?)
    }

    /// Delivers events, advances one frame, and drains command JSON for one session.
    pub fn dispatch_events_tick_and_drain_command_batch_json(
        &mut self,
        handle: HostSessionHandle,
        payload: &str,
    ) -> Result<String, HostSessionError> {
        self.session_mut(handle)?
            .dispatch_events_tick_and_drain_command_batch_json(payload)
    }

    /// Resizes one session, advances a frame, and drains command JSON.
    pub fn resize_tick_and_drain_command_batch_json(
        &mut self,
        handle: HostSessionHandle,
        width: f32,
        height: f32,
    ) -> Result<String, HostSessionError> {
        Ok(self
            .session_mut(handle)?
            .resize_tick_and_drain_command_batch_json(width, height)?)
    }

    /// Applies one decoded host bridge request.
    pub fn handle_request(
        &mut self,
        request: HostBridgeRequest,
    ) -> Result<HostBridgeResponse, HostSessionError> {
        match request {
            HostBridgeRequest::Destroy { handle } => {
                let handle = HostSessionHandle::from_raw(handle);
                let _ = self.session(handle)?;
                self.remove(handle);
                Ok(HostBridgeResponse::Destroyed {
                    handle: handle.to_raw(),
                })
            }
            HostBridgeRequest::SetViewport {
                handle,
                width,
                height,
            } => {
                self.set_viewport_size(HostSessionHandle::from_raw(handle), width, height)?;
                Ok(HostBridgeResponse::Ok)
            }
            HostBridgeRequest::DispatchEventBatch { handle, batch } => {
                let payload = batch
                    .encode_json()
                    .map_err(|error| HostSessionError::RequestJson(error.to_string()))?;
                self.dispatch_event_batch_json(HostSessionHandle::from_raw(handle), &payload)?;
                Ok(HostBridgeResponse::Ok)
            }
            HostBridgeRequest::DrainCommandBatch { handle } => {
                let json = self.drain_command_batch_json(HostSessionHandle::from_raw(handle))?;
                Ok(HostBridgeResponse::CommandBatch {
                    batch: command_batch_value(&json)?,
                })
            }
            HostBridgeRequest::NavigationDebugSnapshot { handle } => {
                let _ = self.session(HostSessionHandle::from_raw(handle))?;
                let snapshot = serde_json::to_value(crate::nav::navigation_debug_snapshot())
                    .map_err(|error| HostSessionError::ResponseJson(error.to_string()))?;
                Ok(HostBridgeResponse::NavigationDebugSnapshot { snapshot })
            }
            HostBridgeRequest::ApplyNavigationCommand { handle, command } => {
                let _ = self.session(HostSessionHandle::from_raw(handle))?;
                Ok(HostBridgeResponse::NavigationCommandOutcome {
                    outcome: crate::nav::apply_navigation_command(command),
                })
            }
            HostBridgeRequest::ApplyNavigationCommands { handle, commands } => {
                let _ = self.session(HostSessionHandle::from_raw(handle))?;
                Ok(HostBridgeResponse::NavigationCommandOutcomes {
                    outcomes: crate::nav::apply_navigation_commands(commands),
                })
            }
            HostBridgeRequest::TickAndDrainCommandBatch { handle } => {
                let json =
                    self.tick_and_drain_command_batch_json(HostSessionHandle::from_raw(handle))?;
                Ok(HostBridgeResponse::CommandBatch {
                    batch: command_batch_value(&json)?,
                })
            }
            HostBridgeRequest::DispatchEventsTickAndDrainCommandBatch { handle, batch } => {
                let payload = batch
                    .encode_json()
                    .map_err(|error| HostSessionError::RequestJson(error.to_string()))?;
                let json = self.dispatch_events_tick_and_drain_command_batch_json(
                    HostSessionHandle::from_raw(handle),
                    &payload,
                )?;
                Ok(HostBridgeResponse::CommandBatch {
                    batch: command_batch_value(&json)?,
                })
            }
            HostBridgeRequest::ResizeTickAndDrainCommandBatch {
                handle,
                width,
                height,
            } => {
                let json = self.resize_tick_and_drain_command_batch_json(
                    HostSessionHandle::from_raw(handle),
                    width,
                    height,
                )?;
                Ok(HostBridgeResponse::CommandBatch {
                    batch: command_batch_value(&json)?,
                })
            }
        }
    }

    /// Applies one JSON host bridge request and returns a JSON bridge response.
    pub fn handle_request_json(&mut self, payload: &str) -> Result<String, HostSessionError> {
        let request: HostBridgeJsonRequest = serde_json::from_str(payload)
            .map_err(|error| HostSessionError::RequestJson(error.to_string()))?;
        request.ensure_supported()?;
        let response = self.handle_request(request.request)?;
        serde_json::to_string(&HostBridgeJsonResponse::new(response))
            .map_err(|error| HostSessionError::ResponseJson(error.to_string()))
    }

    /// Applies one JSON host bridge request and returns a JSON reply envelope.
    ///
    /// Unlike [`handle_request_json`](Self::handle_request_json), this method
    /// does not expose Rust `Result` errors to generated bindings. Malformed
    /// requests, unsupported protocol versions, and missing handles become a
    /// structured `status = "error"` JSON payload that JNI/JS glue can forward
    /// without panicking or inventing a parallel error protocol.
    pub fn handle_request_json_reply(&mut self, payload: &str) -> String {
        match self.handle_request_json_reply_inner(payload) {
            Ok(reply) => reply.encode_json(),
            Err(error) => HostBridgeJsonReply::error(error).encode_json(),
        }
    }

    fn handle_request_json_reply_inner(
        &mut self,
        payload: &str,
    ) -> Result<HostBridgeJsonReply, HostSessionError> {
        let request: HostBridgeJsonRequest = serde_json::from_str(payload)
            .map_err(|error| HostSessionError::RequestJson(error.to_string()))?;
        request.ensure_supported()?;
        let response = self.handle_request(request.request)?;
        Ok(HostBridgeJsonReply::ok(response))
    }

    fn session(&self, handle: HostSessionHandle) -> Result<&HostSession<D>, HostSessionError> {
        self.sessions
            .get(&handle)
            .ok_or(HostSessionError::UnknownSession {
                handle: handle.to_raw(),
            })
    }

    fn session_mut(
        &mut self,
        handle: HostSessionHandle,
    ) -> Result<&mut HostSession<D>, HostSessionError> {
        self.sessions
            .get_mut(&handle)
            .ok_or(HostSessionError::UnknownSession {
                handle: handle.to_raw(),
            })
    }
}

/// Binding-owned host runtime for generated JNI/JS glue.
///
/// This thin wrapper centralizes the registry and JSON error-envelope behavior
/// that platform bindings need. Platform modules add mount helpers for their
/// concrete drivers, while generated glue can keep this type as process/module
/// state and route every frame through one JSON request.
pub struct HostBridge<D> {
    registry: HostSessionRegistry<D>,
}

impl<D> Default for HostBridge<D> {
    fn default() -> Self {
        Self::new()
    }
}

impl<D> HostBridge<D> {
    /// Creates an empty binding runtime.
    pub fn new() -> Self {
        HostBridge {
            registry: HostSessionRegistry::new(),
        }
    }

    /// Creates a binding runtime around an existing session registry.
    pub fn with_registry(registry: HostSessionRegistry<D>) -> Self {
        HostBridge { registry }
    }

    /// Returns shared access to the underlying session registry.
    pub fn registry(&self) -> &HostSessionRegistry<D> {
        &self.registry
    }

    /// Returns mutable access to the underlying session registry.
    pub fn registry_mut(&mut self) -> &mut HostSessionRegistry<D> {
        &mut self.registry
    }

    /// Inserts an already-mounted session and returns its opaque handle.
    pub fn insert_session(&mut self, session: HostSession<D>) -> HostSessionHandle {
        self.registry.insert_session(session)
    }

    /// Inserts a platform driver by wrapping it in a [`HostSession`].
    pub fn insert_driver(&mut self, driver: D) -> HostSessionHandle {
        self.registry.insert_driver(driver)
    }

    /// Removes a session, returning it to Rust if the handle was valid.
    pub fn remove(&mut self, handle: HostSessionHandle) -> Option<HostSession<D>> {
        self.registry.remove(handle)
    }

    /// Returns whether `handle` names a live session.
    pub fn contains(&self, handle: HostSessionHandle) -> bool {
        self.registry.contains(handle)
    }

    /// Number of live sessions.
    pub fn len(&self) -> usize {
        self.registry.len()
    }

    /// Whether no sessions are registered.
    pub fn is_empty(&self) -> bool {
        self.registry.is_empty()
    }
}

impl<D: HostDriver> HostBridge<D> {
    /// Applies one decoded host bridge request.
    pub fn handle_request(
        &mut self,
        request: HostBridgeRequest,
    ) -> Result<HostBridgeResponse, HostSessionError> {
        self.registry.handle_request(request)
    }

    /// Applies one JSON host bridge request and returns a JSON bridge response.
    pub fn handle_request_json(&mut self, payload: &str) -> Result<String, HostSessionError> {
        self.registry.handle_request_json(payload)
    }

    /// Applies one JSON host bridge request and returns a JSON reply envelope.
    pub fn handle_request_json_reply(&mut self, payload: &str) -> String {
        self.registry.handle_request_json_reply(payload)
    }
}

/// Versioned JSON envelope for host-originated bridge requests.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostBridgeJsonRequest {
    /// Bridge protocol version used by the generated host binding.
    pub protocol_version: u32,
    /// Decoded host request payload.
    #[serde(flatten)]
    pub request: HostBridgeRequest,
}

impl HostBridgeJsonRequest {
    /// Creates a request envelope using the current bridge protocol version.
    pub const fn new(request: HostBridgeRequest) -> Self {
        HostBridgeJsonRequest {
            protocol_version: HOST_BRIDGE_PROTOCOL_VERSION,
            request,
        }
    }

    /// Ensures this envelope uses the bridge version supported by this runtime.
    pub fn ensure_supported(&self) -> Result<(), HostSessionError> {
        if self.protocol_version == HOST_BRIDGE_PROTOCOL_VERSION {
            Ok(())
        } else {
            Err(HostSessionError::UnsupportedBridgeVersion {
                expected: HOST_BRIDGE_PROTOCOL_VERSION,
                found: self.protocol_version,
            })
        }
    }
}

/// A host-originated lifecycle request routed through a session registry.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HostBridgeRequest {
    /// Remove a session from its registry.
    Destroy {
        /// Opaque session handle.
        handle: u64,
    },
    /// Set the viewport without ticking.
    SetViewport {
        /// Opaque session handle.
        handle: u64,
        /// Viewport width in logical pixels.
        width: f32,
        /// Viewport height in logical pixels.
        height: f32,
    },
    /// Dispatch host events without ticking.
    DispatchEventBatch {
        /// Opaque session handle.
        handle: u64,
        /// Versioned host event batch.
        batch: crate::wire::WireEventBatch,
    },
    /// Drain currently queued platform commands.
    DrainCommandBatch {
        /// Opaque session handle.
        handle: u64,
    },
    /// Return the current navigation debug snapshot for a live session.
    NavigationDebugSnapshot {
        /// Opaque session handle.
        handle: u64,
    },
    /// Apply one host-originated navigation command to a live session router.
    ApplyNavigationCommand {
        /// Opaque session handle.
        handle: u64,
        /// Serializable navigation command.
        command: crate::nav::NavigationCommand,
    },
    /// Apply host-originated navigation commands in order to a live session router.
    ApplyNavigationCommands {
        /// Opaque session handle.
        handle: u64,
        /// Serializable navigation commands.
        commands: Vec<crate::nav::NavigationCommand>,
    },
    /// Tick a session and drain platform commands.
    TickAndDrainCommandBatch {
        /// Opaque session handle.
        handle: u64,
    },
    /// Dispatch events, tick, and drain platform commands.
    DispatchEventsTickAndDrainCommandBatch {
        /// Opaque session handle.
        handle: u64,
        /// Versioned host event batch.
        batch: crate::wire::WireEventBatch,
    },
    /// Resize, tick, and drain platform commands.
    ResizeTickAndDrainCommandBatch {
        /// Opaque session handle.
        handle: u64,
        /// Viewport width in logical pixels.
        width: f32,
        /// Viewport height in logical pixels.
        height: f32,
    },
}

/// Versioned JSON envelope for host bridge responses.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostBridgeJsonResponse {
    /// Bridge protocol version used by this runtime.
    pub protocol_version: u32,
    /// Decoded host response payload.
    #[serde(flatten)]
    pub response: HostBridgeResponse,
}

impl HostBridgeJsonResponse {
    /// Creates a response envelope using the current bridge protocol version.
    pub const fn new(response: HostBridgeResponse) -> Self {
        HostBridgeJsonResponse {
            protocol_version: HOST_BRIDGE_PROTOCOL_VERSION,
            response,
        }
    }
}

/// Versioned JSON reply envelope for FFI-style generated bindings.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostBridgeJsonReply {
    /// Bridge protocol version used by this runtime.
    pub protocol_version: u32,
    /// Success or error payload.
    #[serde(flatten)]
    pub result: HostBridgeJsonReplyResult,
}

impl HostBridgeJsonReply {
    /// Creates a successful reply envelope.
    pub const fn ok(response: HostBridgeResponse) -> Self {
        HostBridgeJsonReply {
            protocol_version: HOST_BRIDGE_PROTOCOL_VERSION,
            result: HostBridgeJsonReplyResult::Ok { response },
        }
    }

    /// Creates an error reply envelope.
    pub fn error(error: HostSessionError) -> Self {
        HostBridgeJsonReply {
            protocol_version: HOST_BRIDGE_PROTOCOL_VERSION,
            result: HostBridgeJsonReplyResult::Error {
                error: HostBridgeJsonError::from_error(&error),
            },
        }
    }

    fn encode_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|error| {
            let fallback = HostBridgeJsonReply {
                protocol_version: HOST_BRIDGE_PROTOCOL_VERSION,
                result: HostBridgeJsonReplyResult::Error {
                    error: HostBridgeJsonError {
                        code: HostBridgeJsonErrorCode::ResponseJson,
                        message: error.to_string(),
                        handle: None,
                        expected_version: None,
                        found_version: None,
                    },
                },
            };
            serde_json::to_string(&fallback).unwrap_or_else(|_| {
                "{\"protocolVersion\":1,\"status\":\"error\",\"error\":{\"code\":\"response_json\",\"message\":\"failed to encode host bridge reply\"}}".to_string()
            })
        })
    }
}

/// Success or error body for a [`HostBridgeJsonReply`].
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum HostBridgeJsonReplyResult {
    /// The request succeeded.
    Ok {
        /// Successful response payload.
        #[serde(flatten)]
        response: HostBridgeResponse,
    },
    /// The request failed before a successful response could be produced.
    Error {
        /// Structured host bridge error.
        error: HostBridgeJsonError,
    },
}

/// Structured error payload returned by FFI-style JSON replies.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostBridgeJsonError {
    /// Stable machine-readable error code.
    pub code: HostBridgeJsonErrorCode,
    /// Human-readable diagnostic message.
    pub message: String,
    /// Session handle involved in the failure, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handle: Option<u64>,
    /// Runtime-supported protocol version, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_version: Option<u32>,
    /// Protocol version received from generated glue, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub found_version: Option<u32>,
}

impl HostBridgeJsonError {
    fn from_error(error: &HostSessionError) -> Self {
        match error {
            HostSessionError::RequestJson(message) => HostBridgeJsonError {
                code: HostBridgeJsonErrorCode::RequestJson,
                message: format!("invalid host request JSON: {message}"),
                handle: None,
                expected_version: None,
                found_version: None,
            },
            HostSessionError::UnsupportedBridgeVersion { expected, found } => HostBridgeJsonError {
                code: HostBridgeJsonErrorCode::UnsupportedBridgeVersion,
                message: error.to_string(),
                handle: None,
                expected_version: Some(*expected),
                found_version: Some(*found),
            },
            HostSessionError::UnknownSession { handle } => HostBridgeJsonError {
                code: HostBridgeJsonErrorCode::UnknownSession,
                message: error.to_string(),
                handle: Some(*handle),
                expected_version: None,
                found_version: None,
            },
            HostSessionError::Event(_) => HostBridgeJsonError {
                code: HostBridgeJsonErrorCode::Event,
                message: error.to_string(),
                handle: None,
                expected_version: None,
                found_version: None,
            },
            HostSessionError::CommandJson(message) => HostBridgeJsonError {
                code: HostBridgeJsonErrorCode::CommandJson,
                message: format!("failed to encode host command batch: {message}"),
                handle: None,
                expected_version: None,
                found_version: None,
            },
            HostSessionError::ResponseJson(message) => HostBridgeJsonError {
                code: HostBridgeJsonErrorCode::ResponseJson,
                message: format!("failed to encode host response: {message}"),
                handle: None,
                expected_version: None,
                found_version: None,
            },
        }
    }
}

/// Stable machine-readable host bridge error codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostBridgeJsonErrorCode {
    /// Request JSON was malformed.
    RequestJson,
    /// Request used a bridge protocol version this runtime does not support.
    UnsupportedBridgeVersion,
    /// Request referenced a session that is not live.
    UnknownSession,
    /// Host event JSON could not be decoded or validated.
    Event,
    /// Platform command JSON could not be encoded.
    CommandJson,
    /// Bridge response JSON could not be encoded.
    ResponseJson,
}

/// Host bridge response payload returned to generated platform glue.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HostBridgeResponse {
    /// The request succeeded and produced no command batch.
    Ok,
    /// A session was destroyed.
    Destroyed {
        /// Opaque session handle that was removed.
        handle: u64,
    },
    /// A host-facing platform command batch.
    CommandBatch {
        /// Command batch JSON as a nested value, not an escaped string.
        batch: serde_json::Value,
    },
    /// Current navigation state as a nested devtools JSON object.
    NavigationDebugSnapshot {
        /// Navigation debug snapshot JSON.
        snapshot: serde_json::Value,
    },
    /// Result of applying one navigation command.
    NavigationCommandOutcome {
        /// Navigation outcome payload.
        outcome: crate::nav::NavigationCommandOutcome,
    },
    /// Result of applying navigation commands in order.
    NavigationCommandOutcomes {
        /// Navigation outcome payloads.
        outcomes: Vec<crate::nav::NavigationCommandOutcome>,
    },
}

fn command_batch_value(json: &str) -> Result<serde_json::Value, HostSessionError> {
    serde_json::from_str(json).map_err(|error| HostSessionError::CommandJson(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    use serde_json::{json, Value};

    use crate::wire::{WireEvent, WireEventBatch};

    #[derive(Debug)]
    struct RecordingDriver {
        ticks: usize,
        viewport: Size,
        events: RefCell<Vec<WireEventBatch>>,
        command_batches: RefCell<Vec<Value>>,
    }

    impl RecordingDriver {
        fn new(command_batches: Vec<Value>) -> Self {
            RecordingDriver {
                ticks: 0,
                viewport: Size::ZERO,
                events: RefCell::new(Vec::new()),
                command_batches: RefCell::new(command_batches),
            }
        }
    }

    impl HostDriver for RecordingDriver {
        fn tick(&mut self) {
            self.ticks += 1;
        }

        fn set_viewport(&mut self, viewport: Size) {
            self.viewport = viewport;
        }

        fn dispatch_wire_event_batch_json(&self, payload: &str) -> Result<(), WireProtocolError> {
            self.events
                .borrow_mut()
                .push(WireEventBatch::decode_json(payload)?);
            Ok(())
        }

        fn drain_command_batch_json(&self) -> Result<String, serde_json::Error> {
            let mut batches = self.command_batches.borrow_mut();
            let batch = if batches.is_empty() {
                json!({ "commands": [] })
            } else {
                batches.remove(0)
            };
            serde_json::to_string(&batch)
        }
    }

    #[test]
    fn bridge_json_routes_resize_tick_and_nested_command_batch() {
        let mut registry = HostSessionRegistry::new();
        let handle = registry.insert_driver(RecordingDriver::new(vec![json!({
            "commands": [{ "kind": "mount", "id": 7 }]
        })]));

        let request = HostBridgeRequest::ResizeTickAndDrainCommandBatch {
            handle: handle.to_raw(),
            width: 375.0,
            height: 812.0,
        };
        let response = registry
            .handle_request_json(
                &serde_json::to_string(&HostBridgeJsonRequest::new(request))
                    .expect("request encodes"),
            )
            .expect("bridge request succeeds");

        assert_eq!(
            serde_json::from_str::<HostBridgeJsonResponse>(&response).expect("response decodes"),
            HostBridgeJsonResponse::new(HostBridgeResponse::CommandBatch {
                batch: json!({ "commands": [{ "kind": "mount", "id": 7 }] }),
            })
        );
        let driver = registry.get(handle).expect("session remains live").driver();
        assert_eq!(driver.ticks, 1);
        assert_eq!(driver.viewport, Size::new(375.0, 812.0));
    }

    #[test]
    fn bridge_json_returns_navigation_debug_snapshot_for_live_sessions() {
        let mut registry = HostSessionRegistry::new();
        let handle = registry.insert_driver(RecordingDriver::new(Vec::new()));
        crate::nav::reset_route("/orders/42?tab=items#notes");

        let request = HostBridgeJsonRequest::new(HostBridgeRequest::NavigationDebugSnapshot {
            handle: handle.to_raw(),
        });
        let response = registry
            .handle_request_json(&serde_json::to_string(&request).expect("request encodes"))
            .expect("bridge request succeeds");
        let response_json: Value = serde_json::from_str(&response).expect("response is JSON");
        let decoded =
            serde_json::from_str::<HostBridgeJsonResponse>(&response).expect("response decodes");

        assert_eq!(
            response_json["type"].as_str(),
            Some("navigation_debug_snapshot")
        );
        assert_eq!(
            response_json["snapshot"]["current"],
            "/orders/42?tab=items#notes"
        );
        assert_eq!(response_json["snapshot"]["location"]["path"], "/orders/42");
        assert_eq!(
            response_json["snapshot"]["location"]["queryAll"]["tab"][0],
            "items"
        );
        assert_eq!(response_json["snapshot"]["location"]["fragment"], "notes");
        assert_eq!(response_json["snapshot"]["routeFragment"], "notes");
        match decoded.response {
            HostBridgeResponse::NavigationDebugSnapshot { snapshot } => {
                assert_eq!(snapshot["historyDepth"], 1);
                assert_eq!(snapshot["canGoBack"], false);
            }
            other => panic!("expected navigation snapshot response, got {other:?}"),
        }

        assert_eq!(
            registry.handle_request(HostBridgeRequest::NavigationDebugSnapshot { handle: 999 }),
            Err(HostSessionError::UnknownSession { handle: 999 })
        );
    }

    #[test]
    fn bridge_json_applies_navigation_commands_for_live_sessions() {
        let mut registry = HostSessionRegistry::new();
        let handle = registry.insert_driver(RecordingDriver::new(Vec::new()));
        crate::nav::reset_route("/home");
        while crate::nav::dismiss_modal() {}

        let request = json!({
            "protocolVersion": HOST_BRIDGE_PROTOCOL_VERSION,
            "type": "apply_navigation_commands",
            "handle": handle.to_raw(),
            "commands": [
                { "type": "navigate", "route": "/orders" },
                {
                    "type": "set_query_param_values",
                    "key": "tab",
                    "values": ["items", "notes"],
                    "replace": true
                },
                { "type": "set_fragment", "fragment": "details", "replace": true },
                { "type": "present_modal", "route": "/filters" }
            ]
        });
        let response = registry
            .handle_request_json(&serde_json::to_string(&request).expect("request encodes"))
            .expect("bridge request succeeds");
        let response_json: Value = serde_json::from_str(&response).expect("response is JSON");
        let decoded =
            serde_json::from_str::<HostBridgeJsonResponse>(&response).expect("response decodes");

        assert_eq!(
            response_json["type"].as_str(),
            Some("navigation_command_outcomes")
        );
        assert_eq!(response_json["outcomes"].as_array().unwrap().len(), 4);
        assert_eq!(
            response_json["outcomes"][1]["kind"].as_str(),
            Some("set_query_param_values")
        );
        assert_eq!(
            response_json["outcomes"][3]["current"].as_str(),
            Some("/orders?tab=items&tab=notes#details")
        );
        assert_eq!(response_json["outcomes"][2]["location"]["path"], "/orders");
        assert_eq!(
            response_json["outcomes"][2]["location"]["queryAll"]["tab"],
            json!(["items", "notes"])
        );
        assert_eq!(
            response_json["outcomes"][2]["location"]["fragment"],
            "details"
        );
        assert_eq!(response_json["outcomes"][2]["routeFragment"], "details");
        assert_eq!(
            response_json["outcomes"][3]["location"]["fragment"],
            "details"
        );
        assert_eq!(response_json["outcomes"][3]["routeFragment"], "details");
        assert_eq!(response_json["outcomes"][3]["modals"][0], "/filters");
        match decoded.response {
            HostBridgeResponse::NavigationCommandOutcomes { outcomes } => {
                assert_eq!(outcomes.len(), 4);
                assert_eq!(
                    outcomes[0].kind,
                    crate::nav::NavigationCommandKind::Navigate
                );
                assert_eq!(
                    outcomes[1].kind,
                    crate::nav::NavigationCommandKind::SetQueryParamValues
                );
                assert_eq!(outcomes[1].location.query_value("tab"), Some("items"));
                assert_eq!(
                    outcomes[1].location.query_values("tab"),
                    Some(&["items".to_string(), "notes".to_string()][..])
                );
                assert_eq!(
                    outcomes[2].kind,
                    crate::nav::NavigationCommandKind::SetFragment
                );
                assert_eq!(outcomes[2].route_fragment.as_deref(), Some("details"));
                assert_eq!(
                    outcomes[3].kind,
                    crate::nav::NavigationCommandKind::PresentModal
                );
                assert_eq!(outcomes[3].current, "/orders?tab=items&tab=notes#details");
                assert_eq!(outcomes[3].location.fragment.as_deref(), Some("details"));
                assert_eq!(outcomes[3].modals, vec!["/filters".to_string()]);
            }
            other => panic!("expected navigation outcomes response, got {other:?}"),
        }

        let response = registry
            .handle_request(HostBridgeRequest::ApplyNavigationCommand {
                handle: handle.to_raw(),
                command: crate::nav::NavigationCommand::RemoveFragment { replace: true },
            })
            .expect("single command applies");
        match response {
            HostBridgeResponse::NavigationCommandOutcome { outcome } => {
                assert_eq!(
                    outcome.kind,
                    crate::nav::NavigationCommandKind::RemoveFragment
                );
                assert_eq!(outcome.current, "/orders?tab=items&tab=notes");
                assert_eq!(outcome.location.path, "/orders");
                assert_eq!(
                    outcome.location.query_values("tab"),
                    Some(&["items".to_string(), "notes".to_string()][..])
                );
                assert_eq!(outcome.route_fragment, None);
                assert_eq!(outcome.modals, vec!["/filters".to_string()]);
            }
            other => panic!("expected navigation outcome response, got {other:?}"),
        }

        let snapshot_request =
            HostBridgeJsonRequest::new(HostBridgeRequest::NavigationDebugSnapshot {
                handle: handle.to_raw(),
            });
        let snapshot_response = registry
            .handle_request_json(
                &serde_json::to_string(&snapshot_request).expect("request encodes"),
            )
            .expect("snapshot request succeeds");
        let snapshot_json: Value =
            serde_json::from_str(&snapshot_response).expect("snapshot response is JSON");
        assert_eq!(
            snapshot_json["snapshot"]["current"],
            "/orders?tab=items&tab=notes"
        );
        assert_eq!(snapshot_json["snapshot"]["location"]["path"], "/orders");
        assert_eq!(
            snapshot_json["snapshot"]["location"]["queryAll"]["tab"],
            json!(["items", "notes"])
        );
        assert_eq!(
            snapshot_json["snapshot"]["location"]["fragment"],
            Value::Null
        );
        assert_eq!(snapshot_json["snapshot"]["modals"][0], "/filters");

        assert_eq!(
            registry.handle_request(HostBridgeRequest::ApplyNavigationCommand {
                handle: 999,
                command: crate::nav::NavigationCommand::Back,
            }),
            Err(HostSessionError::UnknownSession { handle: 999 })
        );
        while crate::nav::dismiss_modal() {}
    }

    #[test]
    fn bridge_json_dispatches_events_and_destroy_errors_after_removal() {
        let mut registry = HostSessionRegistry::new();
        let handle = registry.insert_driver(RecordingDriver::new(Vec::new()));
        let batch = WireEventBatch::new(vec![WireEvent::Tap { target: 42 }]);

        let response = registry
            .handle_request(HostBridgeRequest::DispatchEventBatch {
                handle: handle.to_raw(),
                batch,
            })
            .expect("event dispatch succeeds");
        assert_eq!(response, HostBridgeResponse::Ok);
        assert_eq!(
            registry
                .get(handle)
                .expect("session remains live")
                .driver()
                .events
                .borrow()[0]
                .events,
            vec![WireEvent::Tap { target: 42 }]
        );

        let response = registry
            .handle_request(HostBridgeRequest::Destroy {
                handle: handle.to_raw(),
            })
            .expect("destroy succeeds");
        assert_eq!(
            response,
            HostBridgeResponse::Destroyed {
                handle: handle.to_raw(),
            }
        );
        assert_eq!(
            registry.handle_request(HostBridgeRequest::TickAndDrainCommandBatch {
                handle: handle.to_raw(),
            }),
            Err(HostSessionError::UnknownSession {
                handle: handle.to_raw(),
            })
        );
    }

    #[test]
    fn bridge_json_reports_invalid_request_json_and_unknown_handles() {
        let mut registry: HostSessionRegistry<RecordingDriver> = HostSessionRegistry::new();
        assert!(matches!(
            registry.handle_request_json("{not valid json"),
            Err(HostSessionError::RequestJson(_))
        ));
        assert_eq!(
            registry.handle_request_json(
                &serde_json::to_string(&HostBridgeJsonRequest {
                    protocol_version: HOST_BRIDGE_PROTOCOL_VERSION + 1,
                    request: HostBridgeRequest::DrainCommandBatch { handle: 99 },
                })
                .expect("request encodes")
            ),
            Err(HostSessionError::UnsupportedBridgeVersion {
                expected: HOST_BRIDGE_PROTOCOL_VERSION,
                found: HOST_BRIDGE_PROTOCOL_VERSION + 1,
            })
        );
        assert_eq!(
            registry.handle_request(HostBridgeRequest::DrainCommandBatch { handle: 99 }),
            Err(HostSessionError::UnknownSession { handle: 99 })
        );
    }

    #[test]
    fn host_bridge_owns_registry_and_returns_json_replies() {
        let mut bridge = HostBridge::new();
        let handle = bridge.insert_driver(RecordingDriver::new(vec![json!({
            "commands": [{ "kind": "frame", "id": 11 }]
        })]));
        assert!(bridge.contains(handle));

        let request = HostBridgeJsonRequest::new(HostBridgeRequest::TickAndDrainCommandBatch {
            handle: handle.to_raw(),
        });
        let reply = bridge
            .handle_request_json_reply(&serde_json::to_string(&request).expect("request encodes"));
        let reply_json: Value = serde_json::from_str(&reply).expect("reply is valid JSON");
        assert_eq!(reply_json["status"].as_str(), Some("ok"));
        assert_eq!(reply_json["type"].as_str(), Some("command_batch"));
        assert_eq!(
            serde_json::from_str::<HostBridgeJsonReply>(&reply).expect("reply decodes"),
            HostBridgeJsonReply::ok(HostBridgeResponse::CommandBatch {
                batch: json!({ "commands": [{ "kind": "frame", "id": 11 }] }),
            })
        );

        let reply = bridge.handle_request_json_reply(
            &serde_json::to_string(&HostBridgeJsonRequest::new(
                HostBridgeRequest::DrainCommandBatch { handle: 999 },
            ))
            .expect("request encodes"),
        );
        let reply_json: Value = serde_json::from_str(&reply).expect("reply is valid JSON");
        assert_eq!(reply_json["status"].as_str(), Some("error"));
        assert_eq!(
            reply_json["error"]["code"].as_str(),
            Some("unknown_session")
        );
        assert_eq!(
            serde_json::from_str::<HostBridgeJsonReply>(&reply).expect("reply decodes"),
            HostBridgeJsonReply::error(HostSessionError::UnknownSession { handle: 999 })
        );

        let reply = bridge.handle_request_json_reply("{not valid json");
        match serde_json::from_str::<HostBridgeJsonReply>(&reply)
            .expect("reply decodes")
            .result
        {
            HostBridgeJsonReplyResult::Error { error } => {
                assert_eq!(error.code, HostBridgeJsonErrorCode::RequestJson);
            }
            _ => panic!("expected error reply"),
        }
    }
}
