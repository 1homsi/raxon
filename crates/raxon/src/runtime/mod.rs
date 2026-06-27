//! The `rax` app driver: it owns the element tree, mounts the root view inside a
//! reactive ownership scope, runs layout, and drains platform events each frame.
//!
//! A platform backend creates an [`App`], hands it the viewport size, pushes
//! events through [`App::event_sink`], and calls [`App::tick`] once per frame
//! (driven by `CADisplayLink`/`Choreographer`). The runtime is intentionally
//! backend-agnostic — it talks only to the [`Host`] and the layout engine.

#![forbid(unsafe_code)]

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Mutex;

use crate::core::{Color, ColorScheme, EdgeInsets, Rect, Size};
use crate::dom::{Event, EventKind, EventSink, Host, Tree, WidgetId, WidgetKind};
use crate::reactive::{
    create_global_signal, create_memo, create_root, create_signal, provide_context, use_context,
    Memo, Scope, Signal,
};
use crate::view::layout::{update_layout_direction, LayoutDirection as AppLayoutDirection};
use crate::view::{mount, View};

// Re-export so callers can name the type without reaching into rax-dom.
pub use crate::dom::{
    HapticStyle, KeyboardType, Lifecycle, LocalNotification, NetworkStatus, PermissionKind,
    PermissionStatus, TextStyle,
};

thread_local! {
    /// Haptic pulses queued by [`haptic`] during event handlers. Drained by
    /// [`App::tick`] after events are processed.
    static PENDING_HAPTICS: std::cell::RefCell<Vec<crate::dom::HapticStyle>> =
        const { std::cell::RefCell::new(Vec::new()) };

    /// Document-picker requests queued by [`present_document_picker`]. Drained by
    /// [`App::tick`]. Each entry is the list of allowed UTType identifiers.
    static PENDING_DOCUMENT_PICKER: RefCell<Vec<Vec<String>>> =
        const { RefCell::new(Vec::new()) };

    /// Background task identifiers to register. Drained by [`App::tick`].
    static PENDING_BG_REGISTRATIONS: RefCell<Vec<String>> =
        const { RefCell::new(Vec::new()) };

    /// Background task schedule requests: (identifier, earliest_seconds). Drained by [`App::tick`].
    static PENDING_BG_SCHEDULES: RefCell<Vec<(String, f64)>> =
        const { RefCell::new(Vec::new()) };

    /// Deep-link handler registered by [`on_deep_link`].
    static DEEP_LINK_HANDLER: RefCell<Option<Box<dyn Fn(String)>>> =
        const { RefCell::new(None) };

    /// Notifications queued by [`schedule_notification`]. Drained by [`App::tick`].
    static PENDING_NOTIFICATIONS: RefCell<Vec<LocalNotification>> =
        const { RefCell::new(Vec::new()) };

    /// Cancellation ids queued by [`cancel_notification`]. Drained by [`App::tick`].
    static PENDING_CANCELLATIONS: RefCell<Vec<String>> =
        const { RefCell::new(Vec::new()) };

    /// Biometric authentication reasons queued by [`authenticate_biometric`]. Drained by [`App::tick`].
    static PENDING_BIOMETRICS: RefCell<Vec<String>> =
        const { RefCell::new(Vec::new()) };

    /// Permission status checks queued by [`check_permission`]. Drained by [`App::tick`].
    static PENDING_PERMISSION_CHECKS: RefCell<Vec<PermissionKind>> =
        const { RefCell::new(Vec::new()) };

    /// Permission prompts queued by [`request_permission`]. Drained by [`App::tick`].
    static PENDING_PERMISSION_REQUESTS: RefCell<Vec<PermissionKind>> =
        const { RefCell::new(Vec::new()) };

    /// Whether a location-start was requested (via [`start_location`]). Drained by [`App::tick`].
    static PENDING_LOCATION_STARTS: RefCell<bool> =
        const { RefCell::new(false) };

    /// Whether a location-stop was requested (via [`stop_location`]). Drained by [`App::tick`].
    static PENDING_LOCATION_STOPS: RefCell<bool> =
        const { RefCell::new(false) };

    /// Motion-start requests queued by [`start_motion`]. Drained by [`App::tick`].
    static PENDING_MOTION_STARTS: RefCell<Option<(bool, bool)>> =
        const { RefCell::new(None) };

    /// Whether a motion-stop was requested (via [`stop_motion`]). Drained by [`App::tick`].
    static PENDING_MOTION_STOPS: RefCell<bool> =
        const { RefCell::new(false) };

    /// In-process UI state (session lifetime only). Cross-restart persistence
    /// should be done via `raxon::store::store_set` / `store_get`.
    static UI_STATE: RefCell<std::collections::HashMap<String, String>> =
        RefCell::new(std::collections::HashMap::new());

    /// Clipboard writes queued by [`set_clipboard`]. Drained by [`App::tick`].
    static PENDING_CLIPBOARD_WRITES: RefCell<Vec<String>> =
        const { RefCell::new(Vec::new()) };

    /// Share-sheet texts queued by [`share_text`]. Drained by [`App::tick`].
    static PENDING_SHARE_TEXTS: RefCell<Vec<String>> =
        const { RefCell::new(Vec::new()) };

    /// External URLs queued by [`open_external_url`]. Drained by [`App::tick`].
    static PENDING_EXTERNAL_URLS: RefCell<Vec<String>> =
        const { RefCell::new(Vec::new()) };

    /// Reactive signal for battery level [0.0–1.0]. Lazily initialised by
    /// [`use_battery_level`]; updated by the platform backend via [`update_battery`].
    static BATTERY_LEVEL: Cell<Option<Signal<f32>>> = const { Cell::new(None) };

    /// Reactive signal for charging state. Lazily initialised by
    /// [`use_battery_charging`]; updated by the platform backend via [`update_battery`].
    static BATTERY_CHARGING: Cell<Option<Signal<bool>>> = const { Cell::new(None) };

    /// Reactive signal for network reachability. Lazily initialised by
    /// [`use_network_status`]; updated by the platform backend via
    /// [`update_network_status`].
    static NETWORK_STATUS: Cell<Option<Signal<NetworkStatus>>> = const { Cell::new(None) };

    /// Reactive signal for app lifecycle state. Lazily initialised by
    /// [`use_app_lifecycle`]; updated by the platform backend via
    /// [`update_app_lifecycle`].
    static APP_LIFECYCLE: Cell<Option<Signal<Lifecycle>>> = const { Cell::new(None) };

    /// Reactive signal for the system locale/preferred language. Lazily
    /// initialised by [`use_system_locale`]; updated by platform hosts via
    /// [`Event::LocaleChanged`](crate::dom::Event::LocaleChanged).
    static SYSTEM_LOCALE: Cell<Option<Signal<String>>> = const { Cell::new(None) };

    /// Reactive signal for the current on-screen keyboard height in logical
    /// pixels. Zero when the keyboard is hidden. Lazily initialised by
    /// [`use_keyboard_height`]; updated by the platform backend via
    /// [`update_keyboard_height`].
    static KEYBOARD_HEIGHT: Cell<Option<Signal<f32>>> = const { Cell::new(None) };

    /// Reactive signal for the platform safe-area insets (notch, status bar,
    /// home indicator) in logical pixels. Lazily initialised by
    /// [`use_safe_area_insets`]; updated whenever the backend calls
    /// [`App::set_safe_area`].
    static SAFE_AREA_INSETS: Cell<Option<Signal<EdgeInsets>>> = const { Cell::new(None) };

    /// Reactive signal for the device's current GPS location. `None` until the
    /// first fix arrives (or if location permission is denied). Lazily
    /// initialised by [`use_location`]; updated by the platform backend via
    /// [`update_location`].
    static LOCATION: Cell<Option<Signal<Option<GeoLocation>>>> = const { Cell::new(None) };

    /// Reactive signal for the device's accelerometer readings. `None` when
    /// the sensor is not running. Lazily initialised by [`use_accelerometer`];
    /// updated by the platform backend via [`update_accelerometer`].
    static ACCELEROMETER: Cell<Option<Signal<Option<AccelerometerData>>>> = const { Cell::new(None) };

    /// Reactive signal for the device's gyroscope readings. `None` when the
    /// sensor is not running. Lazily initialised by [`use_gyroscope`]; updated
    /// by the platform backend via [`update_gyroscope`].
    static GYROSCOPE: Cell<Option<Signal<Option<GyroscopeData>>>> = const { Cell::new(None) };

    /// Reactive signal for the APNS push-notification device token. `None`
    /// until the app successfully registers. Lazily initialised by
    /// [`use_push_token`]; updated by [`update_push_token`] / cleared by
    /// [`clear_push_token`].
    static PUSH_TOKEN: Cell<Option<Signal<Option<String>>>> = const { Cell::new(None) };

    /// Reactive permission status signals keyed by permission kind.
    static PERMISSION_STATUS: RefCell<HashMap<PermissionKind, Signal<PermissionStatus>>> =
        RefCell::new(HashMap::new());

    /// Pending torch state queued by [`set_torch`]. `None` means no change;
    /// `Some(true/false)` is drained by [`App::tick`] and forwarded to the backend.
    static PENDING_TORCH: std::cell::RefCell<Option<bool>> =
        const { std::cell::RefCell::new(None) };

    /// Pending app-badge count queued by [`set_app_badge`]. Drained by [`App::tick`].
    static PENDING_APP_BADGE: std::cell::RefCell<Option<u32>> =
        const { std::cell::RefCell::new(None) };

    /// Whether `register_for_push` was called since the last tick. Drained by [`App::tick`].
    static PENDING_PUSH_REGISTRATION: Cell<bool> = const { Cell::new(false) };

    /// Monotonically incrementing frame counter, ticked once per [`App::tick`] call.
    static FRAME_COUNTER: Cell<u64> = const { Cell::new(0) };

    /// Reactive signal wrapping [`FRAME_COUNTER`], lazily initialised by
    /// [`use_frame_counter`] and updated on every tick.
    static FRAME_COUNTER_SIGNAL: Cell<Option<Signal<u64>>> = const { Cell::new(None) };
}

