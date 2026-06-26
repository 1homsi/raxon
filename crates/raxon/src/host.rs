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

/// Error returned by host-session JSON entry points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostSessionError {
    /// A generated host binding referenced a session handle that no longer exists.
    UnknownSession {
        /// Opaque host-session handle.
        handle: u64,
    },
    /// A host-originated event batch could not be decoded or was unsupported.
    Event(WireProtocolError),
    /// A platform command batch could not be encoded.
    CommandJson(String),
}

impl fmt::Display for HostSessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HostSessionError::UnknownSession { handle } => {
                write!(f, "unknown host session handle {handle}")
            }
            HostSessionError::Event(error) => write!(f, "{error}"),
            HostSessionError::CommandJson(message) => {
                write!(f, "failed to encode host command batch: {message}")
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