// ---------------------------------------------------------------------------
// Error overlay (dev mode)
// ---------------------------------------------------------------------------

/// Global storage for the last panic message, written from the panic hook.
/// Uses `Mutex<Option<String>>` because the hook fires from any thread.
static PANIC_MESSAGE: Mutex<Option<String>> = Mutex::new(None);

/// Installs a panic hook that captures the panic message so it can be surfaced
/// to the user via [`last_panic`] and, typically, [`crate::view::error_overlay`].
///
/// Call this **once** at the very start of `main`, before any other setup.
/// In release builds you can omit it; the hook is a no-op overhead in that
/// case. The original hook is preserved and still runs (so crash logs still
/// appear in the Xcode console).
///
/// ```no_run
/// use raxon::runtime::install_error_overlay;
///
/// install_error_overlay();
/// ```
pub fn install_error_overlay() {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let msg = info.to_string();
        if let Ok(mut guard) = PANIC_MESSAGE.lock() {
            *guard = Some(msg);
        }
        original(info);
    }));
}

/// Returns the panic message captured by [`install_error_overlay`], or `None`
/// if no panic has occurred (or the hook was not installed).
///
/// Pair with a reactive signal to drive [`crate::view::error_overlay`]:
///
/// ```no_run
/// use raxon::runtime::last_panic;
/// use raxon::reactive::create_signal;
///
/// let msg = create_signal(last_panic());
/// ```
pub fn last_panic() -> Option<String> {
    PANIC_MESSAGE.lock().ok()?.clone()
}

/// Triggers a haptic feedback pulse. Call from event handlers (tap callbacks,
/// gesture handlers, etc.). The pulse is delivered on the next frame tick.
///
/// ```no_run
/// use raxon::runtime::{haptic, HapticStyle};
///
/// haptic(HapticStyle::Medium);
/// ```
pub fn haptic(style: HapticStyle) {
    PENDING_HAPTICS.with(|q| q.borrow_mut().push(style));
}

/// Presents the system document picker. `types` are UTType identifiers
/// (e.g. `["public.pdf", "public.plain-text"]`); pass an empty `Vec` to allow
/// any file. The chosen files arrive as a global [`crate::dom::Event::DocumentPicked`]
/// carrying `(filename, bytes)` for each pick.
///
/// ```no_run
/// # use raxon::runtime::present_document_picker;
/// present_document_picker(vec!["public.pdf".into()]);
/// ```
pub fn present_document_picker(types: Vec<String>) {
    PENDING_DOCUMENT_PICKER.with(|q| q.borrow_mut().push(types));
}

/// Registers a handler for deep link URLs. The handler fires whenever the app
/// opens via a URL scheme or universal link.
///
/// ```no_run
/// use raxon::runtime::on_deep_link;
///
/// on_deep_link(|url| println!("opened with: {url}"));
/// ```
pub fn on_deep_link(handler: impl Fn(String) + 'static) {
    DEEP_LINK_HANDLER.with(|h| *h.borrow_mut() = Some(Box::new(handler)));
}

/// Schedules a local notification. The notification is delivered on the next
/// frame tick.
///
/// ```no_run
/// use raxon::runtime::{schedule_notification, LocalNotification};
///
/// schedule_notification(LocalNotification {
///     id: "reminder".to_string(),
///     title: "Hello".to_string(),
///     body: "World".to_string(),
///     delay_seconds: 5,
/// });
/// ```
pub fn schedule_notification(notif: LocalNotification) {
    PENDING_NOTIFICATIONS.with(|q| q.borrow_mut().push(notif));
}

/// Cancels a pending local notification by its identifier.
///
/// ```no_run
/// use raxon::runtime::cancel_notification;
///
/// cancel_notification("reminder");
/// ```
pub fn cancel_notification(id: impl Into<String>) {
    PENDING_CANCELLATIONS.with(|q| q.borrow_mut().push(id.into()));
}

/// Triggers a biometric authentication prompt (Face ID / Touch ID). The result
/// is delivered as a global `Event::BiometricResult`.
///
/// ```no_run
/// use raxon::runtime::authenticate_biometric;
///
/// authenticate_biometric("Confirm your identity");
/// ```
pub fn authenticate_biometric(reason: impl Into<String>) {
    PENDING_BIOMETRICS.with(|q| q.borrow_mut().push(reason.into()));
}

/// Returns a reactive signal for the latest known platform permission status.
///
/// The initial value is [`PermissionStatus::Unknown`]. Call
/// [`check_permission`] to refresh without prompting, or [`request_permission`]
/// to ask the user where the platform supports a prompt.
///
/// Must be called while building views under a running [`App`].
pub fn use_permission(permission: PermissionKind) -> Signal<PermissionStatus> {
    PERMISSION_STATUS.with(|statuses| {
        let mut statuses = statuses.borrow_mut();
        if let Some(sig) = statuses.get(&permission).copied() {
            return sig;
        }
        let sig = create_global_signal(PermissionStatus::Unknown);
        statuses.insert(permission, sig);
        sig
    })
}

/// Called by platform backends when a permission status is known.
/// App code should normally use [`check_permission`] or [`request_permission`].
pub fn update_permission(permission: PermissionKind, status: PermissionStatus) {
    PERMISSION_STATUS.with(|statuses| {
        if let Some(sig) = statuses.borrow().get(&permission).copied() {
            sig.set(status);
        }
    });
}

/// Checks a platform permission without showing a prompt. The result is
/// delivered as [`Event::PermissionChanged`] and updates [`use_permission`].
pub fn check_permission(permission: PermissionKind) {
    PENDING_PERMISSION_CHECKS.with(|q| q.borrow_mut().push(permission));
}

/// Requests a platform permission from the user. The result is delivered as
/// [`Event::PermissionChanged`] and updates [`use_permission`].
pub fn request_permission(permission: PermissionKind) {
    PENDING_PERMISSION_REQUESTS.with(|q| q.borrow_mut().push(permission));
}

// ---------------------------------------------------------------------------
// Device API helpers
// ---------------------------------------------------------------------------

/// GPS location fix reported by the platform backend.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeoLocation {
    /// Latitude in degrees (positive = north).
    pub latitude: f64,
    /// Longitude in degrees (positive = east).
    pub longitude: f64,
    /// Altitude in metres above sea level.
    pub altitude: f64,
    /// Horizontal accuracy in metres (lower is better).
    pub accuracy: f64,
    /// Current speed in metres per second (`-1` when unavailable).
    pub speed: f64,
}

/// Raw accelerometer reading (in g-force units).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AccelerometerData {
    /// Acceleration along the X axis.
    pub x: f64,
    /// Acceleration along the Y axis.
    pub y: f64,
    /// Acceleration along the Z axis.
    pub z: f64,
}

/// Raw gyroscope reading (in radians per second).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GyroscopeData {
    /// Rotation rate around the X axis.
    pub x: f64,
    /// Rotation rate around the Y axis.
    pub y: f64,
    /// Rotation rate around the Z axis.
    pub z: f64,
}

/// Returns a reactive `Signal<Option<GeoLocation>>` that holds the most
/// recent GPS fix from the device. The value is `None` until the first
/// location arrives or if permission is denied.
///
/// Call [`start_location`] to begin receiving updates.
///
/// Must be called while building views under a running [`App`].
pub fn use_location() -> Signal<Option<GeoLocation>> {
    LOCATION.with(|slot| {
        if let Some(sig) = slot.get() {
            return sig;
        }
        let sig = create_signal(None);
        slot.set(Some(sig));
        sig
    })
}

/// Called by the platform backend to push a new GPS fix into the reactive
/// signal exposed by [`use_location`]. App code should not call this directly.
pub fn update_location(loc: GeoLocation) {
    LOCATION.with(|slot| {
        if let Some(sig) = slot.get() {
            sig.set(Some(loc));
        }
    });
}

/// Returns a reactive `Signal<Option<AccelerometerData>>` that holds the
/// most recent accelerometer reading. `None` until motion updates are started
/// via [`start_motion`].
///
/// Must be called while building views under a running [`App`].
pub fn use_accelerometer() -> Signal<Option<AccelerometerData>> {
    ACCELEROMETER.with(|slot| {
        if let Some(sig) = slot.get() {
            return sig;
        }
        let sig = create_signal(None);
        slot.set(Some(sig));
        sig
    })
}

/// Called by the platform backend to push a new accelerometer reading into
/// the reactive signal exposed by [`use_accelerometer`]. App code should not
/// call this directly.
pub fn update_accelerometer(data: AccelerometerData) {
    ACCELEROMETER.with(|slot| {
        if let Some(sig) = slot.get() {
            sig.set(Some(data));
        }
    });
}

/// Returns a reactive `Signal<Option<GyroscopeData>>` that holds the most
/// recent gyroscope reading. `None` until motion updates are started via
/// [`start_motion`].
///
/// Must be called while building views under a running [`App`].
pub fn use_gyroscope() -> Signal<Option<GyroscopeData>> {
    GYROSCOPE.with(|slot| {
        if let Some(sig) = slot.get() {
            return sig;
        }
        let sig = create_signal(None);
        slot.set(Some(sig));
        sig
    })
}

/// Called by the platform backend to push a new gyroscope reading into the
/// reactive signal exposed by [`use_gyroscope`]. App code should not call
/// this directly.
pub fn update_gyroscope(data: GyroscopeData) {
    GYROSCOPE.with(|slot| {
        if let Some(sig) = slot.get() {
            sig.set(Some(data));
        }
    });
}

/// Enables or disables the device flashlight (torch). Applied on the next
/// frame tick so it is safe to call from within reactive closures or event handlers.
///
/// ```no_run
/// use raxon::runtime::set_torch;
///
/// set_torch(true); // torch on
/// set_torch(false); // torch off
/// ```
pub fn set_torch(on: bool) {
    PENDING_TORCH.with(|q| *q.borrow_mut() = Some(on));
}

/// Returns a reactive `Signal<Option<String>>` that holds the APNS device
/// push token (hex-encoded). `None` until the app registers and the OS
/// delivers a token.
///
/// Call [`register_for_push`] to request registration.
///
/// Must be called while building views under a running [`App`].
pub fn use_push_token() -> Signal<Option<String>> {
    PUSH_TOKEN.with(|slot| {
        if let Some(sig) = slot.get() {
            return sig;
        }
        let sig = create_signal(None);
        slot.set(Some(sig));
        sig
    })
}

/// Called by the platform backend when the OS delivers an APNS token.
/// App code should not call this directly.
pub fn update_push_token(token: String) {
    PUSH_TOKEN.with(|slot| {
        if let Some(sig) = slot.get() {
            sig.set(Some(token));
        }
    });
}

/// Clears the push token reactive signal (e.g. after the app unregisters
/// from remote notifications).
pub fn clear_push_token() {
    PUSH_TOKEN.with(|slot| {
        if let Some(sig) = slot.get() {
            sig.set(None);
        }
    });
}

/// Registers this app for Apple Push Notification Service (APNS) remote
/// notifications. On success the OS delivers a device token, which is pushed
/// into the signal returned by [`use_push_token`].
///
/// The registration request is applied on the next frame tick.
///
/// ```no_run
/// use raxon::runtime::register_for_push;
///
/// register_for_push();
/// ```
pub fn register_for_push() {
    PENDING_PUSH_REGISTRATION.with(|c| c.set(true));
}

/// Sets the numeric badge on the app's home-screen icon. Pass `0` to clear
/// the badge. Applied on the next frame tick.
///
/// ```no_run
/// use raxon::runtime::set_app_badge;
///
/// set_app_badge(3); // show "3"
/// set_app_badge(0); // clear badge
/// ```
pub fn set_app_badge(count: u32) {
    PENDING_APP_BADGE.with(|q| *q.borrow_mut() = Some(count));
}

/// Copy `text` to the system clipboard. The write is applied on the next frame
/// tick so it is safe to call from within reactive closures or event handlers.
///
/// ```no_run
/// use raxon::runtime::set_clipboard;
///
/// set_clipboard("Hello, clipboard!");
/// ```
pub fn set_clipboard(text: impl Into<String>) {
    PENDING_CLIPBOARD_WRITES.with(|v| v.borrow_mut().push(text.into()));
}

/// Present the system share sheet with `text`. The sheet is shown on the next
/// frame tick so it is safe to call from within reactive closures or event handlers.
///
/// ```no_run
/// use raxon::runtime::share_text;
///
/// share_text("Check out this link: https://example.com");
/// ```
pub fn share_text(text: impl Into<String>) {
    PENDING_SHARE_TEXTS.with(|v| v.borrow_mut().push(text.into()));
}

/// Ask the platform to open `url` with its default external handler. The request
/// is applied on the next frame tick so it is safe to call from event handlers.
///
/// ```no_run
/// use raxon::runtime::open_external_url;
///
/// open_external_url("https://example.com");
/// ```
pub fn open_external_url(url: impl Into<String>) {
    PENDING_EXTERNAL_URLS.with(|v| v.borrow_mut().push(url.into()));
}

/// Returns a reactive `Signal<f32>` whose value is the current battery charge
/// level in the range `[0.0, 1.0]`. The signal is updated by the platform
/// backend roughly once per second. Returns `1.0` if battery level is
/// unavailable on the current device or platform.
///
/// Must be called while building views under a running [`App`].
///
/// ```no_run
/// use raxon::runtime::use_battery_level;
///
/// let level = use_battery_level();
/// // level.get() → e.g. 0.85
/// ```
pub fn use_battery_level() -> Signal<f32> {
    BATTERY_LEVEL.with(|slot| {
        if let Some(sig) = slot.get() {
            return sig;
        }
        let sig = create_signal(1.0f32);
        slot.set(Some(sig));
        sig
    })
}

/// Returns a reactive `Signal<bool>` that is `true` when the device is
/// charging (plugged in or full) and `false` when unplugged. The signal is
/// updated by the platform backend roughly once per second.
///
/// Must be called while building views under a running [`App`].
///
/// ```no_run
/// use raxon::runtime::use_battery_charging;
///
/// let charging = use_battery_charging();
/// // charging.get() → true / false
/// ```
pub fn use_battery_charging() -> Signal<bool> {
    BATTERY_CHARGING.with(|slot| {
        if let Some(sig) = slot.get() {
            return sig;
        }
        let sig = create_signal(false);
        slot.set(Some(sig));
        sig
    })
}

/// Called by the platform backend to push a new battery reading into the
/// reactive signals exposed by [`use_battery_level`] and
/// [`use_battery_charging`]. App code should not call this directly.
pub fn update_battery(level: f32, charging: bool) {
    BATTERY_LEVEL.with(|slot| {
        if let Some(sig) = slot.get() {
            sig.set(level);
        }
    });
    BATTERY_CHARGING.with(|slot| {
        if let Some(sig) = slot.get() {
            sig.set(charging);
        }
    });
}

/// Returns a reactive `Signal<NetworkStatus>` that reflects the current
/// internet reachability of the device. The signal starts as
/// [`NetworkStatus::Unknown`] and is updated by the platform backend.
///
/// Must be called while building views under a running [`App`].
///
/// ```no_run
/// use raxon::runtime::{use_network_status, NetworkStatus};
///
/// let status = use_network_status();
/// // status.get() → NetworkStatus::WiFi
/// ```
pub fn use_network_status() -> Signal<NetworkStatus> {
    NETWORK_STATUS.with(|slot| {
        if let Some(sig) = slot.get() {
            return sig;
        }
        let sig = create_signal(NetworkStatus::Unknown);
        slot.set(Some(sig));
        sig
    })
}

/// Called by the platform backend to push a new network reachability reading
/// into the reactive signal exposed by [`use_network_status`]. App code should
/// not call this directly.
pub fn update_network_status(status: NetworkStatus) {
    NETWORK_STATUS.with(|slot| {
        if let Some(sig) = slot.get() {
            sig.set(status);
        }
    });
}

/// Returns a reactive `Signal<Lifecycle>` that reflects whether the app is
/// foregrounded, inactive, backgrounded, or terminating.
///
/// The signal starts as [`Lifecycle::Resumed`] because views are built while the
/// app is mounted and interactive. Platform hosts update it through
/// [`Event::AppLifecycle`](crate::dom::Event::AppLifecycle).
///
/// Must be called while building views under a running [`App`].
///
/// ```no_run
/// use raxon::runtime::{use_app_lifecycle, Lifecycle};
///
/// let lifecycle = use_app_lifecycle();
/// // lifecycle.get() -> Lifecycle::Resumed
/// ```
pub fn use_app_lifecycle() -> Signal<Lifecycle> {
    APP_LIFECYCLE.with(|slot| {
        if let Some(sig) = slot.get() {
            return sig;
        }
        let sig = create_signal(Lifecycle::Resumed);
        slot.set(Some(sig));
        sig
    })
}

/// Called by platform backends when the app lifecycle changes. App code should
/// normally read [`use_app_lifecycle`] instead of calling this directly.
pub fn update_app_lifecycle(lifecycle: Lifecycle) {
    APP_LIFECYCLE.with(|slot| {
        if let Some(sig) = slot.get() {
            sig.set(lifecycle);
        }
    });
}

/// Returns a reactive `Signal<String>` containing the platform's preferred
/// locale/language tag, such as `en-US` or `ar-LB`.
///
/// The initial value comes from [`crate::i18n::system_locale`] and is updated by
/// platform hosts through [`Event::LocaleChanged`](crate::dom::Event::LocaleChanged).
/// Locale updates also refresh the app-wide
/// [`use_layout_direction`](crate::view::layout::use_layout_direction) signal so
/// RTL languages can mirror layouts automatically.
///
/// Must be called while building views under a running [`App`].
pub fn use_system_locale() -> Signal<String> {
    SYSTEM_LOCALE.with(|slot| {
        if let Some(sig) = slot.get() {
            return sig;
        }
        let sig = create_signal(normalize_locale_tag(&crate::i18n::system_locale()));
        slot.set(Some(sig));
        sig
    })
}

/// Called by platform backends when the preferred locale changes. App code
/// should normally read [`use_system_locale`] instead of calling this directly.
pub fn update_system_locale(locale: impl Into<String>) {
    let locale = normalize_locale_tag(&locale.into());
    SYSTEM_LOCALE.with(|slot| {
        if let Some(sig) = slot.get() {
            sig.set(locale.clone());
        }
    });
    update_layout_direction(layout_direction_for_locale(&locale));
}

fn normalize_locale_tag(locale: &str) -> String {
    let tag = locale
        .split('.')
        .next()
        .unwrap_or(locale)
        .trim()
        .replace('_', "-");
    if tag.is_empty() {
        "en".to_string()
    } else {
        tag
    }
}

fn layout_direction_for_locale(locale: &str) -> AppLayoutDirection {
    let lang = locale
        .split(['-', '_'])
        .next()
        .unwrap_or(locale)
        .to_ascii_lowercase();
    match lang.as_str() {
        "ar" | "he" | "fa" | "ur" | "yi" | "ji" | "iw" | "ps" | "sd" | "ug" => {
            AppLayoutDirection::Rtl
        }
        _ => AppLayoutDirection::Ltr,
    }
}

/// Returns a reactive `Signal<f32>` whose value is the current on-screen
/// keyboard height in logical pixels. The signal is `0.0` when the keyboard is
/// hidden and a positive value (typically 260–350 pt) while it is shown.
///
/// This signal is updated by the platform backend whenever the keyboard shows
/// or hides. Use it to adjust your layout (e.g. add bottom padding) so focused
/// text fields are not obscured.
///
/// Must be called while building views under a running [`App`].
///
/// ```no_run
/// use raxon::runtime::use_keyboard_height;
///
/// let kbd = use_keyboard_height();
/// // kbd.get() → 0.0 when hidden, ~336.0 when the default keyboard is up
/// ```
pub fn use_keyboard_height() -> Signal<f32> {
    KEYBOARD_HEIGHT.with(|slot| {
        if let Some(sig) = slot.get() {
            return sig;
        }
        let sig = create_signal(0.0f32);
        slot.set(Some(sig));
        sig
    })
}

/// Called by the platform backend to push the current keyboard height into the
/// reactive signal exposed by [`use_keyboard_height`]. App code should not call
/// this directly.
pub fn update_keyboard_height(height: f32) {
    KEYBOARD_HEIGHT.with(|slot| {
        if let Some(sig) = slot.get() {
            sig.set(height);
        }
    });
}

/// Returns a reactive [`Signal<EdgeInsets>`] holding the platform safe-area
/// insets — the space taken by the notch, status bar, and home indicator.
/// Updated automatically whenever the device orientation or chrome changes.
///
/// Use this when you need the inset *values* (e.g. to pad a custom header by
/// the exact top inset); for simply keeping content clear of the unsafe
/// region, prefer the `safe_area_top` / `safe_area_bottom` / `safe_area_view`
/// view builders.
///
/// Must be called while building views under a running [`App`].
///
/// ```no_run
/// use raxon::runtime::use_safe_area_insets;
///
/// let insets = use_safe_area_insets();
/// // insets.get().top → e.g. 47.0 on a notched iPhone
/// ```
pub fn use_safe_area_insets() -> Signal<EdgeInsets> {
    SAFE_AREA_INSETS.with(|slot| {
        if let Some(sig) = slot.get() {
            return sig;
        }
        let sig = create_signal(EdgeInsets::ZERO);
        slot.set(Some(sig));
        sig
    })
}

/// Pushes new safe-area insets into the reactive signal exposed by
/// [`use_safe_area_insets`]. Called from [`App::set_safe_area`]; app code
/// should not call this directly.
pub fn update_safe_area_insets(insets: EdgeInsets) {
    SAFE_AREA_INSETS.with(|slot| {
        if let Some(sig) = slot.get() {
            sig.set(insets);
        }
    });
}

/// The fill shown behind the root — i.e. the safe-area region (notch, status
/// bar, home indicator) that app content does not cover.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Backdrop {
    /// A fixed color regardless of appearance.
    Solid(Color),
    /// Follows the system appearance: `light` in light mode, `dark` in dark.
    System {
        /// Color used in light mode.
        light: Color,
        /// Color used in dark mode.
        dark: Color,
    },
}

impl Backdrop {
    /// Resolves to a concrete color for the given system appearance.
    pub fn resolve(self, scheme: ColorScheme) -> Color {
        match self {
            Backdrop::Solid(c) => c,
            Backdrop::System { light, dark } => {
                if scheme.is_dark() {
                    dark
                } else {
                    light
                }
            }
        }
    }
}

/// Context handle the runtime provides so app code can set the backdrop while
/// building views (see [`set_backdrop`]).
#[derive(Clone)]
struct BackdropSlot(Rc<RefCell<Option<Backdrop>>>);

/// Context handle wrapping the reactive system color-scheme signal.
#[derive(Clone, Copy)]
struct ColorSchemeCtx(Signal<ColorScheme>);

/// Context handle wrapping the reactive high-contrast signal.
#[derive(Clone, Copy)]
struct HighContrastCtx(Signal<bool>);

/// Sets the [`Backdrop`] (the fill behind the safe area) from within view code.
///
/// Call this while building your root view; the running [`App`] picks it up.
/// Use [`Backdrop::System`] to auto-follow the OS light/dark appearance.
///
/// ```no_run
/// use raxon::runtime::{set_backdrop, Backdrop};
/// use raxon::core::Color;
///
/// set_backdrop(Backdrop::System {
///     light: Color::rgb(247, 248, 251),
///     dark: Color::rgb(10, 10, 12),
/// });
/// ```
pub fn set_backdrop(backdrop: Backdrop) {
    if let Some(slot) = use_context::<BackdropSlot>() {
        *slot.0.borrow_mut() = Some(backdrop);
    }
}

/// The reactive system color scheme (light/dark), for adapting app content.
///
/// Returns a signal that updates when the OS appearance changes. Must be called
/// while building views under a running [`App`].
pub fn use_color_scheme() -> Signal<ColorScheme> {
    use_context::<ColorSchemeCtx>()
        .map(|c| c.0)
        .expect("use_color_scheme must be called within a running App")
}

/// Whether the iOS "Increase Contrast" / "Darker System Colors" accessibility
/// setting is currently enabled. Returns a reactive signal that updates each
/// frame when the value changes.
///
/// Must be called while building views under a running [`App`].
pub fn use_high_contrast() -> Signal<bool> {
    use_context::<HighContrastCtx>()
        .map(|c| c.0)
        .expect("use_high_contrast must be called within a running App")
}

/// Starts GPS location updates. Results arrive as global `Event::LocationUpdated`.
///
/// Call from within a reactive context (e.g. inside a `create_effect` or mount
/// callback). Stop with [`stop_location`].
pub fn start_location() {
    PENDING_LOCATION_STARTS.with(|q| *q.borrow_mut() = true);
}

/// Stops GPS location updates.
pub fn stop_location() {
    PENDING_LOCATION_STOPS.with(|q| *q.borrow_mut() = true);
}

/// Starts CoreMotion accelerometer and/or gyroscope updates.
/// Results arrive as global `Event::MotionUpdated` on each frame tick (~60 Hz).
///
/// ```no_run
/// use raxon::runtime::start_motion;
///
/// start_motion(true, true); // enable both accelerometer and gyroscope
/// ```
pub fn start_motion(accelerometer: bool, gyroscope: bool) {
    PENDING_MOTION_STARTS.with(|q| *q.borrow_mut() = Some((accelerometer, gyroscope)));
}

/// Stops CoreMotion sensor updates.
///
/// ```no_run
/// use raxon::runtime::stop_motion;
///
/// stop_motion();
/// ```
pub fn stop_motion() {
    PENDING_MOTION_STOPS.with(|q| *q.borrow_mut() = true);
}

/// Registers a background task identifier with BGTaskScheduler.
///
/// Call once during app startup, before the first background task fires.
/// The identifier must also be listed in `BGTaskSchedulerPermittedIdentifiers`
/// in your app's Info.plist.
///
/// ```no_run
/// use raxon::runtime::register_background_task;
///
/// register_background_task("com.example.app.refresh");
/// ```
pub fn register_background_task(identifier: impl Into<String>) {
    PENDING_BG_REGISTRATIONS.with(|q| q.borrow_mut().push(identifier.into()));
}

/// Schedules the next execution of a registered background task.
///
/// `earliest_seconds` is the minimum number of seconds from now before the
/// system will run the task. The system may run it later.
///
/// ```no_run
/// use raxon::runtime::schedule_background_task;
///
/// schedule_background_task("com.example.app.refresh", 3600.0);
/// ```
pub fn schedule_background_task(identifier: impl Into<String>, earliest_seconds: f64) {
    PENDING_BG_SCHEDULES.with(|q| q.borrow_mut().push((identifier.into(), earliest_seconds)));
}

/// Saves a UI state value for the current session.
///
/// Use this to persist navigation state (current tab, stack depth, scroll
/// position, etc.) so it can be restored within a single app session.
///
/// For persistence across app restarts, use `raxon::store::store_set`.
///
/// ```no_run
/// use raxon::runtime::save_ui_state;
///
/// save_ui_state("selected_tab", "2");
/// ```
pub fn save_ui_state(key: impl Into<String>, value: impl Into<String>) {
    UI_STATE.with(|m| {
        m.borrow_mut().insert(key.into(), value.into());
    });
}

/// Restores a UI state value saved by [`save_ui_state`].
///
/// Returns `None` if the key was never set or was cleared.
///
/// ```no_run
/// use raxon::runtime::restore_ui_state;
///
/// let tab = restore_ui_state("selected_tab").unwrap_or_else(|| "0".to_string());
/// ```
pub fn restore_ui_state(key: impl Into<String>) -> Option<String> {
    UI_STATE.with(|m| m.borrow().get(&key.into()).cloned())
}

/// Clears a UI state value saved by [`save_ui_state`].
///
/// ```no_run
/// use raxon::runtime::clear_ui_state;
///
/// clear_ui_state("selected_tab");
/// ```
pub fn clear_ui_state(key: impl Into<String>) {
    UI_STATE.with(|m| {
        m.borrow_mut().remove(&key.into());
    });
}

/// A running application: a mounted view tree plus the per-frame drive loop.
pub struct App {
    tree: Tree,
    root: WidgetId,
    /// Owns all reactivity created while mounting; disposed when the app drops.
    scope: Option<Scope>,
    viewport: Size,
    /// Safe-area insets (notch, status bar, home indicator) reported by the
    /// platform. The root is laid out within the safe region and offset by the
    /// top-left inset, so apps never hardcode notch/home-indicator padding.
    safe_area: EdgeInsets,
    /// Height (logical px) currently obscured by the soft keyboard, folded into
    /// the bottom inset so focused fields stay visible. Zero when hidden.
    keyboard_inset: f32,
    /// The configured backdrop (set by app code via [`set_backdrop`]).
    backdrop: Rc<RefCell<Option<Backdrop>>>,
    /// Current system appearance, reflected into `scheme_signal`.
    color_scheme: ColorScheme,
    /// Reactive handle to the color scheme, read by [`use_color_scheme`].
    scheme_signal: Signal<ColorScheme>,
    /// Last backdrop color emitted, so we only emit on change.
    last_backdrop: Option<Color>,
    /// Last frame emitted per widget, so re-layout only emits real changes.
    frames: HashMap<WidgetId, Rect>,
    /// Last content size emitted per scroll widget.
    content_sizes: HashMap<WidgetId, Size>,
    /// Wall-clock of the previous tick, for animation deltas.
    last_tick: Option<crate::platform::Monotonic>,
    /// Whether the system high-contrast / darker-colors accessibility setting is on.
    high_contrast: bool,
    /// Reactive signal for high contrast, read by [`use_high_contrast`].
    high_contrast_signal: Signal<bool>,
}

impl App {
    /// Mounts the view produced by `make_view` against `host`, performs the
    /// initial layout for `viewport`, and returns the running app.
    ///
    /// `make_view` runs **inside** the app's reactive root scope, so any
    /// `provide_context` / theming / navigator setup it performs is visible to
    /// the whole tree.
    pub fn new<V: View>(host: Host, viewport: Size, make_view: impl FnOnce() -> V) -> App {
        let mut tree = Tree::new(host);
        let backdrop = Rc::new(RefCell::new(None));
        let backdrop_for_ctx = backdrop.clone();
        let mut scheme_slot = None;
        let mut high_contrast_slot = None;
        let (root, scope) = create_root(|| {
            // Provide the context handles before building, so view code can call
            // set_backdrop()/use_color_scheme() during construction.
            provide_context(BackdropSlot(backdrop_for_ctx));
            let scheme = create_signal(ColorScheme::Light);
            provide_context(ColorSchemeCtx(scheme));
            scheme_slot = Some(scheme);
            let hc = create_signal(false);
            provide_context(HighContrastCtx(hc));
            high_contrast_slot = Some(hc);
            mount(&mut tree, make_view())
        });
        let mut app = App {
            tree,
            root,
            scope: Some(scope),
            viewport,
            safe_area: EdgeInsets::ZERO,
            keyboard_inset: 0.0,
            backdrop,
            color_scheme: ColorScheme::Light,
            scheme_signal: scheme_slot.expect("create_root ran the builder"),
            last_backdrop: None,
            frames: HashMap::new(),
            content_sizes: HashMap::new(),
            last_tick: None,
            high_contrast: false,
            high_contrast_signal: high_contrast_slot.expect("create_root ran the builder"),
        };
        // Register a global handler that routes DeepLink events to the
        // thread-local DEEP_LINK_HANDLER set by on_deep_link().
        app.tree.on_global(EventKind::DeepLink, |event| {
            if let Event::DeepLink { url } = event {
                DEEP_LINK_HANDLER.with(|h| {
                    if let Some(handler) = h.borrow().as_ref() {
                        handler(url.clone());
                    }
                });
            }
        });
        app.tree.on_global(EventKind::PermissionChanged, |event| {
            if let Event::PermissionChanged { permission, status } = *event {
                update_permission(permission, status);
            }
        });
        app.tree
            .on_global(EventKind::NetworkStatusChanged, |event| {
                if let Event::NetworkStatusChanged { status } = *event {
                    update_network_status(status);
                }
            });
        app.tree.on_global(EventKind::AppLifecycle, |event| {
            if let Event::AppLifecycle(lifecycle) = *event {
                update_app_lifecycle(lifecycle);
            }
        });
        app.tree.on_global(EventKind::LocationUpdated, |event| {
            if let Event::LocationUpdated {
                latitude,
                longitude,
                accuracy,
            } = *event
            {
                update_location(GeoLocation {
                    latitude,
                    longitude,
                    altitude: 0.0,
                    accuracy,
                    speed: -1.0,
                });
            }
        });
        app.tree.on_global(EventKind::LocationDenied, |_event| {
            LOCATION.with(|slot| {
                if let Some(sig) = slot.get() {
                    sig.set(None);
                }
            });
        });
        app.tree.on_global(EventKind::MotionUpdated, |event| {
            if let Event::MotionUpdated {
                accel_x,
                accel_y,
                accel_z,
                gyro_x,
                gyro_y,
                gyro_z,
            } = *event
            {
                if let (Some(x), Some(y), Some(z)) = (accel_x, accel_y, accel_z) {
                    update_accelerometer(AccelerometerData { x, y, z });
                }
                if let (Some(x), Some(y), Some(z)) = (gyro_x, gyro_y, gyro_z) {
                    update_gyroscope(GyroscopeData { x, y, z });
                }
            }
        });
        app.tree.run_dynamic(); // materialize dynamic subtrees before first layout
        app.refresh_backdrop();
        app.relayout();
        app
    }

    /// The root widget of the mounted tree.
    pub fn root(&self) -> WidgetId {
        self.root
    }

    /// A `Send` handle the backend uses to enqueue platform events.
    pub fn event_sink(&self) -> EventSink {
        self.tree.event_sink()
    }

    /// Updates the viewport size (on rotation/resize) and re-lays-out.
    pub fn set_viewport(&mut self, size: Size) {
        if size != self.viewport {
            self.viewport = size;
            self.relayout();
        }
    }

    /// Updates the platform safe-area insets (notch, status bar, home
    /// indicator) and re-lays-out so content stays clear of them.
    pub fn set_safe_area(&mut self, insets: EdgeInsets) {
        if insets != self.safe_area {
            self.safe_area = insets;
            update_safe_area_insets(insets);
            self.relayout();
        }
    }

    /// Sets the height obscured by the soft keyboard (0 when hidden) and
    /// re-lays-out so focused content is pushed above it.
    pub fn set_keyboard_inset(&mut self, height: f32) {
        let height = height.max(0.0);
        if height != self.keyboard_inset {
            self.keyboard_inset = height;
            self.relayout();
        }
    }

    /// Updates the system appearance (light/dark). Pushes it into the reactive
    /// [`use_color_scheme`] signal so content adapts, and re-resolves a
    /// [`Backdrop::System`] backdrop.
    pub fn set_color_scheme(&mut self, scheme: ColorScheme) {
        if scheme != self.color_scheme {
            self.color_scheme = scheme;
            self.scheme_signal.set(scheme);
            self.refresh_backdrop();
        }
    }

    /// Updates the high-contrast / darker-colors accessibility state. Pushes it
    /// into the reactive [`use_high_contrast`] signal so content can adapt.
    pub fn set_high_contrast(&mut self, hc: bool) {
        if hc != self.high_contrast {
            self.high_contrast = hc;
            self.high_contrast_signal.set(hc);
        }
    }

    /// Sets the backdrop at runtime (app code normally calls [`set_backdrop`]
    /// during view construction instead).
    pub fn set_backdrop(&mut self, backdrop: Backdrop) {
        *self.backdrop.borrow_mut() = Some(backdrop);
        self.refresh_backdrop();
    }

    /// Re-resolves the configured backdrop against the current scheme and emits
    /// a mutation only when the resulting color changes.
    fn refresh_backdrop(&mut self) {
        let resolved = self.backdrop.borrow().map(|b| b.resolve(self.color_scheme));
        if let Some(color) = resolved {
            if self.last_backdrop != Some(color) {
                self.last_backdrop = Some(color);
                self.tree.set_backdrop(color);
            }
        }
    }

    /// Advances one frame: deliver queued events (which may write signals and
    /// emit paint mutations synchronously), then re-run layout and emit any
    /// changed frames.
    pub fn tick(&mut self) {
        crate::async_rt::run_until_stalled(); // advance async tasks (may resolve resources)

        // Increment the global frame counter and push it into the reactive signal.
        let new_frame = FRAME_COUNTER.with(|c| {
            let n = c.get().wrapping_add(1);
            c.set(n);
            n
        });
        FRAME_COUNTER_SIGNAL.with(|slot| {
            if let Some(sig) = slot.get() {
                sig.set(new_frame);
            }
        });

        // Advance animations by the wall-clock delta since the last frame.
        // `Monotonic` is wasm-safe; `std::time::Instant::now()` panics on web.
        let now = crate::platform::Monotonic::now();
        let dt = self
            .last_tick
            .map(|prev| now.secs_since(prev))
            .unwrap_or(0.0);
        self.last_tick = Some(now);
        crate::anim::tick(dt);

        while let Some(event) = self.tree.pop_event() {
            if let Event::AppearanceChanged {
                color_scheme,
                high_contrast,
            } = &event
            {
                self.set_color_scheme(*color_scheme);
                self.set_high_contrast(*high_contrast);
            }
            if let Event::LocaleChanged { locale } = &event {
                update_system_locale(locale.clone());
            }
            self.tree.dispatch(&event);
        }

        // Drain any haptic pulses queued by event handlers.
        let haptics: Vec<HapticStyle> = PENDING_HAPTICS.with(|q| {
            let mut v = q.borrow_mut();
            std::mem::take(&mut *v)
        });
        for style in haptics {
            self.tree.haptic(style);
        }

        // Drain any document-picker requests queued by app code.
        let doc_pickers: Vec<Vec<String>> = PENDING_DOCUMENT_PICKER.with(|q| {
            let mut v = q.borrow_mut();
            std::mem::take(&mut *v)
        });
        for types in doc_pickers {
            self.tree.present_document_picker(types);
        }

        // Drain any local notifications queued by app code.
        let notifs: Vec<LocalNotification> = PENDING_NOTIFICATIONS.with(|q| {
            let mut v = q.borrow_mut();
            std::mem::take(&mut *v)
        });
        for notif in notifs {
            self.tree.schedule_notification(notif);
        }

        // Drain any notification cancellations queued by app code.
        let cancels: Vec<String> = PENDING_CANCELLATIONS.with(|q| {
            let mut v = q.borrow_mut();
            std::mem::take(&mut *v)
        });
        for id in cancels {
            self.tree.cancel_notification(id);
        }

        // Drain any biometric authentication requests queued by app code.
        let biometrics: Vec<String> = PENDING_BIOMETRICS.with(|q| {
            let mut v = q.borrow_mut();
            std::mem::take(&mut *v)
        });
        for reason in biometrics {
            self.tree.authenticate_biometric(reason);
        }

        // Drain permission checks/requests queued by app code.
        let permission_checks: Vec<PermissionKind> = PENDING_PERMISSION_CHECKS.with(|q| {
            let mut v = q.borrow_mut();
            std::mem::take(&mut *v)
        });
        for permission in permission_checks {
            self.tree.check_permission(permission);
        }
        let permission_requests: Vec<PermissionKind> = PENDING_PERMISSION_REQUESTS.with(|q| {
            let mut v = q.borrow_mut();
            std::mem::take(&mut *v)
        });
        for permission in permission_requests {
            self.tree.request_permission(permission);
        }

        // Drain location start/stop requests.
        let want_start = PENDING_LOCATION_STARTS.with(|q| {
            let v = *q.borrow();
            *q.borrow_mut() = false;
            v
        });
        let want_stop = PENDING_LOCATION_STOPS.with(|q| {
            let v = *q.borrow();
            *q.borrow_mut() = false;
            v
        });
        if want_start {
            self.tree.start_location();
        }
        if want_stop {
            self.tree.stop_location();
        }

        // Drain motion start/stop requests.
        let motion_start = PENDING_MOTION_STARTS.with(|q| q.borrow_mut().take());
        let motion_stop = PENDING_MOTION_STOPS.with(|q| {
            let v = *q.borrow();
            *q.borrow_mut() = false;
            v
        });
        if let Some((accel, gyro)) = motion_start {
            self.tree.start_motion(accel, gyro);
        }
        if motion_stop {
            self.tree.stop_motion();
        }

        // Drain background task registrations.
        let bg_regs: Vec<String> =
            PENDING_BG_REGISTRATIONS.with(|q| std::mem::take(&mut *q.borrow_mut()));
        for id in bg_regs {
            self.tree.register_background_task(id);
        }

        // Drain background task schedule requests.
        let bg_scheds: Vec<(String, f64)> =
            PENDING_BG_SCHEDULES.with(|q| std::mem::take(&mut *q.borrow_mut()));
        for (id, secs) in bg_scheds {
            self.tree.schedule_background_task(id, secs);
        }

        // Drain clipboard writes queued by set_clipboard().
        let clipboard_writes: Vec<String> =
            PENDING_CLIPBOARD_WRITES.with(|q| std::mem::take(&mut *q.borrow_mut()));
        for text in clipboard_writes {
            self.tree.set_clipboard(text);
        }

        // Drain share-sheet texts queued by share_text().
        let share_texts: Vec<String> =
            PENDING_SHARE_TEXTS.with(|q| std::mem::take(&mut *q.borrow_mut()));
        for text in share_texts {
            self.tree.share_text(text);
        }

        // Drain external URLs queued by open_external_url().
        let external_urls: Vec<String> =
            PENDING_EXTERNAL_URLS.with(|q| std::mem::take(&mut *q.borrow_mut()));
        for url in external_urls {
            self.tree.open_external_url(url);
        }

        // Drain torch state queued by set_torch().
        if let Some(on) = PENDING_TORCH.with(|q| q.borrow_mut().take()) {
            self.tree.set_torch(on);
        }

        // Drain push-notification registration requests.
        if PENDING_PUSH_REGISTRATION.with(|c| c.replace(false)) {
            self.tree.register_for_push();
        }

        // Drain app-badge updates queued by set_app_badge().
        if let Some(count) = PENDING_APP_BADGE.with(|q| q.borrow_mut().take()) {
            self.tree.set_app_badge(count);
        }

        self.tree.run_dynamic(); // events/async/anim may have dirtied dynamic subtrees
        self.relayout();
    }

    /// Recomputes layout and emits only the frames (and scroll content sizes)
    /// that actually changed.
    fn relayout(&mut self) {
        // Lay the tree out within the safe region, then shift the root by the
        // top-left inset. Children are positioned relative to the root, so they
        // ride along — only the root frame needs the offset. The keyboard, when
        // up, obscures the bottom (including the home indicator), so take the
        // larger of the two as the effective bottom inset.
        let effective = EdgeInsets {
            bottom: self.safe_area.bottom.max(self.keyboard_inset),
            ..self.safe_area
        };
        let avail = self.viewport.deflate(effective);
        let computed = crate::layout::compute(&self.tree, self.root, avail);
        for (id, mut layout) in computed {
            if id == self.root {
                layout.frame.origin.x += self.safe_area.left;
                layout.frame.origin.y += self.safe_area.top;
            }
            if self.frames.get(&id) != Some(&layout.frame) {
                self.tree.set_frame(id, layout.frame);
                self.frames.insert(id, layout.frame);
            }
            if self.tree.kind_of(id) == Some(WidgetKind::Scroll)
                && self.content_sizes.get(&id) != Some(&layout.content)
            {
                self.tree.set_content_size(id, layout.content);
                self.content_sizes.insert(id, layout.content);
            }
        }
    }
}

/// Returns the `rax-runtime` package version string (e.g. `"0.1.0"`).
///
/// Useful for debug overlays or crash reports.
///
/// ```
/// use raxon::runtime::rax_version;
///
/// println!("rax {}", rax_version());
/// ```
pub fn rax_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

// ---------------------------------------------------------------------------
// Frame counter — timing utilities
// ---------------------------------------------------------------------------

/// Returns the raw (non-reactive) frame count since the app started.
///
/// Incremented once per [`App::tick`]. Useful for non-reactive timing logic,
/// e.g. throttling work to every N frames inside an effect.
///
/// ```no_run
/// use raxon::runtime::current_frame;
///
/// let frame = current_frame();
/// if frame % 60 == 0 {
///     // roughly once per second at 60 fps
/// }
/// ```
pub fn current_frame() -> u64 {
    FRAME_COUNTER.with(|c| c.get())
}

/// Returns a reactive [`Signal<u64>`] that increments by 1 on every frame tick.
///
/// The signal is a global singleton per thread — all callers share the same
/// handle. Subscribe to it in effects or memos to drive frame-rate UI.
///
/// Must be called while building views under a running [`App`].
///
/// ```no_run
/// use raxon::runtime::use_frame_counter;
///
/// let frame = use_frame_counter();
/// // frame.get() increases by 1 each tick
/// ```
pub fn use_frame_counter() -> Signal<u64> {
    FRAME_COUNTER_SIGNAL.with(|slot| {
        if let Some(sig) = slot.get() {
            return sig;
        }
        let sig = create_signal(0u64);
        slot.set(Some(sig));
        sig
    })
}

/// Returns a [`Memo<T>`] that only propagates a new value from `sig` after
/// the signal has been **stable** (unchanged) for at least `frames` consecutive
/// ticks.
///
/// This is a frame-aligned approximation of time-based debouncing. At 60 fps,
/// `frames = 18` is roughly 300 ms of quiet time.
///
/// The returned memo starts with the current value of `sig` and will not update
/// until `sig` has held the same value for `frames` ticks.
///
/// ```no_run
/// use raxon::runtime::{debounce, use_frame_counter};
/// use raxon::reactive::create_signal;
///
/// let query = create_signal(String::new());
/// let debounced = debounce(query, 18); // ~300 ms at 60 fps
/// ```
pub fn debounce<T: Clone + PartialEq + 'static>(sig: Signal<T>, frames: u32) -> Memo<T> {
    // Track: last value seen, how many frames it has been stable, and the
    // committed (output) value.  All state lives inside the memo closure via
    // RefCell so it survives across re-runs.
    use std::cell::RefCell;
    let state: Rc<RefCell<(T, u32, T)>> = {
        // Read the initial value — this happens at Memo creation time, so we
        // borrow sig without going through the reactive graph here.
        let initial = sig.get();
        Rc::new(RefCell::new((initial.clone(), 0, initial)))
    };
    let frame_sig = use_frame_counter();
    create_memo(move || {
        let _ = frame_sig.get(); // subscribe to frame ticks
        let current = sig.get();
        let mut s = state.borrow_mut();
        if current == s.0 {
            // Same as last observed — increment stability counter.
            s.1 = s.1.saturating_add(1);
        } else {
            // Value changed — reset stability counter.
            s.0 = current;
            s.1 = 0;
        }
        if s.1 >= frames {
            // Stable long enough — commit.
            s.2 = s.0.clone();
        }
        s.2.clone()
    })
}

/// Returns a [`Memo<T>`] that propagates at most the **first** change from
/// `sig` within each window of `frames` ticks, ignoring further changes until
/// the next window begins.
///
/// This is a frame-aligned approximation of time-based throttling. At 60 fps,
/// `frames = 6` allows at most ~10 updates per second.
///
/// ```no_run
/// use raxon::runtime::throttle;
/// use raxon::reactive::create_signal;
///
/// let scroll = create_signal(0.0f32);
/// let throttled = throttle(scroll, 6); // at most 10 fps of propagation
/// ```
pub fn throttle<T: Clone + PartialEq + 'static>(sig: Signal<T>, frames: u32) -> Memo<T> {
    // State: (last committed frame, committed value).
    use std::cell::RefCell;
    let initial = sig.get();
    let state: Rc<RefCell<(u64, T)>> = Rc::new(RefCell::new((0, initial)));
    let frame_sig = use_frame_counter();
    create_memo(move || {
        let _ = frame_sig.get(); // subscribe to frame ticks
        let now = current_frame();
        let current = sig.get();
        let mut s = state.borrow_mut();
        if now.saturating_sub(s.0) >= u64::from(frames) {
            // Window elapsed — propagate.
            s.0 = now;
            s.1 = current;
        }
        s.1.clone()
    })
}

impl Drop for App {
    fn drop(&mut self) {
        if let Some(scope) = self.scope.take() {
            scope.dispose();
        }
    }
}
