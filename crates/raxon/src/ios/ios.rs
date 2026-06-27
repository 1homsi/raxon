//! The iOS implementation (compiled only for `target_os = "ios"`).

// objc2 marks many UIKit accessors safe, but their safety is version-dependent;
// we keep FFI calls inside `unsafe` blocks to document intent and stay robust
// across objc2 upgrades.
#![allow(unused_unsafe)]
// TODO: migrate the window/screen bootstrap to UIWindowScene (scene manifest).
// The deprecated path works on current simulators and keeps the demo simple.
#![allow(deprecated)]

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::ffi::{c_char, c_void};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{class, define_class, msg_send, sel, ClassType, MainThreadMarker, MainThreadOnly};
use objc2_core_foundation::{CGAffineTransform, CGPoint, CGRect, CGSize};
use objc2_foundation::{
    NSData, NSDate, NSMutableArray, NSNotification, NSNotificationCenter, NSRange, NSString,
};
use objc2_quartz_core::{CADisplayLink, CAGradientLayer, CALayer, CAShapeLayer, CATextLayer};
use objc2_ui_kit::{
    NSTextAlignment, UIAccessibilityCustomAction, UIActivityIndicatorView, UIApplication,
    UIApplicationDelegate, UIBlurEffect, UIBlurEffectStyle, UIButton, UIButtonType, UIColor,
    UIControl, UIControlEvents, UIControlState, UIDatePicker, UIDatePickerMode, UIDatePickerStyle,
    UIEdgeInsets, UIFont, UIGestureRecognizer, UIGestureRecognizerState, UIImage, UIImageView,
    UILabel, UILongPressGestureRecognizer, UIPanGestureRecognizer, UIPinchGestureRecognizer,
    UIProgressView, UIRotationGestureRecognizer, UIScreen, UIScrollView, UIScrollViewDelegate,
    UISegmentedControl, UISlider, UIStepper, UISwitch, UITapGestureRecognizer, UITextBorderStyle,
    UITextField, UITextFieldViewMode, UITextInputTraits, UITextView, UITraitEnvironment,
    UIUserInterfaceStyle, UIView, UIViewController, UIVisualEffectView, UIWindow,
};

use block2::RcBlock;

use crate::core::{Color, ColorScheme, EdgeInsets, Point, Rect, Size};
use crate::dom::{
    Attribute, Backend, Callback, DrawCmd, Event, EventSink, GestureKind, GesturePhase,
    HapticStyle, Host, ImageErrorCallback, ImageLoadCallback, KeyboardType, LayoutDirection,
    Lifecycle, MenuItem, Mutation, NetworkStatus, PermissionKind, PermissionStatus, ScrollCallback,
    ScrollInfo, SwipeDirection, TextDecoration, TextSelection, WidgetId, WidgetKind,
};
// TextStyle is referenced as crate::dom::TextStyle in the match arms.
use crate::runtime::App;
use crate::view::View;

// ---------------------------------------------------------------------------
// Per-thread state. Everything here lives on the main thread.
// ---------------------------------------------------------------------------

type ViewFactory = Box<dyn FnOnce(Host, Size) -> App>;

static PENDING_PERMISSION_RESULTS: Mutex<Vec<(PermissionKind, PermissionStatus)>> =
    Mutex::new(Vec::new());
static PENDING_MEDIA_RESULTS: Mutex<Vec<Vec<Arc<Vec<u8>>>>> = Mutex::new(Vec::new());
static PENDING_MEDIA_CANCELS: Mutex<usize> = Mutex::new(0);

const SC_NETWORK_REACHABILITY_FLAGS_REACHABLE: u32 = 1 << 1;
const SC_NETWORK_REACHABILITY_FLAGS_CONNECTION_REQUIRED: u32 = 1 << 2;
const SC_NETWORK_REACHABILITY_FLAGS_CONNECTION_ON_TRAFFIC: u32 = 1 << 3;
const SC_NETWORK_REACHABILITY_FLAGS_INTERVENTION_REQUIRED: u32 = 1 << 4;
const SC_NETWORK_REACHABILITY_FLAGS_CONNECTION_ON_DEMAND: u32 = 1 << 5;
const SC_NETWORK_REACHABILITY_FLAGS_IS_WWAN: u32 = 1 << 18;

#[link(name = "SystemConfiguration", kind = "framework")]
extern "C" {
    fn SCNetworkReachabilityCreateWithName(
        allocator: *const c_void,
        nodename: *const c_char,
    ) -> *const c_void;
    fn SCNetworkReachabilityGetFlags(target: *const c_void, flags: *mut u32) -> u8;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: *const c_void);
}

struct IosState {
    app: Rc<RefCell<App>>,
    event_sink: EventSink,
    // Keep platform objects alive for the lifetime of the app.
    _window: Retained<UIWindow>,
    _view_controller: Retained<UIViewController>,
    _display_link: Retained<CADisplayLink>,
    _ticker: Retained<Ticker>,
}

thread_local! {
    static FACTORY: RefCell<Option<ViewFactory>> = const { RefCell::new(None) };
    static STATE: RefCell<Option<IosState>> = const { RefCell::new(None) };
    // Keyboard height pending application. Set from the keyboard notifications
    // (which can fire *synchronously while the app is mid-tick*), applied by the
    // next frame tick — never borrows the app, avoiding re-entrant borrows.
    static PENDING_KEYBOARD: Cell<Option<f32>> = const { Cell::new(None) };
    // QR detections queued from the AVCaptureMetadataOutput delegate callback.
    // The delegate fires on the main queue; we drain these in handle_tick so we
    // never borrow the app reentrantly from inside a capture callback.
    static PENDING_QR: RefCell<Vec<(u64, String)>> = const { RefCell::new(Vec::new()) };
    // Deep link URLs queued by application:openURL:options:. Drained in handle_tick.
    static PENDING_DEEP_LINKS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    // App lifecycle transitions queued by UIApplicationDelegate callbacks.
    static PENDING_APP_LIFECYCLE: RefCell<Vec<Lifecycle>> = const { RefCell::new(Vec::new()) };
    // Biometric authentication results queued from the reply block (arbitrary thread).
    // Drained in handle_tick and dispatched as Event::BiometricResult.
    static PENDING_BIOMETRIC: RefCell<Vec<(bool, Option<String>)>> = const { RefCell::new(Vec::new()) };
    // GPS location fixes queued from the CLLocationManager delegate.
    static PENDING_LOCATIONS: RefCell<Vec<(f64, f64, f64)>> = const { RefCell::new(Vec::new()) };
    // Set to true if location permission was denied.
    static PENDING_LOCATION_DENIED: Cell<bool> = const { Cell::new(false) };
    // The CLLocationManager instance (raw pointer, retained manually).
    static LOCATION_MANAGER: RefCell<Option<*mut AnyObject>> = const { RefCell::new(None) };
    // CLLocationManager.delegate is weak, so keep the delegate alive while
    // location updates are running.
    static LOCATION_DELEGATE: RefCell<Option<Retained<LocationDelegate>>> = const { RefCell::new(None) };
    // One-shot manager used for permission prompts that should not start GPS.
    static PERMISSION_LOCATION_MANAGER: RefCell<Option<*mut AnyObject>> = const { RefCell::new(None) };
    static PERMISSION_LOCATION_DELEGATE: RefCell<Option<Retained<LocationDelegate>>> = const { RefCell::new(None) };
    // Motion data polled each tick from CMMotionManager. Each tuple:
    // (accel_x, accel_y, accel_z, gyro_x, gyro_y, gyro_z)
    static PENDING_MOTION: RefCell<Vec<(Option<f64>, Option<f64>, Option<f64>, Option<f64>, Option<f64>, Option<f64>)>> =
        const { RefCell::new(vec![]) };
    // The CMMotionManager instance (raw pointer, retained manually).
    static MOTION_MANAGER: RefCell<Option<*mut AnyObject>> = const { RefCell::new(None) };
    // Set of view tags (WidgetId::to_u64()) that have AnimateLayout enabled.
    static ANIMATED_LAYOUT_VIEWS: RefCell<HashSet<u64>> = RefCell::new(HashSet::new());
    // Media picker results queued from the PHPickerViewControllerDelegate callback.
    // Each entry is a Vec of image byte payloads (may be empty for first-version stub).
    static PENDING_MEDIA: RefCell<Vec<Vec<std::sync::Arc<Vec<u8>>>>> = const { RefCell::new(Vec::new()) };
    // Set to true if the user cancelled the media picker.
    static PENDING_MEDIA_CANCEL: Cell<bool> = const { Cell::new(false) };
    // Keep the MediaPickerDelegate alive while the picker is visible.
    static MEDIA_PICKER_DELEGATE: RefCell<Option<Retained<MediaPickerDelegate>>> = const { RefCell::new(None) };
    // Document picker results queued from the UIDocumentPickerDelegate callback.
    // Each entry is one pick session's files as (filename, bytes).
    static PENDING_DOCUMENTS: RefCell<Vec<Vec<(String, Vec<u8>)>>> = const { RefCell::new(Vec::new()) };
    // Keep the DocumentPickerDelegate alive while the picker is visible.
    static DOCUMENT_PICKER_DELEGATE: RefCell<Option<Retained<DocumentPickerDelegate>>> = const { RefCell::new(None) };
    // Frame timestamps for FPS calculation (ring buffer of the last 1 second).
    static FRAME_TIMESTAMPS: RefCell<std::collections::VecDeque<std::time::Instant>> =
        RefCell::new(std::collections::VecDeque::new());
    // Tick counter used to throttle UIDevice battery polling (once per ~60 ticks).
    static BATTERY_TICK: Cell<u64> = const { Cell::new(0) };
    // Tick counter + last value used to throttle native reachability polling.
    static NETWORK_TICK: Cell<u64> = const { Cell::new(0) };
    static LAST_NETWORK_STATUS: Cell<Option<NetworkStatus>> = const { Cell::new(None) };
    // UIScrollView callbacks keyed by WidgetId::to_u64(). The Objective-C
    // delegate remains ivar-free and looks handlers up by the view's tag.
    static SCROLL_HANDLERS: RefCell<HashMap<u64, ScrollHandlers>> = RefCell::new(HashMap::new());
    // Text input constraints keyed by WidgetId::to_u64(), read by the shared
    // UITextField/UITextView delegate before UIKit applies an edit.
    static TEXT_MAX_LENGTHS: RefCell<HashMap<u64, usize>> = RefCell::new(HashMap::new());
    // Press/swipe callbacks keyed by WidgetId::to_u64(). Recognizers recover
    // the widget from the view tag and dispatch these Rust callbacks.
    static PRESS_HANDLERS: RefCell<HashMap<u64, PressHandlers>> = RefCell::new(HashMap::new());
    static SWIPE_HANDLERS: RefCell<HashMap<(u64, u8), Callback>> = RefCell::new(HashMap::new());
    // Image lifecycle callbacks keyed by WidgetId::to_u64(). The latest
    // native load result is kept so callbacks installed after the source/data
    // mutation still observe the current image.
    static IMAGE_HANDLERS: RefCell<HashMap<u64, ImageHandlers>> = RefCell::new(HashMap::new());
    // UIAccessibilityCustomAction object pointers mapped to their owning
    // widget/action payload. The backend retains the action objects while this
    // map lets the selector stay ivar-free.
    static ACCESSIBILITY_ACTION_PAYLOADS: RefCell<HashMap<usize, (u64, String)>> =
        RefCell::new(HashMap::new());
}

/// Send a selector that takes a single `NSInteger` argument via raw
/// `objc_msgSend`, bypassing objc2's debug method-verification.
///
/// `UITextInputTraits` setters (`setKeyboardType:` / `setReturnKeyType:`) are
/// *forwarded* by `UITextField`/`UITextView` rather than implemented directly,
/// so `class_getInstanceMethod` returns NULL and objc2's verified `msg_send!`
/// aborts with "method not found" — even though the real send succeeds through
/// the ObjC forwarding machinery. This calls the runtime directly.
unsafe fn send_set_int(obj: *const AnyObject, sel: objc2::runtime::Sel, value: isize) {
    extern "C" {
        fn objc_msgSend();
    }
    let f: unsafe extern "C" fn(*const AnyObject, objc2::runtime::Sel, isize) =
        std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
    f(obj, sel, value);
}

fn handle_tap(tag_bits: u64) {
    // Enqueue only. The CADisplayLink tick drains and rebuilds on the next
    // frame — never synchronously inside this UIKit action, so a view (e.g. the
    // tapped button) is never torn down while its action is still on the stack.
    STATE.with(|s| {
        if let Some(state) = s.borrow().as_ref() {
            state.event_sink.dispatch(Event::Tap {
                target: WidgetId::from_u64(tag_bits),
            });
        }
    });
}

fn poll_ios_network_status_if_due() -> Option<NetworkStatus> {
    NETWORK_TICK.with(|tick| {
        let current = tick.get();
        tick.set(current.wrapping_add(1));
        if current % 60 != 0 {
            return None;
        }

        let status = query_ios_network_status();
        LAST_NETWORK_STATUS.with(|last| {
            if last.get() == Some(status) {
                None
            } else {
                last.set(Some(status));
                Some(status)
            }
        })
    })
}

fn query_ios_network_status() -> NetworkStatus {
    unsafe {
        let target = SCNetworkReachabilityCreateWithName(
            std::ptr::null(),
            b"apple.com\0".as_ptr().cast::<c_char>(),
        );
        if target.is_null() {
            return NetworkStatus::Unknown;
        }

        let mut flags = 0_u32;
        let ok = SCNetworkReachabilityGetFlags(target, &mut flags) != 0;
        CFRelease(target);
        if !ok {
            return NetworkStatus::Unknown;
        }

        ios_network_status_from_flags(flags)
    }
}

fn ios_network_status_from_flags(flags: u32) -> NetworkStatus {
    let reachable = flags & SC_NETWORK_REACHABILITY_FLAGS_REACHABLE != 0;
    let connection_required = flags & SC_NETWORK_REACHABILITY_FLAGS_CONNECTION_REQUIRED != 0;
    let can_connect_automatically = flags
        & (SC_NETWORK_REACHABILITY_FLAGS_CONNECTION_ON_DEMAND
            | SC_NETWORK_REACHABILITY_FLAGS_CONNECTION_ON_TRAFFIC)
        != 0;
    let intervention_required = flags & SC_NETWORK_REACHABILITY_FLAGS_INTERVENTION_REQUIRED != 0;

    let online = reachable
        && (!connection_required || (can_connect_automatically && !intervention_required));
    if !online {
        return NetworkStatus::Offline;
    }

    if flags & SC_NETWORK_REACHABILITY_FLAGS_IS_WWAN != 0 {
        NetworkStatus::Cellular
    } else {
        NetworkStatus::WiFi
    }
}

fn handle_tick() {
    // Drain any QR detections collected since the last tick. We pull them out
    // *before* borrowing the app so we never hold two borrows simultaneously.
    let qr_events: Vec<(u64, String)> = PENDING_QR.with(|q| {
        let mut v = q.borrow_mut();
        std::mem::take(&mut *v)
    });

    // Drain any deep link URLs queued by application:openURL:options:.
    let deep_links: Vec<String> = PENDING_DEEP_LINKS.with(|q| {
        let mut v = q.borrow_mut();
        std::mem::take(&mut *v)
    });

    // Drain app lifecycle transitions queued by UIApplicationDelegate callbacks.
    let lifecycle_events: Vec<Lifecycle> = PENDING_APP_LIFECYCLE.with(|q| {
        let mut v = q.borrow_mut();
        std::mem::take(&mut *v)
    });

    // Drain any biometric results queued by the LAContext reply block.
    let biometric_results: Vec<(bool, Option<String>)> = PENDING_BIOMETRIC.with(|q| {
        let mut v = q.borrow_mut();
        std::mem::take(&mut *v)
    });

    // Drain GPS fixes queued by LocationDelegate.
    let location_fixes: Vec<(f64, f64, f64)> = PENDING_LOCATIONS.with(|q| {
        let mut v = q.borrow_mut();
        std::mem::take(&mut *v)
    });
    let location_denied = PENDING_LOCATION_DENIED.with(|c| c.replace(false));

    let permission_results: Vec<(PermissionKind, PermissionStatus)> = PENDING_PERMISSION_RESULTS
        .lock()
        .map(|mut v| std::mem::take(&mut *v))
        .unwrap_or_default();

    // Poll CMMotionManager for current accelerometer and gyroscope readings.
    // CMAcceleration / CMRotationRate are both {f64, f64, f64} structs that
    // objc2's msg_send! needs to receive by value. We implement Encode for our
    // local wrappers using the same compound encoding CGPoint uses.
    #[repr(C)]
    struct Motion3 {
        x: f64,
        y: f64,
        z: f64,
    }
    unsafe impl objc2::Encode for Motion3 {
        const ENCODING: objc2::encode::Encoding = objc2::encode::Encoding::Struct(
            "CMAcceleration",
            &[f64::ENCODING, f64::ENCODING, f64::ENCODING],
        );
    }

    MOTION_MANAGER.with(|m| {
        if let Some(mgr) = *m.borrow() {
            let mut accel: (Option<f64>, Option<f64>, Option<f64>) = (None, None, None);
            let mut gyro: (Option<f64>, Option<f64>, Option<f64>) = (None, None, None);

            unsafe {
                // Get accelerometer data
                let accel_data: *mut AnyObject = msg_send![mgr, accelerometerData];
                if !accel_data.is_null() {
                    let acc: Motion3 = msg_send![accel_data, acceleration];
                    accel = (Some(acc.x), Some(acc.y), Some(acc.z));
                }

                // Get gyroscope data
                let gyro_data: *mut AnyObject = msg_send![mgr, gyroData];
                if !gyro_data.is_null() {
                    let rate: Motion3 = msg_send![gyro_data, rotationRate];
                    gyro = (Some(rate.x), Some(rate.y), Some(rate.z));
                }
            }

            if accel.0.is_some() || gyro.0.is_some() {
                PENDING_MOTION.with(|q| {
                    q.borrow_mut()
                        .push((accel.0, accel.1, accel.2, gyro.0, gyro.1, gyro.2));
                });
            }
        }
    });

    // Drain motion events queued above.
    let motion_events: Vec<(
        Option<f64>,
        Option<f64>,
        Option<f64>,
        Option<f64>,
        Option<f64>,
        Option<f64>,
    )> = PENDING_MOTION.with(|q| {
        let mut v = q.borrow_mut();
        std::mem::take(&mut *v)
    });

    // Drain media picker results and cancellation flag.
    let mut media_results: Vec<Vec<Arc<Vec<u8>>>> = PENDING_MEDIA.with(|q| {
        let mut v = q.borrow_mut();
        std::mem::take(&mut *v)
    });
    let async_media_results: Vec<Vec<Arc<Vec<u8>>>> = PENDING_MEDIA_RESULTS
        .lock()
        .map(|mut v| std::mem::take(&mut *v))
        .unwrap_or_default();
    media_results.extend(async_media_results);
    let media_cancelled = PENDING_MEDIA_CANCEL.with(|c| c.replace(false))
        || PENDING_MEDIA_CANCELS
            .lock()
            .map(|mut count| {
                let cancelled = *count > 0;
                *count = 0;
                cancelled
            })
            .unwrap_or(false);

    // Drain document picker results.
    let document_results: Vec<Vec<(String, Vec<u8>)>> = PENDING_DOCUMENTS.with(|q| {
        let mut v = q.borrow_mut();
        std::mem::take(&mut *v)
    });
    let network_status = poll_ios_network_status_if_due();

    STATE.with(|s| {
        if let Some(state) = s.borrow().as_ref() {
            // Feed the platform safe-area insets to the runtime each frame; it
            // only re-lays-out when they actually change.
            let insets = state._window.safeAreaInsets();
            // Report the system appearance so dark/light-aware backdrops and
            // content can adapt.
            let scheme = if unsafe { state._window.traitCollection().userInterfaceStyle() }
                == UIUserInterfaceStyle::Dark
            {
                ColorScheme::Dark
            } else {
                ColorScheme::Light
            };
            let mut app = state.app.borrow_mut();
            app.set_color_scheme(scheme);
            // Detect high-contrast / darker system colors (UIAccessibilityIsDarkerSystemColorsEnabled).
            let high_contrast = unsafe {
                extern "C" {
                    fn UIAccessibilityDarkerSystemColorsEnabled() -> bool;
                }
                UIAccessibilityDarkerSystemColorsEnabled()
            };
            app.set_high_contrast(high_contrast);
            app.set_safe_area(EdgeInsets {
                top: insets.top as f32,
                right: insets.right as f32,
                bottom: insets.bottom as f32,
                left: insets.left as f32,
            });
            if let Some(height) = PENDING_KEYBOARD.with(|k| k.take()) {
                app.set_keyboard_inset(height);
            }
            // Dispatch queued QR detections into the event system.
            for (tag, value) in qr_events {
                state.event_sink.dispatch(Event::QrDetected {
                    target: WidgetId::from_u64(tag),
                    value,
                });
            }
            // Dispatch queued deep link URLs into the event system.
            for url in deep_links {
                state.event_sink.dispatch(Event::DeepLink { url });
            }
            // Dispatch queued app lifecycle transitions.
            for lifecycle in lifecycle_events {
                state.event_sink.dispatch(Event::AppLifecycle(lifecycle));
            }
            // Dispatch queued biometric results into the event system.
            for (success, error) in biometric_results {
                state
                    .event_sink
                    .dispatch(Event::BiometricResult { success, error });
            }
            // Dispatch queued permission status results.
            for (permission, status) in permission_results {
                crate::runtime::update_permission(permission, status);
                state
                    .event_sink
                    .dispatch(Event::PermissionChanged { permission, status });
            }
            // Dispatch queued GPS location fixes.
            for (latitude, longitude, accuracy) in location_fixes {
                state.event_sink.dispatch(Event::LocationUpdated {
                    latitude,
                    longitude,
                    accuracy,
                });
            }
            if location_denied {
                state.event_sink.dispatch(Event::LocationDenied);
            }
            if let Some(status) = network_status {
                state
                    .event_sink
                    .dispatch(Event::NetworkStatusChanged { status });
            }
            // Dispatch queued motion sensor readings.
            for (accel_x, accel_y, accel_z, gyro_x, gyro_y, gyro_z) in motion_events {
                state.event_sink.dispatch(Event::MotionUpdated {
                    accel_x,
                    accel_y,
                    accel_z,
                    gyro_x,
                    gyro_y,
                    gyro_z,
                });
            }
            // Dispatch media picker results.
            for images in media_results {
                state.event_sink.dispatch(Event::MediaPicked { images });
            }
            if media_cancelled {
                state.event_sink.dispatch(Event::MediaPickerCancelled);
            }
            // Dispatch document picker results.
            for files in document_results {
                state.event_sink.dispatch(Event::DocumentPicked { files });
            }
            app.tick();
            crate::plugin::tick_plugins();

            // Poll UIDevice battery state roughly once per second (~60 ticks).
            // Battery monitoring must be enabled before reading level/state.
            BATTERY_TICK.with(|c| {
                let tick = c.get();
                c.set(tick.wrapping_add(1));
                if tick % 60 == 0 {
                    unsafe {
                        let device: *mut AnyObject = msg_send![class!(UIDevice), currentDevice];
                        let _: () = msg_send![device, setBatteryMonitoringEnabled: true];
                        // batteryLevel returns -1.0 when monitoring is not enabled
                        // or the level is unknown; clamp to 0.0 in that case.
                        let level: f32 = msg_send![device, batteryLevel];
                        // batteryState: 0=unknown, 1=unplugged, 2=charging, 3=full
                        let state: isize = msg_send![device, batteryState];
                        let charging = state == 2 || state == 3;
                        crate::runtime::update_battery(level.max(0.0), charging);
                    }
                }
            });

            // Update FPS counter: keep a sliding window of timestamps for the
            // last 1 second and push the count into the reactive FPS signal.
            let fps = FRAME_TIMESTAMPS.with(|ts| {
                let now = std::time::Instant::now();
                let mut deq = ts.borrow_mut();
                deq.push_back(now);
                // Evict timestamps older than 1 second.
                while deq
                    .front()
                    .map(|t| now.duration_since(*t) > std::time::Duration::from_secs(1))
                    .unwrap_or(false)
                {
                    deq.pop_front();
                }
                deq.len() as f32
            });
            crate::view::update_fps(fps);
        }
    });
}

fn handle_keyboard(height: f32) {
    // Record only; the frame tick applies it. This callback can fire
    // synchronously while the app is already borrowed (removing a focused text
    // field resigns first responder, which posts the hide notification), so it
    // must never borrow the app itself.
    PENDING_KEYBOARD.with(|k| k.set(Some(height)));
}

fn queue_app_lifecycle(lifecycle: Lifecycle) {
    PENDING_APP_LIFECYCLE.with(|q| q.borrow_mut().push(lifecycle));
}

fn handle_value_changed(tag_bits: u64, value: f64) {
    STATE.with(|s| {
        if let Some(state) = s.borrow().as_ref() {
            state.event_sink.dispatch(Event::ValueChanged {
                target: WidgetId::from_u64(tag_bits),
                value,
            });
        }
    });
}

fn dispatch_target_event(make: impl FnOnce(WidgetId) -> Event, tag_bits: u64) {
    // Enqueue only; the frame tick drains it (see `handle_tap`).
    STATE.with(|s| {
        if let Some(state) = s.borrow().as_ref() {
            state
                .event_sink
                .dispatch(make(WidgetId::from_u64(tag_bits)));
        }
    });
}

fn recognizer_tag(recognizer: &UIGestureRecognizer) -> Option<u64> {
    unsafe { recognizer.view() }.map(|v| unsafe { v.tag() } as u64)
}

fn handle_text_changed(tag_bits: u64, value: String) {
    STATE.with(|s| {
        if let Some(state) = s.borrow().as_ref() {
            let selection = TextSelection::caret(value.len());
            state.event_sink.dispatch(Event::TextChanged {
                target: WidgetId::from_u64(tag_bits),
                value,
                selection,
            });
        }
    });
}

fn set_text_max_length(tag: u64, max_len: usize) {
    TEXT_MAX_LENGTHS.with(|limits| {
        limits.borrow_mut().insert(tag, max_len);
    });
}

fn clear_text_input_state(tag: u64) {
    TEXT_MAX_LENGTHS.with(|limits| {
        limits.borrow_mut().remove(&tag);
    });
}

#[derive(Clone)]
enum ImageLoadResult {
    Loaded,
    Error(String),
}

#[derive(Default)]
struct ImageHandlers {
    on_load: Option<ImageLoadCallback>,
    on_error: Option<ImageErrorCallback>,
    last_result: Option<ImageLoadResult>,
}

fn set_image_on_load(tag: u64, callback: ImageLoadCallback) {
    let fire_now = IMAGE_HANDLERS.with(|handlers| {
        let mut handlers = handlers.borrow_mut();
        let state = handlers.entry(tag).or_default();
        let fire_now = matches!(state.last_result, Some(ImageLoadResult::Loaded));
        state.on_load = Some(callback.clone());
        fire_now
    });

    if fire_now {
        callback.call();
    }
}

fn set_image_on_error(tag: u64, callback: ImageErrorCallback) {
    let last_error = IMAGE_HANDLERS.with(|handlers| {
        let mut handlers = handlers.borrow_mut();
        let state = handlers.entry(tag).or_default();
        let last_error = match &state.last_result {
            Some(ImageLoadResult::Error(error)) => Some(error.clone()),
            _ => None,
        };
        state.on_error = Some(callback.clone());
        last_error
    });

    if let Some(error) = last_error {
        callback.call(error);
    }
}

fn mark_image_loaded(tag: u64) {
    let callback = IMAGE_HANDLERS.with(|handlers| {
        let mut handlers = handlers.borrow_mut();
        let state = handlers.entry(tag).or_default();
        state.last_result = Some(ImageLoadResult::Loaded);
        state.on_load.clone()
    });

    if let Some(callback) = callback {
        callback.call();
    }
}

fn mark_image_error(tag: u64, error: impl Into<String>) {
    let error = error.into();
    let callback = IMAGE_HANDLERS.with(|handlers| {
        let mut handlers = handlers.borrow_mut();
        let state = handlers.entry(tag).or_default();
        state.last_result = Some(ImageLoadResult::Error(error.clone()));
        state.on_error.clone()
    });

    if let Some(callback) = callback {
        callback.call(error);
    }
}

fn clear_image_handlers(tag: u64) {
    IMAGE_HANDLERS.with(|handlers| {
        handlers.borrow_mut().remove(&tag);
    });
}

fn action_key(action: &UIAccessibilityCustomAction) -> usize {
    action as *const UIAccessibilityCustomAction as usize
}

fn register_accessibility_action_payload(
    action: &UIAccessibilityCustomAction,
    tag: u64,
    name: String,
) {
    ACCESSIBILITY_ACTION_PAYLOADS.with(|payloads| {
        payloads
            .borrow_mut()
            .insert(action_key(action), (tag, name));
    });
}

fn clear_accessibility_action_payloads(tag: u64) {
    ACCESSIBILITY_ACTION_PAYLOADS.with(|payloads| {
        payloads
            .borrow_mut()
            .retain(|_, (payload_tag, _)| *payload_tag != tag);
    });
}

fn handle_accessibility_custom_action(action: &UIAccessibilityCustomAction) -> bool {
    let payload = ACCESSIBILITY_ACTION_PAYLOADS
        .with(|payloads| payloads.borrow().get(&action_key(action)).cloned());
    let Some((tag, action)) = payload else {
        return false;
    };

    STATE.with(|s| {
        if let Some(state) = s.borrow().as_ref() {
            state.event_sink.dispatch(Event::AccessibilityAction {
                target: WidgetId::from_u64(tag),
                action,
            });
        }
    });
    true
}

fn should_allow_text_edit(
    tag: u64,
    current: Option<&NSString>,
    range: NSRange,
    replacement: &NSString,
) -> bool {
    let Some(max_len) = TEXT_MAX_LENGTHS.with(|limits| limits.borrow().get(&tag).copied()) else {
        return true;
    };

    let current_len = current.map(|text| text.length()).unwrap_or_default();
    let removed_len = if range.location <= current_len {
        range.length.min(current_len - range.location)
    } else {
        0
    };
    let next_len = current_len
        .saturating_sub(removed_len)
        .saturating_add(replacement.length());

    next_len <= max_len
}

#[derive(Default)]
struct PressHandlers {
    on_press_in: Option<Callback>,
    on_press_out: Option<Callback>,
    pressed: bool,
}

fn update_press_handlers(tag: u64, update: impl FnOnce(&mut PressHandlers)) {
    PRESS_HANDLERS.with(|handlers| {
        let mut handlers = handlers.borrow_mut();
        update(handlers.entry(tag).or_default());
    });
}

fn set_swipe_handler(tag: u64, direction: SwipeDirection, callback: Callback) {
    SWIPE_HANDLERS.with(|handlers| {
        handlers
            .borrow_mut()
            .insert((tag, swipe_direction_bits(direction)), callback);
    });
}

fn clear_interaction_handlers(tag: u64) {
    PRESS_HANDLERS.with(|handlers| {
        handlers.borrow_mut().remove(&tag);
    });
    SWIPE_HANDLERS.with(|handlers| {
        handlers
            .borrow_mut()
            .retain(|(handler_tag, _), _| *handler_tag != tag);
    });
}

fn swipe_direction_bits(direction: SwipeDirection) -> u8 {
    match direction {
        SwipeDirection::Right => 1,
        SwipeDirection::Left => 2,
        SwipeDirection::Up => 4,
        SwipeDirection::Down => 8,
    }
}

fn handle_press_recognized(recognizer: &UILongPressGestureRecognizer) {
    let Some(tag) = recognizer_tag(recognizer) else {
        return;
    };
    let state = unsafe { recognizer.state() };
    let callback = PRESS_HANDLERS.with(|handlers| {
        let mut handlers = handlers.borrow_mut();
        let handlers = handlers.get_mut(&tag)?;
        if state == UIGestureRecognizerState::Began {
            handlers.pressed = true;
            handlers.on_press_in.clone()
        } else if state == UIGestureRecognizerState::Ended
            || state == UIGestureRecognizerState::Cancelled
            || state == UIGestureRecognizerState::Failed
        {
            if handlers.pressed {
                handlers.pressed = false;
                handlers.on_press_out.clone()
            } else {
                None
            }
        } else {
            None
        }
    });

    if let Some(callback) = callback {
        callback.call();
    }
}

fn handle_swipe_recognized(recognizer: &UIGestureRecognizer) {
    if unsafe { recognizer.state() } != UIGestureRecognizerState::Recognized {
        return;
    }
    let Some(tag) = recognizer_tag(recognizer) else {
        return;
    };
    let direction_bits = unsafe {
        let bits: usize = msg_send![recognizer, direction];
        bits as u8
    };
    let callback =
        SWIPE_HANDLERS.with(|handlers| handlers.borrow().get(&(tag, direction_bits)).cloned());

    if let Some(callback) = callback {
        callback.call();
    }
}

#[derive(Default)]
struct ScrollHandlers {
    on_scroll: Option<ScrollCallback>,
    on_begin: Option<Callback>,
    on_end: Option<Callback>,
    last_offset: Option<(f32, f32)>,
    last_instant: Option<Instant>,
}

fn update_scroll_handlers(tag: u64, update: impl FnOnce(&mut ScrollHandlers)) {
    SCROLL_HANDLERS.with(|handlers| {
        let mut handlers = handlers.borrow_mut();
        update(handlers.entry(tag).or_default());
    });
}

fn clear_scroll_handlers(tag: u64) {
    SCROLL_HANDLERS.with(|handlers| {
        handlers.borrow_mut().remove(&tag);
    });
}

fn scroll_view_tag(scroll_view: &UIScrollView) -> u64 {
    unsafe { scroll_view.tag() as u64 }
}

fn handle_scroll_did_scroll(tag: u64, offset_x: f32, offset_y: f32) {
    let now = Instant::now();
    let callback = SCROLL_HANDLERS.with(|handlers| {
        let mut handlers = handlers.borrow_mut();
        let state = handlers.get_mut(&tag)?;
        let (velocity_x, velocity_y) = match (state.last_offset, state.last_instant) {
            (Some((last_x, last_y)), Some(last_instant)) => {
                let dt = now.duration_since(last_instant).as_secs_f32();
                if dt > 0.0 {
                    ((offset_x - last_x) / dt, (offset_y - last_y) / dt)
                } else {
                    (0.0, 0.0)
                }
            }
            _ => (0.0, 0.0),
        };

        state.last_offset = Some((offset_x, offset_y));
        state.last_instant = Some(now);
        state.on_scroll.clone().map(|cb| {
            (
                cb,
                ScrollInfo {
                    offset_x,
                    offset_y,
                    velocity_x,
                    velocity_y,
                },
            )
        })
    });

    if let Some((callback, info)) = callback {
        callback.call(info);
    }
}

fn handle_scroll_begin(tag: u64) {
    let callback = SCROLL_HANDLERS.with(|handlers| {
        let mut handlers = handlers.borrow_mut();
        let state = handlers.get_mut(&tag)?;
        state.last_offset = None;
        state.last_instant = None;
        state.on_begin.clone()
    });

    if let Some(callback) = callback {
        callback.call();
    }
}

fn handle_scroll_end(tag: u64) {
    let callback = SCROLL_HANDLERS.with(|handlers| {
        let mut handlers = handlers.borrow_mut();
        let state = handlers.get_mut(&tag)?;
        state.last_offset = None;
        state.last_instant = None;
        state.on_end.clone()
    });

    if let Some(callback) = callback {
        callback.call();
    }
}

// ---------------------------------------------------------------------------
// Objective-C glue classes (no Rust ivars; they read the thread-local STATE).
// ---------------------------------------------------------------------------

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RaxActionTarget"]
    struct ActionTarget;

    unsafe impl NSObjectProtocol for ActionTarget {}

    impl ActionTarget {
        #[unsafe(method(didTapButton:))]
        fn did_tap(&self, sender: &UIButton) {
            let tag = unsafe { sender.tag() };
            handle_tap(tag as u64);
        }

        #[unsafe(method(togglePassword:))]
        fn toggle_password(&self, sender: &UIButton) {
            // The eye button is a UITextField rightView; walk up to the owning
            // field, flip secureTextEntry, and swap the eye glyph. Self-contained
            // so app code gets a password reveal toggle for free on `.secure()`.
            unsafe {
                let mut cur: Option<Retained<UIView>> = msg_send![sender, superview];
                while let Some(v) = cur {
                    if let Ok(field) = v.clone().downcast::<UITextField>() {
                        let secure: bool = msg_send![&*field, isSecureTextEntry];
                        let now = !secure;
                        let _: () = msg_send![&*field, setSecureTextEntry: now];
                        let name = if now { "eye.slash" } else { "eye" };
                        let ns = NSString::from_str(name);
                        if let Some(img) = UIImage::systemImageNamed(&ns) {
                            let _: () = msg_send![sender, setImage: &*img, forState: 0usize];
                        }
                        return;
                    }
                    cur = msg_send![&*v, superview];
                }
            }
        }

        #[unsafe(method(valueChanged:))]
        fn value_changed(&self, sender: &UIControl) {
            let tag = unsafe { sender.tag() } as u64;
            let value = if let Some(sw) = sender.downcast_ref::<UISwitch>() {
                if unsafe { sw.isOn() } {
                    1.0
                } else {
                    0.0
                }
            } else if let Some(sl) = sender.downcast_ref::<UISlider>() {
                unsafe { sl.value() as f64 }
            } else if let Some(seg) = sender.downcast_ref::<UISegmentedControl>() {
                unsafe { seg.selectedSegmentIndex() as f64 }
            } else if let Some(st) = sender.downcast_ref::<UIStepper>() {
                unsafe { st.value() }
            } else if let Some(dp) = sender.downcast_ref::<UIDatePicker>() {
                unsafe { dp.date().timeIntervalSince1970() }
            } else {
                0.0
            };
            handle_value_changed(tag, value);
        }

        #[unsafe(method(keyboardWillShow:))]
        fn keyboard_will_show(&self, note: &NSNotification) {
            // Pull the keyboard's end frame from the notification's userInfo and
            // treat its height as the obscured region (docked keyboard).
            let height: f32 = unsafe {
                let info: *mut AnyObject = msg_send![note, userInfo];
                if info.is_null() {
                    return;
                }
                let key = NSString::from_str("UIKeyboardFrameEndUserInfoKey");
                let value: *mut AnyObject = msg_send![info, objectForKey: &*key];
                if value.is_null() {
                    return;
                }
                let rect: CGRect = msg_send![value, CGRectValue];
                rect.size.height as f32
            };
            handle_keyboard(height);
        }

        #[unsafe(method(keyboardWillHide:))]
        fn keyboard_will_hide(&self, _note: &NSNotification) {
            handle_keyboard(0.0);
        }

        #[unsafe(method(textChanged:))]
        fn text_changed(&self, sender: &UITextField) {
            let tag = unsafe { sender.tag() } as u64;
            let text = unsafe { sender.text() }.map(|s| s.to_string()).unwrap_or_default();
            handle_text_changed(tag, text);
        }

        #[unsafe(method(textViewDidChange:))]
        fn text_view_did_change(&self, sender: &UITextView) {
            let tag = unsafe { sender.tag() } as u64;
            // UITextView.text() returns Option<Retained<NSString>> or Retained<NSString>
            // depending on objc2 version. Use msg_send to be safe.
            let text: String = unsafe {
                let ns: Option<Retained<objc2_foundation::NSString>> = msg_send![sender, text];
                ns.map(|s| s.to_string()).unwrap_or_default()
            };
            handle_text_changed(tag, text);
        }

        #[unsafe(method(textField:shouldChangeCharactersInRange:replacementString:))]
        fn text_field_should_change_characters(
            &self,
            sender: &UITextField,
            range: NSRange,
            replacement: &NSString,
        ) -> bool {
            let tag = unsafe { sender.tag() } as u64;
            let current = sender.text();
            should_allow_text_edit(tag, current.as_deref(), range, replacement)
        }

        #[unsafe(method(textView:shouldChangeTextInRange:replacementText:))]
        fn text_view_should_change_text(
            &self,
            sender: &UITextView,
            range: NSRange,
            replacement: &NSString,
        ) -> bool {
            let tag = unsafe { sender.tag() } as u64;
            let current = sender.text();
            should_allow_text_edit(tag, Some(&current), range, replacement)
        }

        #[unsafe(method(tapRecognized:))]
        fn tap_recognized(&self, recognizer: &UITapGestureRecognizer) {
            if let Some(tag) = recognizer_tag(recognizer) {
                dispatch_target_event(|target| Event::Tap { target }, tag);
            }
        }

        #[unsafe(method(doubleTapRecognized:))]
        fn double_tap_recognized(&self, recognizer: &UITapGestureRecognizer) {
            if let Some(tag) = recognizer_tag(recognizer) {
                dispatch_target_event(|target| Event::DoubleTap { target }, tag);
            }
        }

        #[unsafe(method(longPressRecognized:))]
        fn long_press_recognized(&self, recognizer: &UILongPressGestureRecognizer) {
            if unsafe { recognizer.state() } == UIGestureRecognizerState::Began {
                if let Some(tag) = recognizer_tag(recognizer) {
                    dispatch_target_event(|target| Event::LongPress { target }, tag);
                }
            }
        }

        #[unsafe(method(pressRecognized:))]
        fn press_recognized(&self, recognizer: &UILongPressGestureRecognizer) {
            handle_press_recognized(recognizer);
        }

        #[unsafe(method(swipeRecognized:))]
        fn swipe_recognized(&self, recognizer: &UIGestureRecognizer) {
            handle_swipe_recognized(recognizer);
        }

        #[unsafe(method(contextMenuLongPress:))]
        fn context_menu_long_press(&self, recognizer: &UILongPressGestureRecognizer) {
            if unsafe { recognizer.state() } == UIGestureRecognizerState::Began {
                if let Some(tag) = recognizer_tag(recognizer) {
                    present_context_menu(tag);
                }
            }
        }

        #[unsafe(method(handleRefresh:))]
        fn handle_refresh(&self, sender: &UIControl) {
            let tag = unsafe { sender.tag() } as u64;
            dispatch_target_event(|target| Event::Refresh { target }, tag);
        }

        #[unsafe(method(accessibilityAction:))]
        fn accessibility_action(&self, action: &UIAccessibilityCustomAction) -> bool {
            handle_accessibility_custom_action(action)
        }

        #[unsafe(method(textFieldShouldReturn:))]
        fn text_field_should_return(&self, sender: &UITextField) -> bool {
            let tag = unsafe { sender.tag() } as u64;
            dispatch_target_event(|target| Event::Submit { target }, tag);
            unsafe { sender.resignFirstResponder() };
            true
        }

        #[unsafe(method(pinchRecognized:))]
        fn pinch_recognized(&self, recognizer: &UIPinchGestureRecognizer) {
            let Some(tag) = recognizer_tag(recognizer) else {
                return;
            };
            let scale = unsafe { recognizer.scale() as f32 };
            let velocity = unsafe { recognizer.velocity() as f32 };
            let phase = match unsafe { recognizer.state() } {
                UIGestureRecognizerState::Began => GesturePhase::Began,
                UIGestureRecognizerState::Changed => GesturePhase::Changed,
                _ => GesturePhase::Ended,
            };
            dispatch_target_event(
                move |target| crate::dom::Event::PinchChanged {
                    target,
                    scale,
                    velocity,
                    phase,
                },
                tag,
            );
        }

        #[unsafe(method(rotateRecognized:))]
        fn rotate_recognized(&self, recognizer: &UIRotationGestureRecognizer) {
            let Some(tag) = recognizer_tag(recognizer) else {
                return;
            };
            let rotation = unsafe { recognizer.rotation() as f32 };
            let velocity = unsafe { recognizer.velocity() as f32 };
            let phase = match unsafe { recognizer.state() } {
                UIGestureRecognizerState::Began => GesturePhase::Began,
                UIGestureRecognizerState::Changed => GesturePhase::Changed,
                _ => GesturePhase::Ended,
            };
            dispatch_target_event(
                move |target| crate::dom::Event::RotateChanged {
                    target,
                    rotation,
                    velocity,
                    phase,
                },
                tag,
            );
        }

        #[unsafe(method(panRecognized:))]
        fn pan_recognized(&self, recognizer: &UIPanGestureRecognizer) {
            let Some(tag) = recognizer_tag(recognizer) else {
                return;
            };
            let view = unsafe { recognizer.view() };
            let t = unsafe { recognizer.translationInView(view.as_deref()) };
            let v = unsafe { recognizer.velocityInView(view.as_deref()) };
            let phase = match unsafe { recognizer.state() } {
                UIGestureRecognizerState::Began => GesturePhase::Began,
                UIGestureRecognizerState::Changed => GesturePhase::Changed,
                _ => GesturePhase::Ended,
            };
            let translation = Point::new(t.x as f32, t.y as f32);
            let velocity = Point::new(v.x as f32, v.y as f32);
            dispatch_target_event(
                move |target| Event::PanChanged {
                    target,
                    translation,
                    velocity,
                    phase,
                },
                tag,
            );
        }

        /// UIGestureRecognizerDelegate: allow all recognizers to fire simultaneously.
        /// This enables pan + pinch + rotate on the same view at the same time.
        #[unsafe(method(gestureRecognizer:shouldRecognizeSimultaneouslyWithGestureRecognizer:))]
        fn should_recognize_simultaneously(
            &self,
            _gesture: &UIGestureRecognizer,
            _other: &UIGestureRecognizer,
        ) -> bool {
            true
        }
    }
);

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RaxScrollDelegate"]
    struct ScrollDelegate;

    unsafe impl NSObjectProtocol for ScrollDelegate {}

    #[allow(non_snake_case)]
    unsafe impl UIScrollViewDelegate for ScrollDelegate {
        #[unsafe(method(scrollViewDidScroll:))]
        fn scrollViewDidScroll(&self, scroll_view: &UIScrollView) {
            let tag = scroll_view_tag(scroll_view);
            let offset = scroll_view.contentOffset();
            handle_scroll_did_scroll(tag, offset.x as f32, offset.y as f32);
        }

        #[unsafe(method(scrollViewWillBeginDragging:))]
        fn scrollViewWillBeginDragging(&self, scroll_view: &UIScrollView) {
            handle_scroll_begin(scroll_view_tag(scroll_view));
        }

        #[unsafe(method(scrollViewDidEndDragging:willDecelerate:))]
        fn scrollViewDidEndDragging_willDecelerate(
            &self,
            scroll_view: &UIScrollView,
            decelerate: bool,
        ) {
            if !decelerate {
                handle_scroll_end(scroll_view_tag(scroll_view));
            }
        }

        #[unsafe(method(scrollViewDidEndDecelerating:))]
        fn scrollViewDidEndDecelerating(&self, scroll_view: &UIScrollView) {
            handle_scroll_end(scroll_view_tag(scroll_view));
        }

        #[unsafe(method(scrollViewDidEndScrollingAnimation:))]
        fn scrollViewDidEndScrollingAnimation(&self, scroll_view: &UIScrollView) {
            handle_scroll_end(scroll_view_tag(scroll_view));
        }
    }
);

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RaxTicker"]
    struct Ticker;

    unsafe impl NSObjectProtocol for Ticker {}

    impl Ticker {
        #[unsafe(method(tick:))]
        fn tick(&self, _link: &CADisplayLink) {
            handle_tick();
        }
    }
);

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RaxAppDelegate"]
    struct AppDelegate;

    unsafe impl NSObjectProtocol for AppDelegate {}

    unsafe impl UIApplicationDelegate for AppDelegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish_launching(&self, _notification: &NSNotification) {
            let mtm = MainThreadMarker::new().expect("delegate runs on the main thread");
            setup(mtm);
        }

        #[unsafe(method(applicationDidBecomeActive:))]
        fn application_did_become_active(&self, _application: &AnyObject) {
            queue_app_lifecycle(Lifecycle::Resumed);
        }

        #[unsafe(method(applicationWillResignActive:))]
        fn application_will_resign_active(&self, _application: &AnyObject) {
            queue_app_lifecycle(Lifecycle::Inactive);
        }

        #[unsafe(method(applicationDidEnterBackground:))]
        fn application_did_enter_background(&self, _application: &AnyObject) {
            queue_app_lifecycle(Lifecycle::Backgrounded);
        }

        #[unsafe(method(applicationWillTerminate:))]
        fn application_will_terminate(&self, _application: &AnyObject) {
            queue_app_lifecycle(Lifecycle::Terminating);
        }

        #[unsafe(method(application:openURL:options:))]
        fn application_open_url(
            &self,
            _application: &AnyObject,
            url: &AnyObject,
            _options: &AnyObject,
        ) -> bool {
            unsafe {
                let url_str: Retained<NSString> = msg_send![url, absoluteString];
                let url_string = url_str.to_string();
                PENDING_DEEP_LINKS.with(|q| q.borrow_mut().push(url_string));
            }
            true
        }

        #[unsafe(method(application:didRegisterForRemoteNotificationsWithDeviceToken:))]
        fn did_register_for_remote_notifications(
            &self,
            _application: &AnyObject,
            device_token: &NSData,
        ) {
            if let Some(token) = apns_device_token_hex(device_token) {
                crate::runtime::update_push_token(token);
            }
        }

        #[unsafe(method(application:didFailToRegisterForRemoteNotificationsWithError:))]
        fn did_fail_to_register_for_remote_notifications(
            &self,
            _application: &AnyObject,
            _error: &AnyObject,
        ) {
            crate::runtime::clear_push_token();
        }
    }
);

// ---------------------------------------------------------------------------
// QR scanner: active sessions, keyed by widget_tag.
// ---------------------------------------------------------------------------

// Keep sessions (and their associated delegates) alive.  We use raw pointers
// because AnyObject is not Send; everything here runs on the main thread.
struct QrEntry {
    /// The `AVCaptureSession *` (retained via raw objc retain).
    session: *mut AnyObject,
    /// The `RaxQrDelegate *` (retained via raw objc retain).
    delegate: *mut AnyObject,
}

thread_local! {
    static QR_SESSIONS: RefCell<HashMap<u64, QrEntry>> = RefCell::new(HashMap::new());
}

define_class!(
    /// Delegate that receives `AVCaptureMetadataOutput` callbacks and queues QR
    /// detections into `PENDING_QR` for drain on the next frame tick.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RaxQrDelegate"]
    struct QrDelegate;

    unsafe impl NSObjectProtocol for QrDelegate {}

    impl QrDelegate {
        /// `captureOutput:didOutputMetadataObjects:fromConnection:` — fired on the
        /// main queue by `AVCaptureMetadataOutput` for every metadata batch.
        #[unsafe(method(captureOutput:didOutputMetadataObjects:fromConnection:))]
        fn did_output_metadata(
            &self,
            _output: &AnyObject,
            objects: &NSMutableArray<AnyObject>,
            _connection: &AnyObject,
        ) {
            // Look up which widget this delegate belongs to by matching the
            // self pointer in our QR_SESSIONS map.
            let self_ptr = self as *const _ as *mut AnyObject;
            let widget_tag = QR_SESSIONS.with(|map| {
                map.borrow().iter().find_map(|(tag, entry)| {
                    if entry.delegate == self_ptr {
                        Some(*tag)
                    } else {
                        None
                    }
                })
            });
            let Some(tag) = widget_tag else { return };

            // Walk every metadata object in the batch.
            let count: usize = unsafe { msg_send![objects, count] };
            for i in 0..count {
                let obj: *mut AnyObject = unsafe { msg_send![objects, objectAtIndex: i] };
                if obj.is_null() {
                    continue;
                }
                // Check that this is a QR metadata object.
                let type_str: *mut AnyObject = unsafe { msg_send![obj, type] };
                if type_str.is_null() {
                    continue;
                }
                // AVMetadataObjectTypeQRCode = "org.iso.QRCode"
                let expected = NSString::from_str("org.iso.QRCode");
                let is_qr: bool =
                    unsafe { msg_send![type_str, isEqualToString: &*expected] };
                if !is_qr {
                    continue;
                }
                // `stringValue` holds the decoded payload.
                let string_value: *mut AnyObject = unsafe { msg_send![obj, stringValue] };
                if string_value.is_null() {
                    continue;
                }
                let ns_str: *const NSString = string_value.cast();
                let value = unsafe { (*ns_str).to_string() };

                PENDING_QR.with(|q| q.borrow_mut().push((tag, value)));
            }
        }
    }
);

fn apns_device_token_hex(device_token: &NSData) -> Option<String> {
    unsafe {
        let len: usize = msg_send![device_token, length];
        let bytes: *const u8 = msg_send![device_token, bytes];
        if bytes.is_null() || len == 0 {
            return None;
        }

        let slice = std::slice::from_raw_parts(bytes, len);
        let mut token = String::with_capacity(len * 2);
        use std::fmt::Write as _;
        for byte in slice {
            let _ = write!(&mut token, "{byte:02x}");
        }
        Some(token)
    }
}

fn objc_class(name: &str) -> Option<&'static objc2::runtime::AnyClass> {
    use std::ffi::CString;
    let c = CString::new(name).ok()?;
    objc2::runtime::AnyClass::get(c.as_c_str())
}

fn queue_permission_result(permission: PermissionKind, status: PermissionStatus) {
    if let Ok(mut results) = PENDING_PERMISSION_RESULTS.lock() {
        results.push((permission, status));
    }
}

fn queue_media_result(images: Vec<Arc<Vec<u8>>>) {
    if let Ok(mut results) = PENDING_MEDIA_RESULTS.lock() {
        results.push(images);
    }
}

fn queue_media_cancel() {
    if let Ok(mut cancellations) = PENDING_MEDIA_CANCELS.lock() {
        *cancellations = cancellations.saturating_add(1);
    }
}

struct MediaLoadState {
    remaining: usize,
    images: Vec<Arc<Vec<u8>>>,
}

fn load_ios_media_picker_results(results: &NSMutableArray<AnyObject>) {
    let count: usize = unsafe { msg_send![results, count] };
    if count == 0 {
        queue_media_cancel();
        return;
    }

    let state = Arc::new(Mutex::new(MediaLoadState {
        remaining: count,
        images: Vec::with_capacity(count),
    }));

    for index in 0..count {
        let result: *mut AnyObject = unsafe { msg_send![results, objectAtIndex: index] };
        if result.is_null() {
            complete_ios_media_load(state.clone(), None);
            continue;
        }

        let provider: *mut AnyObject = unsafe { msg_send![result, itemProvider] };
        let type_identifier = match ios_media_provider_type(provider) {
            Some(identifier) => identifier,
            None => {
                complete_ios_media_load(state.clone(), None);
                continue;
            }
        };

        unsafe { objc_retain(provider) };
        let state_for_block = state.clone();
        let completion = RcBlock::new(move |data: *mut AnyObject, _error: *mut NSObject| {
            let bytes = unsafe { nsdata_to_arc_bytes(data) };
            complete_ios_media_load(state_for_block.clone(), bytes);
            unsafe { objc_release(provider) };
        });
        unsafe {
            let identifier = NSString::from_str(type_identifier);
            let _: () = msg_send![
                provider,
                loadDataRepresentationForTypeIdentifier: &*identifier
                completionHandler: &*completion
            ];
        }
    }
}

fn ios_media_provider_type(provider: *mut AnyObject) -> Option<&'static str> {
    if provider.is_null() {
        return None;
    }
    for identifier in ["public.image", "public.jpeg", "public.png"] {
        let ns = NSString::from_str(identifier);
        let supported: bool =
            unsafe { msg_send![provider, hasItemConformingToTypeIdentifier: &*ns] };
        if supported {
            return Some(identifier);
        }
    }
    None
}

fn complete_ios_media_load(state: Arc<Mutex<MediaLoadState>>, bytes: Option<Arc<Vec<u8>>>) {
    let finished = {
        let Ok(mut state) = state.lock() else {
            return;
        };
        if let Some(bytes) = bytes {
            state.images.push(bytes);
        }
        state.remaining = state.remaining.saturating_sub(1);
        if state.remaining == 0 {
            Some(std::mem::take(&mut state.images))
        } else {
            None
        }
    };

    if let Some(images) = finished {
        if images.is_empty() {
            queue_media_cancel();
        } else {
            queue_media_result(images);
        }
    }
}

unsafe fn nsdata_to_arc_bytes(data: *mut AnyObject) -> Option<Arc<Vec<u8>>> {
    if data.is_null() {
        return None;
    }
    let len: usize = msg_send![data, length];
    let ptr: *const u8 = msg_send![data, bytes];
    if ptr.is_null() || len == 0 {
        return None;
    }
    Some(Arc::new(std::slice::from_raw_parts(ptr, len).to_vec()))
}

fn permission_from_av_status(status: isize) -> PermissionStatus {
    match status {
        0 => PermissionStatus::NotDetermined,
        1 => PermissionStatus::Restricted,
        2 => PermissionStatus::Denied,
        3 => PermissionStatus::Granted,
        _ => PermissionStatus::Unknown,
    }
}

fn permission_from_location_status(status: isize) -> PermissionStatus {
    match status {
        0 => PermissionStatus::NotDetermined,
        1 => PermissionStatus::Restricted,
        2 => PermissionStatus::Denied,
        3 | 4 => PermissionStatus::Granted,
        _ => PermissionStatus::Unknown,
    }
}

fn permission_from_photo_status(status: isize) -> PermissionStatus {
    match status {
        0 => PermissionStatus::NotDetermined,
        1 => PermissionStatus::Restricted,
        2 => PermissionStatus::Denied,
        3 => PermissionStatus::Granted,
        4 => PermissionStatus::Limited,
        _ => PermissionStatus::Unknown,
    }
}

fn permission_from_notification_status(status: isize) -> PermissionStatus {
    match status {
        0 => PermissionStatus::NotDetermined,
        1 => PermissionStatus::Denied,
        2 | 3 | 4 => PermissionStatus::Granted,
        _ => PermissionStatus::Unknown,
    }
}

fn av_media_type_for_permission(permission: PermissionKind) -> Option<Retained<NSString>> {
    match permission {
        PermissionKind::Camera => Some(NSString::from_str("vide")),
        PermissionKind::Microphone => Some(NSString::from_str("soun")),
        _ => None,
    }
}

fn check_ios_permission(permission: PermissionKind) {
    unsafe {
        let status = match permission {
            PermissionKind::Location => {
                let status: isize = msg_send![class!(CLLocationManager), authorizationStatus];
                permission_from_location_status(status)
            }
            PermissionKind::Camera | PermissionKind::Microphone => {
                let Some(media_type) = av_media_type_for_permission(permission) else {
                    return;
                };
                let status: isize = msg_send![
                    av_class("AVCaptureDevice"),
                    authorizationStatusForMediaType: &*media_type
                ];
                permission_from_av_status(status)
            }
            PermissionKind::Photos => {
                let Some(photo_library) = objc_class("PHPhotoLibrary") else {
                    queue_permission_result(permission, PermissionStatus::Unsupported);
                    return;
                };
                let status: isize = msg_send![photo_library, authorizationStatus];
                permission_from_photo_status(status)
            }
            PermissionKind::Notifications => {
                check_ios_notification_permission();
                return;
            }
            PermissionKind::Motion => PermissionStatus::Granted,
        };
        queue_permission_result(permission, status);
    }
}

fn check_ios_notification_permission() {
    unsafe {
        let center: *mut AnyObject =
            msg_send![class!(UNUserNotificationCenter), currentNotificationCenter];
        if center.is_null() {
            queue_permission_result(PermissionKind::Notifications, PermissionStatus::Unsupported);
            return;
        }
        let completion = RcBlock::new(|settings: *mut AnyObject| {
            if settings.is_null() {
                queue_permission_result(PermissionKind::Notifications, PermissionStatus::Unknown);
                return;
            }
            let status: isize = msg_send![settings, authorizationStatus];
            queue_permission_result(
                PermissionKind::Notifications,
                permission_from_notification_status(status),
            );
        });
        let _: () = msg_send![
            center,
            getNotificationSettingsWithCompletionHandler: &*completion
        ];
    }
}

fn request_ios_permission(permission: PermissionKind) {
    match permission {
        PermissionKind::Location => request_ios_location_permission(),
        PermissionKind::Camera | PermissionKind::Microphone => {
            request_ios_av_permission(permission);
        }
        PermissionKind::Photos => request_ios_photo_permission(),
        PermissionKind::Notifications => request_ios_notification_permission(),
        PermissionKind::Motion => {
            queue_permission_result(PermissionKind::Motion, PermissionStatus::Granted);
        }
    }
}

fn request_ios_location_permission() {
    unsafe {
        let mgr: *mut AnyObject = msg_send![class!(CLLocationManager), new];
        if mgr.is_null() {
            queue_permission_result(PermissionKind::Location, PermissionStatus::Unsupported);
            return;
        }

        let delegate: Retained<LocationDelegate> = msg_send![LocationDelegate::class(), new];
        let _: () = msg_send![mgr, setDelegate: &*delegate];
        let _: () = msg_send![mgr, requestWhenInUseAuthorization];
        PERMISSION_LOCATION_MANAGER.with(|lm| {
            let old = {
                let mut slot = lm.borrow_mut();
                slot.replace(mgr)
            };
            if let Some(old) = old {
                let nil_delegate: *const AnyObject = std::ptr::null();
                let _: () = msg_send![old, setDelegate: nil_delegate];
                objc_release(old);
            }
        });
        PERMISSION_LOCATION_DELEGATE.with(|d| *d.borrow_mut() = Some(delegate));
        check_ios_permission(PermissionKind::Location);
    }
}

fn request_ios_av_permission(permission: PermissionKind) {
    let Some(media_type) = av_media_type_for_permission(permission) else {
        return;
    };
    unsafe {
        let completion = RcBlock::new(move |granted: objc2::runtime::Bool| {
            let status = if granted.as_bool() {
                PermissionStatus::Granted
            } else {
                PermissionStatus::Denied
            };
            queue_permission_result(permission, status);
        });
        let _: () = msg_send![
            av_class("AVCaptureDevice"),
            requestAccessForMediaType: &*media_type
            completionHandler: &*completion
        ];
    }
}

fn request_ios_photo_permission() {
    unsafe {
        let Some(photo_library) = objc_class("PHPhotoLibrary") else {
            queue_permission_result(PermissionKind::Photos, PermissionStatus::Unsupported);
            return;
        };
        let completion = RcBlock::new(|status: isize| {
            queue_permission_result(PermissionKind::Photos, permission_from_photo_status(status));
        });
        let _: () = msg_send![photo_library, requestAuthorization: &*completion];
    }
}

fn request_ios_notification_permission() {
    unsafe {
        let center: *mut AnyObject =
            msg_send![class!(UNUserNotificationCenter), currentNotificationCenter];
        if center.is_null() {
            queue_permission_result(PermissionKind::Notifications, PermissionStatus::Unsupported);
            return;
        }
        let completion = RcBlock::new(|granted: objc2::runtime::Bool, _error: *mut NSObject| {
            let status = if granted.as_bool() {
                PermissionStatus::Granted
            } else {
                PermissionStatus::Denied
            };
            queue_permission_result(PermissionKind::Notifications, status);
        });
        let _: () = msg_send![
            center,
            requestAuthorizationWithOptions: 0b111usize
            completionHandler: &*completion
        ];
    }
}

fn start_ios_location_updates() {
    unsafe {
        let mgr: *mut AnyObject = msg_send![class!(CLLocationManager), new];
        if mgr.is_null() {
            return;
        }

        let delegate: Retained<LocationDelegate> = msg_send![LocationDelegate::class(), new];
        let _: () = msg_send![mgr, setDelegate: &*delegate];
        let _: () = msg_send![mgr, requestWhenInUseAuthorization];
        let _: () = msg_send![mgr, startUpdatingLocation];

        LOCATION_MANAGER.with(|lm| {
            let old = {
                let mut slot = lm.borrow_mut();
                slot.replace(mgr)
            };
            if let Some(old) = old {
                let nil_delegate: *const AnyObject = std::ptr::null();
                let _: () = msg_send![old, setDelegate: nil_delegate];
                let _: () = msg_send![old, stopUpdatingLocation];
                objc_release(old);
            }
        });
        LOCATION_DELEGATE.with(|d| *d.borrow_mut() = Some(delegate));
    }
}

fn stop_ios_location_updates() {
    LOCATION_MANAGER.with(|lm| {
        if let Some(mgr) = lm.borrow_mut().take() {
            unsafe {
                let nil_delegate: *const AnyObject = std::ptr::null();
                let _: () = msg_send![mgr, setDelegate: nil_delegate];
                let _: () = msg_send![mgr, stopUpdatingLocation];
                objc_release(mgr);
            }
        }
    });
    LOCATION_DELEGATE.with(|d| *d.borrow_mut() = None);
}

define_class!(
    /// CLLocationManagerDelegate that queues GPS fixes and auth denials for the next tick.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RaxLocationDelegate"]
    struct LocationDelegate;

    unsafe impl NSObjectProtocol for LocationDelegate {}

    impl LocationDelegate {
        /// `locationManager:didUpdateLocations:` — fires when new fixes arrive.
        #[unsafe(method(locationManager:didUpdateLocations:))]
        fn did_update_locations(&self, _manager: &AnyObject, locations: &NSMutableArray<AnyObject>) {
            let count: usize = unsafe { msg_send![locations, count] };
            if count == 0 {
                return;
            }
            let last: *mut AnyObject = unsafe { msg_send![locations, objectAtIndex: count - 1] };
            if last.is_null() {
                return;
            }
            // CLLocationCoordinate2D = {CLLocationDegrees latitude; CLLocationDegrees longitude;}
            // where CLLocationDegrees = f64.  The struct has the same ABI as CGPoint = {f64, f64}
            // on arm64 (System-V / Apple ABI: two f64 in registers or on stack).
            // We reuse CGPoint (which already implements objc2::Encode) to receive the return
            // value and interpret x = latitude, y = longitude.
            let (lat, lon): (f64, f64) = unsafe {
                let coord: CGPoint = msg_send![last, coordinate];
                // x maps to latitude (first field), y maps to longitude (second field).
                (coord.x, coord.y)
            };
            let accuracy: f64 = unsafe { msg_send![last, horizontalAccuracy] };
            PENDING_LOCATIONS.with(|q| q.borrow_mut().push((lat, lon, accuracy)));
        }

        /// `locationManager:didFailWithError:` — treat failures as denied.
        #[unsafe(method(locationManager:didFailWithError:))]
        fn did_fail(&self, _manager: &AnyObject, _error: &AnyObject) {
            PENDING_LOCATION_DENIED.with(|c| c.set(true));
            queue_permission_result(PermissionKind::Location, PermissionStatus::Denied);
        }

        /// `locationManagerDidChangeAuthorization:` — fires on iOS 14+ when status changes.
        #[unsafe(method(locationManagerDidChangeAuthorization:))]
        fn did_change_auth(&self, manager: &AnyObject) {
            // kCLAuthorizationStatusDenied = 2, kCLAuthorizationStatusRestricted = 1
            let status: isize = unsafe { msg_send![manager, authorizationStatus] };
            if status == 1 || status == 2 {
                PENDING_LOCATION_DENIED.with(|c| c.set(true));
            }
            queue_permission_result(PermissionKind::Location, permission_from_location_status(status));
        }
    }
);

define_class!(
    /// PHPickerViewControllerDelegate that queues picked image results for the next tick.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RaxMediaPickerDelegate"]
    struct MediaPickerDelegate;

    unsafe impl NSObjectProtocol for MediaPickerDelegate {}

    impl MediaPickerDelegate {
        /// `picker:didFinishPicking:` — fires when the user finishes picking or cancels.
        #[unsafe(method(picker:didFinishPicking:))]
        fn did_finish_picking(&self, picker: &AnyObject, results: &NSMutableArray<AnyObject>) {
            unsafe {
                let null_completion: *const AnyObject = std::ptr::null();
                let _: () = msg_send![picker, dismissViewControllerAnimated: true completion: null_completion];
            }

            let count: usize = unsafe { msg_send![results, count] };
            if count == 0 {
                queue_media_cancel();
                // Drop the delegate reference now that we are done.
                MEDIA_PICKER_DELEGATE.with(|d| *d.borrow_mut() = None);
                return;
            }

            load_ios_media_picker_results(results);
            // Drop the delegate now that the pick is complete.
            MEDIA_PICKER_DELEGATE.with(|d| *d.borrow_mut() = None);
        }
    }
);

define_class!(
    /// UIDocumentPickerDelegate that reads picked files' bytes and queues them
    /// for dispatch on the next tick as `Event::DocumentPicked`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RaxDocumentPickerDelegate"]
    struct DocumentPickerDelegate;

    unsafe impl NSObjectProtocol for DocumentPickerDelegate {}

    impl DocumentPickerDelegate {
        /// `documentPicker:didPickDocumentsAtURLs:` — fires with the chosen file URLs.
        #[unsafe(method(documentPicker:didPickDocumentsAtURLs:))]
        fn did_pick(&self, _picker: &AnyObject, urls: &NSMutableArray<AnyObject>) {
            let mut files: Vec<(String, Vec<u8>)> = Vec::new();
            unsafe {
                let count: usize = msg_send![urls, count];
                for i in 0..count {
                    let url: *mut AnyObject = msg_send![urls, objectAtIndex: i];
                    if url.is_null() {
                        continue;
                    }
                    // Security-scoped resources must be opened before reading.
                    let scoped: bool = msg_send![url, startAccessingSecurityScopedResource];
                    let name = {
                        let comp: *mut AnyObject = msg_send![url, lastPathComponent];
                        if comp.is_null() {
                            String::from("file")
                        } else {
                            (*(comp as *const NSString)).to_string()
                        }
                    };
                    let data: *mut AnyObject = msg_send![class!(NSData), dataWithContentsOfURL: url];
                    if !data.is_null() {
                        let len: usize = msg_send![data, length];
                        let ptr: *const u8 = msg_send![data, bytes];
                        let bytes = if ptr.is_null() || len == 0 {
                            Vec::new()
                        } else {
                            std::slice::from_raw_parts(ptr, len).to_vec()
                        };
                        files.push((name, bytes));
                    }
                    if scoped {
                        let _: () = msg_send![url, stopAccessingSecurityScopedResource];
                    }
                }
            }
            PENDING_DOCUMENTS.with(|q| q.borrow_mut().push(files));
            DOCUMENT_PICKER_DELEGATE.with(|d| *d.borrow_mut() = None);
        }

        /// `documentPickerWasCancelled:` — user dismissed without picking.
        #[unsafe(method(documentPickerWasCancelled:))]
        fn was_cancelled(&self, _picker: &AnyObject) {
            // Deliver an empty pick so the app can react if it wants.
            PENDING_DOCUMENTS.with(|q| q.borrow_mut().push(Vec::new()));
            DOCUMENT_PICKER_DELEGATE.with(|d| *d.borrow_mut() = None);
        }
    }
);

/// Returns the ObjC class for `name`, looked up at runtime. Panics in debug if
/// not found (the class must be linked into the binary, i.e. AVFoundation must
/// be in the linker inputs).
fn av_class(name: &str) -> &'static objc2::runtime::AnyClass {
    use std::ffi::CString;
    let c = CString::new(name).unwrap();
    objc2::runtime::AnyClass::get(c.as_c_str())
        .unwrap_or_else(|| panic!("ObjC class not found: {name}"))
}

/// Retain a raw ObjC object pointer.
unsafe fn objc_retain(obj: *mut AnyObject) {
    if !obj.is_null() {
        let _: *mut AnyObject = msg_send![obj, retain];
    }
}

/// Release a raw ObjC object pointer.
unsafe fn objc_release(obj: *mut AnyObject) {
    if !obj.is_null() {
        let _: () = msg_send![obj, release];
    }
}

fn set_ios_torch(on: bool) {
    unsafe {
        let media_type = NSString::from_str("vide"); // AVMediaTypeVideo
        let device: *mut AnyObject = msg_send![
            av_class("AVCaptureDevice"),
            defaultDeviceWithMediaType: &*media_type
        ];
        if device.is_null() {
            return;
        }

        let has_torch: bool = msg_send![device, hasTorch];
        if !has_torch {
            return;
        }
        if on {
            let available: bool = msg_send![device, isTorchAvailable];
            if !available {
                return;
            }
        }

        let mut error: *mut AnyObject = std::ptr::null_mut();
        let locked: bool = msg_send![device, lockForConfiguration: &mut error];
        if !locked || !error.is_null() {
            return;
        }

        // AVCaptureTorchModeOff = 0, On = 1.
        let mode: isize = if on { 1 } else { 0 };
        let _: () = msg_send![device, setTorchMode: mode];
        let _: () = msg_send![device, unlockForConfiguration];
    }
}

fn register_ios_remote_notifications() {
    unsafe {
        let center: *mut AnyObject =
            msg_send![class!(UNUserNotificationCenter), currentNotificationCenter];
        if !center.is_null() {
            // UNAuthorizationOptionBadge | Sound | Alert.
            let options: usize = 1 | 2 | 4;
            let completion =
                RcBlock::new(|_granted: objc2::runtime::Bool, _error: *mut NSObject| {});
            let _: () = msg_send![
                center,
                requestAuthorizationWithOptions: options
                completionHandler: &*completion
            ];
        }

        let app: *mut AnyObject = msg_send![class!(UIApplication), sharedApplication];
        if !app.is_null() {
            let _: () = msg_send![app, registerForRemoteNotifications];
        }
    }
}

fn set_ios_app_badge(count: u32) {
    unsafe {
        let app: *mut AnyObject = msg_send![class!(UIApplication), sharedApplication];
        if !app.is_null() {
            let _: () = msg_send![app, setApplicationIconBadgeNumber: count as isize];
        }
    }
}

/// Starts an AVCaptureSession on `view`, routing QR detections to `widget_tag`.
///
/// The session and delegate are owned by `QR_SESSIONS` (thread-local) and are
/// released when [`stop_qr_scanner`] is called for the same tag.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
fn setup_qr_scanner(view: *const UIView, widget_tag: u64) {
    unsafe {
        // ── Session ────────────────────────────────────────────────────────
        let session: *mut AnyObject = msg_send![av_class("AVCaptureSession"), new];
        if session.is_null() {
            return;
        }

        // ── Video input device ─────────────────────────────────────────────
        let media_type = NSString::from_str("vide"); // AVMediaTypeVideo
        let device: *mut AnyObject = msg_send![
            av_class("AVCaptureDevice"),
            defaultDeviceWithMediaType: &*media_type
        ];
        if device.is_null() {
            objc_release(session);
            return;
        }

        let mut error: *mut AnyObject = std::ptr::null_mut();
        let input: *mut AnyObject = msg_send![
            av_class("AVCaptureDeviceInput"),
            deviceInputWithDevice: device
            error: &mut error
        ];
        if input.is_null() || !error.is_null() {
            objc_release(session);
            return;
        }

        let can_add_input: bool = msg_send![session, canAddInput: input];
        if can_add_input {
            let _: () = msg_send![session, addInput: input];
        }

        // ── Metadata output ────────────────────────────────────────────────
        let output: *mut AnyObject = msg_send![av_class("AVCaptureMetadataOutput"), new];
        if output.is_null() {
            objc_release(session);
            return;
        }

        let can_add_output: bool = msg_send![session, canAddOutput: output];
        if can_add_output {
            let _: () = msg_send![session, addOutput: output];
        }

        // ── Delegate (set *after* adding the output to the session) ────────
        let delegate: *mut AnyObject = msg_send![QrDelegate::class(), new];

        // Use the main queue for callbacks — same thread as handle_tick so
        // PENDING_QR doesn't need a Mutex.
        // `dispatch_get_main_queue()` is a C `static inline` that returns the
        // address of the `_dispatch_main_q` global — there is no callable symbol
        // by that name, so reference the global directly.
        let main_queue: *mut AnyObject = {
            extern "C" {
                static _dispatch_main_q: objc2::runtime::AnyObject;
            }
            (&_dispatch_main_q as *const objc2::runtime::AnyObject) as *mut AnyObject
        };
        let _: () = msg_send![output, setDelegate: delegate callbackQueue: main_queue];

        // Restrict to QR codes only.
        let qr_type = NSString::from_str("org.iso.QRCode");
        let types = NSMutableArray::<AnyObject>::new();
        let qr_obj: &AnyObject = &*(qr_type.as_ref() as *const _ as *const AnyObject);
        types.addObject(qr_obj);
        let _: () = msg_send![output, setMetadataObjectTypes: &*types];

        // ── Preview layer ──────────────────────────────────────────────────
        let preview: *mut AnyObject =
            msg_send![av_class("AVCaptureVideoPreviewLayer"), layerWithSession: session];
        if !preview.is_null() {
            // Fill the view completely.
            let gravity = NSString::from_str("AVLayerVideoGravityResizeAspectFill");
            let _: () = msg_send![preview, setVideoGravity: &*gravity];

            let view_layer: *mut AnyObject = msg_send![&*view, layer];
            let _: () = msg_send![view_layer, addSublayer: preview];

            let bounds: CGRect = msg_send![&*view, bounds];
            let _: () = msg_send![preview, setFrame: bounds];
        }

        // ── Own the session + delegate in our thread-local map ─────────────
        // `new` already retains; retain once more so the entry holds a
        // reference independent of any autorelease pool.
        objc_retain(session);
        objc_retain(delegate);

        QR_SESSIONS.with(|map| {
            map.borrow_mut()
                .insert(widget_tag, QrEntry { session, delegate });
        });

        // Release the initial +new reference — the map entry holds the only
        // surviving retain count now.
        objc_release(session);
        objc_release(delegate);

        // ── Start ──────────────────────────────────────────────────────────
        QR_SESSIONS.with(|map| {
            if let Some(entry) = map.borrow().get(&widget_tag) {
                let _: () = msg_send![entry.session, startRunning];
            }
        });
    }
}

/// Stops and discards the `AVCaptureSession` for `widget_tag`, if any.
fn stop_qr_scanner(widget_tag: u64) {
    let entry = QR_SESSIONS.with(|map| map.borrow_mut().remove(&widget_tag));
    if let Some(e) = entry {
        unsafe {
            let _: () = msg_send![e.session, stopRunning];
            objc_release(e.session);
            objc_release(e.delegate);
        }
    }
}

fn new_instance<T: ClassType>() -> Retained<T> {
    unsafe { msg_send![T::class(), new] }
}

// ---------------------------------------------------------------------------
// Bootstrap
// ---------------------------------------------------------------------------

fn setup(mtm: MainThreadMarker) {
    // Install the ureq-backed HTTP client for the main thread.
    crate::net::set_client(super::http::UreqClient);
    // Persist rax-store keys to NSUserDefaults across launches.
    crate::store::set_storage(super::storage::UiKitStorage);

    let screen = UIScreen::mainScreen(mtm);
    let bounds = screen.bounds();

    let window: Retained<UIWindow> = unsafe { UIWindow::initWithFrame(mtm.alloc(), bounds) };
    let view_controller: Retained<UIViewController> =
        unsafe { UIViewController::initWithNibName_bundle(mtm.alloc(), None, None) };
    let container = view_controller
        .view()
        .expect("view controller has a content view");

    let action_target: Retained<ActionTarget> = new_instance();

    // Observe keyboard show/hide so the runtime can inset content above it.
    unsafe {
        let center = NSNotificationCenter::defaultCenter();
        center.addObserver_selector_name_object(
            &action_target,
            sel!(keyboardWillShow:),
            Some(&NSString::from_str("UIKeyboardWillShowNotification")),
            None,
        );
        center.addObserver_selector_name_object(
            &action_target,
            sel!(keyboardWillHide:),
            Some(&NSString::from_str("UIKeyboardWillHideNotification")),
            None,
        );
    }

    let backend = UiKitBackend {
        mtm,
        container: container.clone(),
        action_target: action_target.clone(),
        views: HashMap::new(),
        gradient_layers: HashMap::new(),
        blur_views: HashMap::new(),
        accessibility_action_lists: HashMap::new(),
        scroll_delegates: HashMap::new(),
        press_recognizers: HashMap::new(),
        swipe_recognizers: HashMap::new(),
    };

    let viewport = Size::new(bounds.size.width as f32, bounds.size.height as f32);
    let factory = FACTORY
        .with(|f| f.borrow_mut().take())
        .expect("run() set the factory");
    crate::plugin::start_plugins();
    let app = factory(Host::new(backend), viewport);
    let event_sink = app.event_sink();
    let app = Rc::new(RefCell::new(app));

    unsafe { window.setRootViewController(Some(&view_controller)) };
    window.makeKeyAndVisible();

    let ticker: Retained<Ticker> = new_instance();
    let display_link =
        unsafe { CADisplayLink::displayLinkWithTarget_selector(&ticker, sel!(tick:)) };

    // Request 120fps on ProMotion displays. Falls back to 60fps on non-ProMotion.
    // preferredFrameRateRange is available on iOS 15+.
    unsafe {
        // CAFrameRateRange { minimum: f32, maximum: f32, preferred: f32 }
        // We call objc_msgSend directly to pass the struct by value without needing
        // to implement objc2::Encode for a local type. This matches the ABI for
        // arm64 (three f32 fields in a struct, passed as a single struct argument).
        #[repr(C)]
        struct CAFrameRateRange {
            minimum: f32,
            maximum: f32,
            preferred: f32,
        }
        extern "C" {
            fn objc_msgSend();
        }
        // SAFETY: this is a best-effort ProMotion request; if the selector doesn't
        // exist on the running OS version it will no-op at the ObjC runtime level.
        let sel_set_preferred_frame_rate_range = objc2::sel!(setPreferredFrameRateRange:);
        let range = CAFrameRateRange {
            minimum: 60.0,
            maximum: 120.0,
            preferred: 120.0,
        };
        let fn_ptr: unsafe extern "C" fn(*const AnyObject, objc2::runtime::Sel, CAFrameRateRange) =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        fn_ptr(
            &*display_link as *const _ as *const AnyObject,
            sel_set_preferred_frame_rate_range,
            range,
        );
    }
    // Fallback: request 120fps via the older API (ignored on iOS 15+ in favor of range)
    unsafe {
        let _: () = msg_send![&*display_link, setPreferredFramesPerSecond: 120i64];
    }

    let run_loop = unsafe { objc2_foundation::NSRunLoop::mainRunLoop() };
    unsafe {
        display_link.addToRunLoop_forMode(&run_loop, objc2_foundation::NSDefaultRunLoopMode);
    }

    STATE.with(|s| {
        *s.borrow_mut() = Some(IosState {
            app,
            event_sink,
            _window: window,
            _view_controller: view_controller,
            _display_link: display_link,
            _ticker: ticker,
        });
    });
}

/// Entry point: hand control to UIKit. Never returns.
pub fn run<V, F>(make_view: F) -> !
where
    F: FnOnce() -> V + 'static,
    V: View,
{
    FACTORY.with(|f| {
        *f.borrow_mut() = Some(Box::new(move |host, viewport| {
            App::new(host, viewport, make_view)
        }));
    });

    let mtm = MainThreadMarker::new().expect("run() must be called on the main thread");
    let delegate_name = NSString::from_class(AppDelegate::class());
    // UIApplicationMain never returns; its `!` return type makes `run` diverge.
    UIApplication::main(None, Some(&delegate_name), mtm)
}

// ---------------------------------------------------------------------------
// The backend: Mutation stream -> UIKit.
// ---------------------------------------------------------------------------

struct UiKitBackend {
    mtm: MainThreadMarker,
    container: Retained<UIView>,
    action_target: Retained<ActionTarget>,
    views: HashMap<u64, Retained<UIView>>,
    /// Gradient sublayers keyed by widget id, so we can resize them to match the
    /// view's bounds whenever its frame changes.
    gradient_layers: HashMap<u64, Retained<CAGradientLayer>>,
    /// Native blur subviews keyed by widget id. They are inserted behind
    /// regular children and resized with the owning view's bounds.
    blur_views: HashMap<u64, Retained<UIVisualEffectView>>,
    /// Retained iOS custom accessibility action arrays keyed by widget id.
    accessibility_action_lists: HashMap<u64, Retained<NSMutableArray<UIAccessibilityCustomAction>>>,
    /// Scroll delegates keyed by widget id. `UIScrollView.delegate` is weak, so
    /// the backend must hold these strongly for as long as the view exists.
    scroll_delegates: HashMap<u64, Retained<ScrollDelegate>>,
    /// Zero-duration long-press recognizers used for press-in / press-out callbacks.
    press_recognizers: HashMap<u64, Retained<UILongPressGestureRecognizer>>,
    /// Directional swipe recognizers keyed by widget id and UIKit direction bits.
    swipe_recognizers: HashMap<(u64, u8), Retained<UIGestureRecognizer>>,
}

impl UiKitBackend {
    fn view(&self, id: WidgetId) -> Option<&Retained<UIView>> {
        self.views.get(&id.to_u64())
    }

    /// Installs a long-press context menu on `view`: stores the items keyed by
    /// the widget id (recovered from the view tag on fire) and attaches a
    /// `UILongPressGestureRecognizer` that presents them as a native action
    /// sheet via [`present_context_menu`].
    fn install_context_menu(&self, id: WidgetId, view: &UIView, items: Vec<MenuItem>) {
        CONTEXT_MENUS.with(|m| {
            m.borrow_mut().insert(id.to_u64(), items);
        });
        unsafe {
            view.setUserInteractionEnabled(true);
            // Same tag convention as AddGesture so `recognizer_tag` recovers the id.
            view.setTag(id.to_u64() as isize);
            let r = UILongPressGestureRecognizer::initWithTarget_action(
                self.mtm.alloc(),
                Some(&self.action_target),
                Some(sel!(contextMenuLongPress:)),
            );
            view.addGestureRecognizer(&r);
        }
    }

    fn install_scroll_delegate(&mut self, id: WidgetId, scroll_view: &UIScrollView) {
        let tag = id.to_u64();
        scroll_view.setTag(tag as isize);
        let delegate = self
            .scroll_delegates
            .entry(tag)
            .or_insert_with(new_instance::<ScrollDelegate>);
        let protocol_delegate: &ProtocolObject<dyn UIScrollViewDelegate> =
            ProtocolObject::from_ref(&**delegate);
        unsafe {
            scroll_view.setDelegate(Some(protocol_delegate));
        }
    }

    fn clear_scroll_delegate(&mut self, id: WidgetId) {
        let tag = id.to_u64();
        if let Some(view) = self.views.get(&tag) {
            if let Ok(scroll_view) = view.clone().downcast::<UIScrollView>() {
                unsafe {
                    scroll_view.setDelegate(None);
                }
            }
        }
        self.scroll_delegates.remove(&tag);
        clear_scroll_handlers(tag);
    }

    fn install_press_recognizer(&mut self, id: WidgetId, view: &UIView) {
        let tag = id.to_u64();
        if self.press_recognizers.contains_key(&tag) {
            return;
        }

        unsafe {
            view.setUserInteractionEnabled(true);
            view.setTag(tag as isize);
        }
        let recognizer = unsafe {
            UILongPressGestureRecognizer::initWithTarget_action(
                self.mtm.alloc(),
                Some(&self.action_target),
                Some(sel!(pressRecognized:)),
            )
        };
        recognizer.setMinimumPressDuration(0.0);
        unsafe {
            let _: () = msg_send![&*recognizer, setCancelsTouchesInView: false];
            let _: () = msg_send![&*recognizer, setDelaysTouchesBegan: false];
            let _: () = msg_send![&*recognizer, setDelaysTouchesEnded: false];
            let _: () = msg_send![&*recognizer, setDelegate: &*self.action_target];
        }
        view.addGestureRecognizer(&recognizer);
        self.press_recognizers.insert(tag, recognizer);
    }

    fn install_swipe_recognizer(&mut self, id: WidgetId, view: &UIView, direction: SwipeDirection) {
        let tag = id.to_u64();
        let direction_bits = swipe_direction_bits(direction);
        let key = (tag, direction_bits);
        if self.swipe_recognizers.contains_key(&key) {
            return;
        }

        unsafe {
            view.setUserInteractionEnabled(true);
            view.setTag(tag as isize);
            let recognizer: *mut AnyObject = msg_send![class!(UISwipeGestureRecognizer), alloc];
            let recognizer: *mut AnyObject = msg_send![
                recognizer,
                initWithTarget: &*self.action_target
                action: sel!(swipeRecognized:)
            ];
            if recognizer.is_null() {
                return;
            }
            let _: () = msg_send![recognizer, setDirection: direction_bits as usize];
            let _: () = msg_send![recognizer, setCancelsTouchesInView: false];
            let _: () = msg_send![recognizer, setDelegate: &*self.action_target];
            let recognizer = Retained::from_raw(recognizer.cast::<UIGestureRecognizer>())
                .expect("UISwipeGestureRecognizer init returned a valid object");
            view.addGestureRecognizer(&recognizer);
            self.swipe_recognizers.insert(key, recognizer);
        }
    }

    fn clear_interaction_recognizers(&mut self, id: WidgetId) {
        let tag = id.to_u64();
        self.press_recognizers.remove(&tag);
        self.swipe_recognizers
            .retain(|(recognizer_tag, _), _| *recognizer_tag != tag);
        clear_interaction_handlers(tag);
    }

    fn blur_frame_for(view: &UIView) -> CGRect {
        let bounds = view.bounds();
        CGRect {
            origin: CGPoint { x: 0.0, y: 0.0 },
            size: bounds.size,
        }
    }

    fn blur_style_for_radius(radius: f32) -> UIBlurEffectStyle {
        if radius >= 24.0 {
            UIBlurEffectStyle::Prominent
        } else if radius >= 10.0 {
            UIBlurEffectStyle::Regular
        } else {
            UIBlurEffectStyle::Light
        }
    }

    fn blur_alpha_for_radius(radius: f32) -> f64 {
        (radius / 20.0).clamp(0.15, 1.0) as f64
    }

    fn apply_blur(&mut self, id: WidgetId, view: &UIView, radius: f32) {
        let tag = id.to_u64();
        if radius <= 0.0 {
            self.clear_blur(id);
            return;
        }

        let effect = UIBlurEffect::effectWithStyle(Self::blur_style_for_radius(radius), self.mtm);
        let frame = Self::blur_frame_for(view);
        let alpha = Self::blur_alpha_for_radius(radius);

        if let Some(blur_view) = self.blur_views.get(&tag) {
            blur_view.setEffect(Some(effect.as_super()));
            let blur_view = blur_view.as_super();
            blur_view.setFrame(frame);
            blur_view.setAlpha(alpha);
            view.sendSubviewToBack(blur_view);
            return;
        }

        let blur_view =
            UIVisualEffectView::initWithEffect(self.mtm.alloc(), Some(effect.as_super()));
        let blur_as_view = blur_view.as_super();
        blur_as_view.setFrame(frame);
        blur_as_view.setAlpha(alpha);
        blur_as_view.setUserInteractionEnabled(false);
        view.insertSubview_atIndex(blur_as_view, 0);
        self.blur_views.insert(tag, blur_view);
    }

    fn update_blur_frame(&self, id: WidgetId, frame: CGRect) {
        if let Some(blur_view) = self.blur_views.get(&id.to_u64()) {
            blur_view.as_super().setFrame(CGRect {
                origin: CGPoint { x: 0.0, y: 0.0 },
                size: frame.size,
            });
        }
    }

    fn clear_blur(&mut self, id: WidgetId) {
        if let Some(blur_view) = self.blur_views.remove(&id.to_u64()) {
            blur_view.as_super().removeFromSuperview();
        }
    }

    fn install_accessibility_actions(&mut self, id: WidgetId, view: &UIView, actions: Vec<String>) {
        let tag = id.to_u64();
        clear_accessibility_action_payloads(tag);

        if actions.is_empty() {
            unsafe {
                let _: () = msg_send![
                    &*view,
                    setAccessibilityCustomActions: std::ptr::null::<AnyObject>()
                ];
            }
            self.accessibility_action_lists.remove(&tag);
            return;
        }

        let native_actions = NSMutableArray::<UIAccessibilityCustomAction>::new();
        for action_name in actions {
            if action_name.is_empty() {
                continue;
            }
            let ns = NSString::from_str(&action_name);
            let action: *mut UIAccessibilityCustomAction = unsafe {
                let allocated: *mut AnyObject =
                    msg_send![class!(UIAccessibilityCustomAction), alloc];
                let initialized: *mut AnyObject = msg_send![
                    allocated,
                    initWithName: &*ns
                    target: &*self.action_target
                    selector: sel!(accessibilityAction:)
                ];
                initialized.cast()
            };
            let Some(action) = (unsafe { Retained::from_raw(action) }) else {
                continue;
            };
            register_accessibility_action_payload(&action, tag, action_name);
            native_actions.addObject(&action);
        }

        if native_actions.is_empty() {
            unsafe {
                let _: () = msg_send![
                    &*view,
                    setAccessibilityCustomActions: std::ptr::null::<AnyObject>()
                ];
            }
            self.accessibility_action_lists.remove(&tag);
            return;
        }

        unsafe {
            let _: () = msg_send![&*view, setIsAccessibilityElement: true];
            let _: () = msg_send![&*view, setAccessibilityCustomActions: &*native_actions];
        }
        self.accessibility_action_lists.insert(tag, native_actions);
    }

    fn clear_accessibility_actions(&mut self, id: WidgetId) {
        let tag = id.to_u64();
        if let Some(view) = self.views.get(&tag) {
            unsafe {
                let _: () = msg_send![
                    &**view,
                    setAccessibilityCustomActions: std::ptr::null::<AnyObject>()
                ];
            }
        }
        self.accessibility_action_lists.remove(&tag);
        clear_accessibility_action_payloads(tag);
    }

    fn text_field_accessory_view(&self, text: &str) -> Retained<UIView> {
        let width = (text.chars().count() as f64 * 8.5 + 12.0).max(20.0);
        let frame = CGRect {
            origin: CGPoint { x: 0.0, y: 0.0 },
            size: CGSize {
                width,
                height: 34.0,
            },
        };
        let label = unsafe { UILabel::initWithFrame(self.mtm.alloc(), frame) };
        let ns = NSString::from_str(text);
        unsafe {
            label.setText(Some(&ns));
            label.setTextAlignment(NSTextAlignment::Center);
            label.setFont(Some(&UIFont::systemFontOfSize(15.0)));
            label.setTextColor(Some(&to_ui_color(Color::rgb(107, 114, 128))));
        }
        label.into_super()
    }
}

fn to_cg_rect(rect: Rect) -> CGRect {
    CGRect {
        origin: CGPoint {
            x: rect.origin.x as f64,
            y: rect.origin.y as f64,
        },
        size: CGSize {
            width: rect.size.width as f64,
            height: rect.size.height as f64,
        },
    }
}

fn to_ui_color(c: Color) -> Retained<UIColor> {
    unsafe {
        UIColor::colorWithRed_green_blue_alpha(
            c.r as f64 / 255.0,
            c.g as f64 / 255.0,
            c.b as f64 / 255.0,
            c.a as f64 / 255.0,
        )
    }
}

fn to_cg_color(c: Color) -> Retained<objc2_core_graphics::CGColor> {
    unsafe { to_ui_color(c).CGColor() }
}

fn cg_rect(x: f32, y: f32, w: f32, h: f32) -> CGRect {
    CGRect {
        origin: CGPoint {
            x: x as f64,
            y: y as f64,
        },
        size: CGSize {
            width: w as f64,
            height: h as f64,
        },
    }
}

/// Renders a [`DrawCmd`] list into `view`'s layer, replacing any previous
/// canvas content. Rects/circles map to plain `CALayer`s (crisp rounded
/// corners), lines/paths to `CAShapeLayer`s, and text to a `CATextLayer`.
/// Coordinates are the canvas's local space (origin top-left).
fn render_canvas(view: &UIView, cmds: &[DrawCmd]) {
    use objc2_core_graphics::CGMutablePath;

    let host = view.layer();
    let nil = std::ptr::null::<AnyObject>();
    // Canvas views hold only canvas content, so clear all sublayers first.
    unsafe {
        let _: () = msg_send![&*host, setSublayers: nil];
    }

    unsafe {
        for cmd in cmds {
            match cmd {
                DrawCmd::Rect {
                    x,
                    y,
                    w,
                    h,
                    radius,
                    fill,
                    stroke,
                } => {
                    let layer = CALayer::new();
                    let _: () = msg_send![&*layer, setFrame: cg_rect(*x, *y, *w, *h)];
                    if let Some(f) = fill {
                        let cg = to_cg_color(*f);
                        let _: () = msg_send![&*layer, setBackgroundColor: &*cg];
                    }
                    if *radius > 0.0 {
                        let _: () = msg_send![&*layer, setCornerRadius: *radius as f64];
                    }
                    if let Some(s) = stroke {
                        let cg = to_cg_color(s.color);
                        let _: () = msg_send![&*layer, setBorderWidth: s.width as f64];
                        let _: () = msg_send![&*layer, setBorderColor: &*cg];
                    }
                    let _: () = msg_send![&*host, addSublayer: &*layer];
                }
                DrawCmd::Circle {
                    cx,
                    cy,
                    r,
                    fill,
                    stroke,
                } => {
                    let layer = CALayer::new();
                    let _: () =
                        msg_send![&*layer, setFrame: cg_rect(cx - r, cy - r, r * 2.0, r * 2.0)];
                    let _: () = msg_send![&*layer, setCornerRadius: *r as f64];
                    if let Some(f) = fill {
                        let cg = to_cg_color(*f);
                        let _: () = msg_send![&*layer, setBackgroundColor: &*cg];
                    }
                    if let Some(s) = stroke {
                        let cg = to_cg_color(s.color);
                        let _: () = msg_send![&*layer, setBorderWidth: s.width as f64];
                        let _: () = msg_send![&*layer, setBorderColor: &*cg];
                    }
                    let _: () = msg_send![&*host, addSublayer: &*layer];
                }
                DrawCmd::Line {
                    x1,
                    y1,
                    x2,
                    y2,
                    width,
                    color,
                } => {
                    let path = CGMutablePath::new();
                    CGMutablePath::move_to_point(
                        Some(&path),
                        std::ptr::null(),
                        *x1 as f64,
                        *y1 as f64,
                    );
                    CGMutablePath::add_line_to_point(
                        Some(&path),
                        std::ptr::null(),
                        *x2 as f64,
                        *y2 as f64,
                    );
                    let sl = CAShapeLayer::new();
                    let cg = to_cg_color(*color);
                    let _: () = msg_send![&*sl, setPath: &*path];
                    let _: () = msg_send![&*sl, setStrokeColor: &*cg];
                    let _: () = msg_send![&*sl, setFillColor: nil];
                    let _: () = msg_send![&*sl, setLineWidth: *width as f64];
                    let _: () = msg_send![&*host, addSublayer: &*sl];
                }
                DrawCmd::Path {
                    points,
                    closed,
                    fill,
                    stroke,
                } => {
                    if points.is_empty() {
                        continue;
                    }
                    let path = CGMutablePath::new();
                    let (x0, y0) = points[0];
                    CGMutablePath::move_to_point(
                        Some(&path),
                        std::ptr::null(),
                        x0 as f64,
                        y0 as f64,
                    );
                    for &(x, y) in &points[1..] {
                        CGMutablePath::add_line_to_point(
                            Some(&path),
                            std::ptr::null(),
                            x as f64,
                            y as f64,
                        );
                    }
                    if *closed {
                        CGMutablePath::close_subpath(Some(&path));
                    }
                    let sl = CAShapeLayer::new();
                    let _: () = msg_send![&*sl, setPath: &*path];
                    match fill {
                        Some(f) => {
                            let cg = to_cg_color(*f);
                            let _: () = msg_send![&*sl, setFillColor: &*cg];
                        }
                        None => {
                            let _: () = msg_send![&*sl, setFillColor: nil];
                        }
                    }
                    if let Some(s) = stroke {
                        let cg = to_cg_color(s.color);
                        let _: () = msg_send![&*sl, setStrokeColor: &*cg];
                        let _: () = msg_send![&*sl, setLineWidth: s.width as f64];
                    } else {
                        let _: () = msg_send![&*sl, setStrokeColor: nil];
                    }
                    let _: () = msg_send![&*host, addSublayer: &*sl];
                }
                DrawCmd::Text {
                    x,
                    y,
                    text,
                    size,
                    color,
                    align,
                } => {
                    let tl = CATextLayer::new();
                    let ns = NSString::from_str(text);
                    let cg = to_cg_color(*color);
                    let _: () = msg_send![&*tl, setString: &*ns];
                    let _: () = msg_send![&*tl, setForegroundColor: &*cg];
                    let _: () = msg_send![&*tl, setFontSize: *size as f64];
                    let _: () = msg_send![&*tl, setContentsScale: 2.0f64];
                    let box_w = (text.chars().count() as f32 * size * 0.62).max(*size);
                    let _: () = msg_send![&*tl, setFrame: cg_rect(*x, *y, box_w, size * 1.3)];
                    let mode = match align {
                        crate::dom::TextAlign::Start => "left",
                        crate::dom::TextAlign::Center => "center",
                        crate::dom::TextAlign::End => "right",
                    };
                    let m = NSString::from_str(mode);
                    let _: () = msg_send![&*tl, setAlignmentMode: &*m];
                    let _: () = msg_send![&*host, addSublayer: &*tl];
                }
            }
        }
    }
}

thread_local! {
    /// Context-menu items keyed by widget id, populated by
    /// `install_context_menu` and read when a long-press fires.
    static CONTEXT_MENUS: RefCell<HashMap<u64, Vec<MenuItem>>> = RefCell::new(HashMap::new());
}

/// Presents the context menu registered for `tag` as a native action sheet.
fn present_context_menu(tag: u64) {
    let items = CONTEXT_MENUS.with(|m| m.borrow().get(&tag).cloned());
    let Some(items) = items else {
        return;
    };
    if items.is_empty() {
        return;
    }
    unsafe {
        let nil = std::ptr::null::<AnyObject>();
        // UIAlertControllerStyleActionSheet == 1.
        let ac: *mut AnyObject = msg_send![
            class!(UIAlertController),
            alertControllerWithTitle: nil,
            message: nil,
            preferredStyle: 1isize,
        ];
        if ac.is_null() {
            return;
        }
        for item in &items {
            let title = NSString::from_str(&item.title);
            // UIAlertActionStyleDefault == 0, Destructive == 2.
            let style: isize = if item.destructive { 2 } else { 0 };
            let action = item.action.clone();
            let handler = RcBlock::new(move |_action: *mut AnyObject| {
                action();
            });
            let alert_action: *mut AnyObject = msg_send![
                class!(UIAlertAction),
                actionWithTitle: &*title,
                style: style,
                handler: &*handler,
            ];
            let _: () = msg_send![ac, addAction: alert_action];
        }
        // A trailing Cancel (UIAlertActionStyleCancel == 1) to dismiss.
        let cancel_title = NSString::from_str("Cancel");
        let cancel: *mut AnyObject = msg_send![
            class!(UIAlertAction),
            actionWithTitle: &*cancel_title,
            style: 1isize,
            handler: nil,
        ];
        let _: () = msg_send![ac, addAction: cancel];

        STATE.with(|s| {
            if let Some(state) = s.borrow().as_ref() {
                let vc: &UIViewController = &state._view_controller;
                // On iPad an action sheet needs a popover anchor; anchor it to
                // the presenting controller's view to avoid an exception.
                let pop: *mut AnyObject = msg_send![ac, popoverPresentationController];
                if !pop.is_null() {
                    let view: Retained<UIView> = msg_send![vc, view];
                    let _: () = msg_send![pop, setSourceView: &*view];
                    let bounds: CGRect = msg_send![&*view, bounds];
                    let _: () = msg_send![pop, setSourceRect: bounds];
                }
                let _: () =
                    msg_send![vc, presentViewController: ac, animated: true, completion: nil];
            }
        });
    }
}

impl Backend for UiKitBackend {
    fn apply(&mut self, mutation: Mutation) {
        match mutation {
            Mutation::Create { id, kind } => {
                let zero = CGRect {
                    origin: CGPoint { x: 0.0, y: 0.0 },
                    size: CGSize {
                        width: 0.0,
                        height: 0.0,
                    },
                };
                let view: Retained<UIView> = match kind {
                    WidgetKind::View | WidgetKind::Stack => unsafe {
                        UIView::initWithFrame(self.mtm.alloc(), zero)
                    },
                    WidgetKind::Text => {
                        let label: Retained<UILabel> =
                            unsafe { UILabel::initWithFrame(self.mtm.alloc(), zero) };
                        label.into_super()
                    }
                    WidgetKind::Button => {
                        let button =
                            unsafe { UIButton::buttonWithType(UIButtonType::System, self.mtm) };
                        unsafe {
                            button.addTarget_action_forControlEvents(
                                Some(&self.action_target),
                                sel!(didTapButton:),
                                UIControlEvents::TouchUpInside,
                            );
                            button.setTag(id.to_u64() as isize);
                        }
                        button.into_super().into_super()
                    }
                    WidgetKind::Image => {
                        let iv: Retained<UIImageView> =
                            unsafe { UIImageView::initWithFrame(self.mtm.alloc(), zero) };
                        iv.into_super()
                    }
                    WidgetKind::Switch => {
                        let sw: Retained<UISwitch> =
                            unsafe { UISwitch::initWithFrame(self.mtm.alloc(), zero) };
                        unsafe {
                            sw.addTarget_action_forControlEvents(
                                Some(&self.action_target),
                                sel!(valueChanged:),
                                UIControlEvents::ValueChanged,
                            );
                            sw.setTag(id.to_u64() as isize);
                        }
                        sw.into_super().into_super()
                    }
                    WidgetKind::Slider => {
                        let sl: Retained<UISlider> =
                            unsafe { UISlider::initWithFrame(self.mtm.alloc(), zero) };
                        unsafe {
                            sl.addTarget_action_forControlEvents(
                                Some(&self.action_target),
                                sel!(valueChanged:),
                                UIControlEvents::ValueChanged,
                            );
                            sl.setTag(id.to_u64() as isize);
                        }
                        sl.into_super().into_super()
                    }
                    WidgetKind::Scroll => {
                        let sv: Retained<UIScrollView> =
                            unsafe { UIScrollView::initWithFrame(self.mtm.alloc(), zero) };
                        self.install_scroll_delegate(id, &sv);
                        sv.into_super()
                    }
                    WidgetKind::ActivityIndicator => {
                        let spinner: Retained<UIActivityIndicatorView> = unsafe {
                            UIActivityIndicatorView::initWithFrame(self.mtm.alloc(), zero)
                        };
                        unsafe { spinner.startAnimating() };
                        spinner.into_super()
                    }
                    WidgetKind::Progress => {
                        let bar: Retained<UIProgressView> =
                            unsafe { UIProgressView::initWithFrame(self.mtm.alloc(), zero) };
                        bar.into_super()
                    }
                    WidgetKind::Segmented => {
                        let seg: Retained<UISegmentedControl> =
                            unsafe { UISegmentedControl::initWithFrame(self.mtm.alloc(), zero) };
                        unsafe {
                            seg.addTarget_action_forControlEvents(
                                Some(&self.action_target),
                                sel!(valueChanged:),
                                UIControlEvents::ValueChanged,
                            );
                            seg.setTag(id.to_u64() as isize);
                        }
                        seg.into_super().into_super()
                    }
                    WidgetKind::Stepper => {
                        let st: Retained<UIStepper> =
                            unsafe { UIStepper::initWithFrame(self.mtm.alloc(), zero) };
                        unsafe {
                            st.addTarget_action_forControlEvents(
                                Some(&self.action_target),
                                sel!(valueChanged:),
                                UIControlEvents::ValueChanged,
                            );
                            st.setTag(id.to_u64() as isize);
                        }
                        st.into_super().into_super()
                    }
                    WidgetKind::DatePicker => {
                        let dp: Retained<UIDatePicker> =
                            unsafe { UIDatePicker::initWithFrame(self.mtm.alloc(), zero) };
                        unsafe {
                            dp.addTarget_action_forControlEvents(
                                Some(&self.action_target),
                                sel!(valueChanged:),
                                UIControlEvents::ValueChanged,
                            );
                            dp.setTag(id.to_u64() as isize);
                        }
                        dp.into_super().into_super()
                    }
                    WidgetKind::TextInput => {
                        let field: Retained<UITextField> =
                            unsafe { UITextField::initWithFrame(self.mtm.alloc(), zero) };
                        unsafe {
                            field.setBorderStyle(UITextBorderStyle::RoundedRect);
                            field.addTarget_action_forControlEvents(
                                Some(&self.action_target),
                                sel!(textChanged:),
                                UIControlEvents::EditingChanged,
                            );
                            field.setTag(id.to_u64() as isize);
                            // Set delegate for textFieldShouldReturn: (submit key)
                            let _: () = msg_send![&*field, setDelegate: &*self.action_target];
                        }
                        field.into_super().into_super()
                    }
                    WidgetKind::TextArea => {
                        let tv: Retained<UITextView> =
                            unsafe { UITextView::initWithFrame(self.mtm.alloc(), zero) };
                        unsafe {
                            tv.setTag(id.to_u64() as isize);
                            // Use msg_send to set delegate since UITextViewDelegate protocol binding
                            let _: () = msg_send![&*tv, setDelegate: &*self.action_target];
                        }
                        // UITextView -> UIScrollView -> UIView
                        tv.into_super().into_super()
                    }
                    WidgetKind::Camera => {
                        // Plain UIView as the preview container; AVCaptureSession is
                        // attached when QrScanning(true) is set on it.
                        let v = unsafe { UIView::initWithFrame(self.mtm.alloc(), zero) };
                        unsafe { v.setTag(id.to_u64() as isize) };
                        v
                    }
                    WidgetKind::LazyList => {
                        // A UIScrollView acting as a simple scrolling list container.
                        // True UITableView-backed recycling is planned as future work.
                        let sv: Retained<UIScrollView> =
                            unsafe { UIScrollView::initWithFrame(self.mtm.alloc(), zero) };
                        self.install_scroll_delegate(id, &sv);
                        sv.into_super()
                    }
                    WidgetKind::WebView => {
                        // Allocate WKWebView using raw msg_send! (avoids a WebKit crate dep).
                        // WKWebView -> UIScrollView -> UIView; we wrap as UIView.
                        unsafe {
                            let wv: *mut AnyObject = msg_send![class!(WKWebView), alloc];
                            let wv: *mut AnyObject = msg_send![wv, initWithFrame: zero
                                                                    configuration: std::ptr::null::<AnyObject>()];
                            if wv.is_null() {
                                // Fallback: plain UIView (shouldn't happen on a real device)
                                UIView::initWithFrame(self.mtm.alloc(), zero)
                            } else {
                                let wv_view: *mut UIView = wv as *mut UIView;
                                Retained::retain(wv_view)
                                    .expect("WKWebView init returned a valid object")
                            }
                        }
                    }
                    WidgetKind::MapView => unsafe {
                        let frame = CGRect {
                            origin: CGPoint { x: 0.0, y: 0.0 },
                            size: CGSize {
                                width: 100.0,
                                height: 100.0,
                            },
                        };
                        let mv: *mut AnyObject = msg_send![class!(MKMapView), alloc];
                        let mv: *mut AnyObject = msg_send![mv, initWithFrame: frame];
                        let mv_view: *mut UIView = mv as *mut UIView;
                        Retained::retain(mv_view).expect("MKMapView init failed")
                    },
                    WidgetKind::Canvas => {
                        // Plain UIView; the DrawList attribute populates its layer
                        // with CAShapeLayer/CALayer/CATextLayer content.
                        let v = unsafe { UIView::initWithFrame(self.mtm.alloc(), zero) };
                        unsafe { v.setTag(id.to_u64() as isize) };
                        v
                    }
                };
                self.views.insert(id.to_u64(), view);
            }
            Mutation::SetAttribute { id, attr } => {
                let Some(view) = self.view(id).cloned() else {
                    return;
                };
                match attr {
                    Attribute::Text(text) => {
                        let ns = NSString::from_str(&text);
                        if let Ok(label) = view.clone().downcast::<UILabel>() {
                            unsafe { label.setText(Some(&ns)) };
                        } else if let Ok(button) = view.clone().downcast::<UIButton>() {
                            unsafe { button.setTitle_forState(Some(&ns), UIControlState::Normal) };
                        } else if let Ok(field) = view.clone().downcast::<UITextField>() {
                            // Avoid clobbering the field mid-edit while focused.
                            let editing = unsafe { field.isFirstResponder() };
                            if !editing {
                                unsafe { field.setText(Some(&ns)) };
                            }
                        } else if let Ok(tv) = view.clone().downcast::<UITextView>() {
                            let editing = unsafe { tv.isFirstResponder() };
                            if !editing {
                                unsafe { tv.setText(Some(&ns)) };
                            }
                        }
                    }
                    Attribute::Placeholder(text) => {
                        if let Ok(field) = view.clone().downcast::<UITextField>() {
                            let ns = NSString::from_str(&text);
                            unsafe { field.setPlaceholder(Some(&ns)) };
                        }
                    }
                    Attribute::FontSize(size) => {
                        let font = unsafe { UIFont::systemFontOfSize(size as f64) };
                        if let Ok(label) = view.clone().downcast::<UILabel>() {
                            unsafe { label.setFont(Some(&font)) };
                        }
                    }
                    Attribute::FontWeight(weight) => {
                        if let Ok(label) = view.clone().downcast::<UILabel>() {
                            let size = unsafe { label.font() }
                                .map(|f| unsafe { f.pointSize() })
                                .unwrap_or(17.0);
                            // Map 100..900 onto UIFontWeight (-0.8..0.62).
                            let w = ((weight - 400.0) / 300.0) as f64 * 0.4;
                            let font = unsafe { UIFont::systemFontOfSize_weight(size, w) };
                            unsafe { label.setFont(Some(&font)) };
                        }
                    }
                    Attribute::Italic(italic) => {
                        if italic {
                            if let Ok(label) = view.clone().downcast::<UILabel>() {
                                let size = unsafe { label.font() }
                                    .map(|f| unsafe { f.pointSize() })
                                    .unwrap_or(17.0);
                                let font = unsafe { UIFont::italicSystemFontOfSize(size) };
                                unsafe { label.setFont(Some(&font)) };
                            }
                        }
                    }
                    Attribute::FontFamily(family) => {
                        if let Ok(label) = view.clone().downcast::<UILabel>() {
                            let size = unsafe { label.font() }
                                .map(|f| unsafe { f.pointSize() })
                                .unwrap_or(17.0);
                            let name = NSString::from_str(&family);
                            // If the named font exists, use it; otherwise fall back to
                            // the system font at the same size so text is never invisible.
                            let font = unsafe { UIFont::fontWithName_size(&name, size) }
                                .unwrap_or_else(|| unsafe { UIFont::systemFontOfSize(size) });
                            unsafe { label.setFont(Some(&font)) };
                        }
                    }
                    Attribute::TextAlign(align) => {
                        if let Ok(label) = view.clone().downcast::<UILabel>() {
                            let a = match align {
                                crate::dom::TextAlign::Start => NSTextAlignment::Natural,
                                crate::dom::TextAlign::Center => NSTextAlignment::Center,
                                crate::dom::TextAlign::End => NSTextAlignment::Right,
                            };
                            unsafe { label.setTextAlignment(a) };
                        }
                    }
                    Attribute::TextColor(color) => {
                        let c = to_ui_color(color);
                        if let Ok(label) = view.clone().downcast::<UILabel>() {
                            unsafe { label.setTextColor(Some(&c)) };
                        } else if let Ok(button) = view.clone().downcast::<UIButton>() {
                            unsafe {
                                button.setTitleColor_forState(Some(&c), UIControlState::Normal)
                            };
                        } else if let Ok(field) = view.clone().downcast::<UITextField>() {
                            unsafe { field.setTextColor(Some(&c)) };
                        } else if let Ok(tv) = view.clone().downcast::<UITextView>() {
                            unsafe { tv.setTextColor(Some(&c)) };
                        }
                    }
                    Attribute::BackgroundColor(color) => {
                        let c = to_ui_color(color);
                        unsafe { view.setBackgroundColor(Some(&c)) };
                    }
                    Attribute::CornerRadius(radius) => {
                        let layer = view.layer();
                        unsafe {
                            layer.setCornerRadius(radius as f64);
                            layer.setMasksToBounds(true);
                        }
                    }
                    Attribute::Opacity(o) => unsafe { view.setAlpha(o as f64) },
                    Attribute::BorderWidth(w) => {
                        let layer = view.layer();
                        unsafe { layer.setBorderWidth(w as f64) };
                    }
                    Attribute::BorderColor(color) => {
                        let layer = view.layer();
                        let cg = unsafe { to_ui_color(color).CGColor() };
                        unsafe { layer.setBorderColor(Some(&cg)) };
                    }
                    Attribute::ImageSource(name) => {
                        if let Ok(image_view) = view.clone().downcast::<UIImageView>() {
                            let ns = NSString::from_str(&name);
                            if let Some(img) = unsafe { UIImage::systemImageNamed(&ns) }
                                .or_else(|| unsafe { UIImage::imageNamed(&ns) })
                            {
                                unsafe { image_view.setImage(Some(&img)) };
                                mark_image_loaded(id.to_u64());
                            } else {
                                unsafe { image_view.setImage(None) };
                                mark_image_error(
                                    id.to_u64(),
                                    format!("image source not found: {name}"),
                                );
                            }
                        }
                    }
                    Attribute::BoolValue(on) => {
                        if let Ok(sw) = view.clone().downcast::<UISwitch>() {
                            unsafe { sw.setOn(on) };
                        }
                    }
                    Attribute::FloatValue(value) => {
                        if let Ok(sl) = view.clone().downcast::<UISlider>() {
                            unsafe { sl.setValue(value) };
                        } else if let Ok(bar) = view.clone().downcast::<UIProgressView>() {
                            unsafe { bar.setProgress(value) };
                        } else if let Ok(seg) = view.clone().downcast::<UISegmentedControl>() {
                            unsafe { seg.setSelectedSegmentIndex(value as isize) };
                        } else if let Ok(st) = view.clone().downcast::<UIStepper>() {
                            unsafe { st.setValue(value as f64) };
                        }
                    }
                    Attribute::DateValue(epoch_seconds) => {
                        if let Ok(dp) = view.clone().downcast::<UIDatePicker>() {
                            let date =
                                unsafe { NSDate::dateWithTimeIntervalSince1970(epoch_seconds) };
                            unsafe { dp.setDate(&date) };
                        }
                    }
                    Attribute::DatePickerMode(mode) => {
                        if let Ok(dp) = view.clone().downcast::<UIDatePicker>() {
                            let mode = match mode {
                                crate::dom::DatePickerMode::Date => UIDatePickerMode::Date,
                                crate::dom::DatePickerMode::Time => UIDatePickerMode::Time,
                                crate::dom::DatePickerMode::DateTime => {
                                    UIDatePickerMode::DateAndTime
                                }
                            };
                            unsafe { dp.setDatePickerMode(mode) };
                        }
                    }
                    Attribute::DatePickerStyle(style) => {
                        if let Ok(dp) = view.clone().downcast::<UIDatePicker>() {
                            let style = match style {
                                crate::dom::DatePickerStyle::Automatic => {
                                    UIDatePickerStyle::Automatic
                                }
                                crate::dom::DatePickerStyle::Wheels => UIDatePickerStyle::Wheels,
                                crate::dom::DatePickerStyle::Compact => UIDatePickerStyle::Compact,
                                crate::dom::DatePickerStyle::Inline => UIDatePickerStyle::Inline,
                            };
                            unsafe { dp.setPreferredDatePickerStyle(style) };
                        }
                    }
                    Attribute::DateMin(epoch_seconds) => {
                        if let Ok(dp) = view.clone().downcast::<UIDatePicker>() {
                            let date =
                                unsafe { NSDate::dateWithTimeIntervalSince1970(epoch_seconds) };
                            unsafe { dp.setMinimumDate(Some(&date)) };
                        }
                    }
                    Attribute::DateMax(epoch_seconds) => {
                        if let Ok(dp) = view.clone().downcast::<UIDatePicker>() {
                            let date =
                                unsafe { NSDate::dateWithTimeIntervalSince1970(epoch_seconds) };
                            unsafe { dp.setMaximumDate(Some(&date)) };
                        }
                    }
                    Attribute::Range { min, max, step } => {
                        if let Ok(st) = view.clone().downcast::<UIStepper>() {
                            unsafe {
                                st.setMinimumValue(min as f64);
                                st.setMaximumValue(max as f64);
                                st.setStepValue(step as f64);
                            }
                        }
                    }
                    Attribute::Items(items) => {
                        if let Ok(seg) = view.clone().downcast::<UISegmentedControl>() {
                            unsafe { seg.removeAllSegments() };
                            for (i, title) in items.iter().enumerate() {
                                let ns = NSString::from_str(title);
                                unsafe {
                                    seg.insertSegmentWithTitle_atIndex_animated(
                                        Some(&ns),
                                        i,
                                        false,
                                    );
                                }
                            }
                        }
                    }
                    Attribute::TintColor(color) => {
                        unsafe { view.setTintColor(Some(&to_ui_color(color))) };
                    }
                    Attribute::AccessibilityLabel(label) => {
                        let ns = NSString::from_str(&label);
                        unsafe {
                            let _: () = msg_send![&*view, setIsAccessibilityElement: true];
                            let _: () = msg_send![&*view, setAccessibilityLabel: &*ns];
                        }
                    }
                    Attribute::AccessibilityRole(role) => {
                        // UIAccessibilityTraits bits (UIAccessibilityConstants.h).
                        let traits: i64 = match role {
                            crate::dom::Role::None => 0,
                            crate::dom::Role::Button => 1 << 0,
                            crate::dom::Role::Link => 1 << 1,
                            crate::dom::Role::Image => 1 << 2,
                            crate::dom::Role::Search => 1 << 10,
                            crate::dom::Role::Adjustable => 1 << 12,
                            crate::dom::Role::Header => 1 << 15,
                        };
                        unsafe {
                            let _: () = msg_send![&*view, setIsAccessibilityElement: true];
                            let _: () = msg_send![&*view, setAccessibilityTraits: traits];
                        }
                    }
                    Attribute::Shadow(shadow) => {
                        let layer = view.layer();
                        let cg = unsafe { to_ui_color(shadow.color).CGColor() };
                        unsafe {
                            layer.setShadowColor(Some(&cg));
                            layer.setShadowRadius(shadow.radius as f64);
                            layer.setShadowOffset(CGSize {
                                width: shadow.dx as f64,
                                height: shadow.dy as f64,
                            });
                            layer.setShadowOpacity(shadow.color.a as f32 / 255.0);
                            layer.setMasksToBounds(false);
                        }
                    }
                    Attribute::Transform(t) => {
                        // Compose scale → rotate → translate into one matrix.
                        let (sin, cos) = (t.rotate.sin() as f64, t.rotate.cos() as f64);
                        let (sx, sy) = (t.scale_x as f64, t.scale_y as f64);
                        let m = CGAffineTransform {
                            a: sx * cos,
                            b: sx * sin,
                            c: -sy * sin,
                            d: sy * cos,
                            tx: t.translate_x as f64,
                            ty: t.translate_y as f64,
                        };
                        unsafe { view.setTransform(m) };
                    }
                    Attribute::Gradient(g) => {
                        // Reuse an existing gradient layer for this id, else make
                        // one and insert it beneath the view's content.
                        let key = id.to_u64();
                        let layer = self.gradient_layers.entry(key).or_insert_with(|| {
                            let gl = unsafe { CAGradientLayer::new() };
                            view.layer().insertSublayer_atIndex(&gl, 0);
                            gl
                        });
                        let colors = NSMutableArray::<AnyObject>::new();
                        for c in &g.colors {
                            let cg = unsafe { to_ui_color(*c).CGColor() };
                            // A CGColorRef is a valid `id` for an NSArray.
                            let obj: &AnyObject =
                                unsafe { &*((&*cg as *const objc2_core_graphics::CGColor).cast()) };
                            colors.addObject(obj);
                        }
                        unsafe { layer.setColors(Some(&colors)) };
                        layer.setStartPoint(CGPoint {
                            x: g.start.0 as f64,
                            y: g.start.1 as f64,
                        });
                        layer.setEndPoint(CGPoint {
                            x: g.end.0 as f64,
                            y: g.end.1 as f64,
                        });
                        layer.setFrame(view.bounds());
                    }
                    Attribute::DrawList(cmds) => {
                        render_canvas(&view, &cmds);
                    }
                    Attribute::ContextMenu(items) => {
                        self.install_context_menu(id, &view, items);
                    }
                    Attribute::NumberOfLines(n) => {
                        if let Ok(label) = view.clone().downcast::<UILabel>() {
                            unsafe {
                                label.setNumberOfLines(n as isize);
                            }
                        }
                    }
                    Attribute::ImageData(bytes) => {
                        if let Ok(image_view) = view.clone().downcast::<UIImageView>() {
                            let data = unsafe {
                                NSData::initWithBytes_length(
                                    self.mtm.alloc(),
                                    bytes.as_ptr() as *const std::ffi::c_void,
                                    bytes.len(),
                                )
                            };
                            if let Some(img) = unsafe { UIImage::imageWithData(&data) } {
                                unsafe { image_view.setImage(Some(&img)) };
                                mark_image_loaded(id.to_u64());
                            } else {
                                unsafe { image_view.setImage(None) };
                                mark_image_error(id.to_u64(), "image data could not be decoded");
                            }
                        }
                    }
                    Attribute::ImageResizeMode(mode) => {
                        // UIViewContentMode raw values:
                        //   ScaleToFill = 0, ScaleAspectFit = 2, ScaleAspectFill = 1,
                        //   Center = 4
                        let content_mode: isize = match mode {
                            crate::dom::ImageResizeMode::Stretch => 0,
                            crate::dom::ImageResizeMode::Cover => 1,
                            crate::dom::ImageResizeMode::Contain => 2,
                            crate::dom::ImageResizeMode::Center => 4,
                            crate::dom::ImageResizeMode::Repeat => 0, // stub; tiling needs CALayer
                        };
                        unsafe {
                            let _: () = msg_send![&*view, setContentMode: content_mode];
                        }
                    }
                    Attribute::ImageOnLoad(cb) => {
                        set_image_on_load(id.to_u64(), cb);
                    }
                    Attribute::ImageOnError(cb) => {
                        set_image_on_error(id.to_u64(), cb);
                    }
                    Attribute::Horizontal(horiz) => {
                        if let Ok(sv) = view.clone().downcast::<UIScrollView>() {
                            unsafe {
                                sv.setAlwaysBounceHorizontal(horiz);
                                sv.setAlwaysBounceVertical(!horiz);
                                if horiz {
                                    sv.setShowsVerticalScrollIndicator(false);
                                    sv.setShowsHorizontalScrollIndicator(true);
                                } else {
                                    sv.setShowsVerticalScrollIndicator(true);
                                    sv.setShowsHorizontalScrollIndicator(false);
                                }
                            }
                        }
                    }
                    Attribute::Refreshing(refreshing) => {
                        if let Ok(sv) = view.clone().downcast::<UIScrollView>() {
                            unsafe {
                                let rc: *mut AnyObject = msg_send![&*sv, refreshControl];
                                if rc.is_null() {
                                    let new_rc: *mut AnyObject =
                                        msg_send![av_class("UIRefreshControl"), new];
                                    let _: () = msg_send![new_rc, addTarget: &*self.action_target
                                                                  action: sel!(handleRefresh:)
                                                       forControlEvents: UIControlEvents::ValueChanged.bits()];
                                    let _: () = msg_send![new_rc, setTag: id.to_u64() as isize];
                                    let _: () = msg_send![&*sv, setRefreshControl: new_rc];
                                }
                                let rc: *mut AnyObject = msg_send![&*sv, refreshControl];
                                if !rc.is_null() {
                                    if refreshing {
                                        let _: () = msg_send![rc, beginRefreshing];
                                    } else {
                                        let _: () = msg_send![rc, endRefreshing];
                                    }
                                }
                            }
                        }
                    }
                    Attribute::ScrollEnabled(enabled) => {
                        if let Ok(sv) = view.clone().downcast::<UIScrollView>() {
                            unsafe {
                                let _: () = msg_send![&*sv, setScrollEnabled: enabled];
                            }
                        }
                    }
                    Attribute::ShowsScrollIndicator(show) => {
                        if let Ok(sv) = view.clone().downcast::<UIScrollView>() {
                            unsafe {
                                let _: () =
                                    msg_send![&*sv, setShowsHorizontalScrollIndicator: show];
                                let _: () = msg_send![&*sv, setShowsVerticalScrollIndicator: show];
                            }
                        }
                    }
                    Attribute::PagingEnabled(enabled) => {
                        if let Ok(sv) = view.clone().downcast::<UIScrollView>() {
                            unsafe {
                                let _: () = msg_send![&*sv, setPagingEnabled: enabled];
                            }
                        }
                    }
                    Attribute::ContentInset {
                        top,
                        right,
                        bottom,
                        left,
                    } => {
                        if let Ok(sv) = view.clone().downcast::<UIScrollView>() {
                            let insets = UIEdgeInsets {
                                top: top as f64,
                                left: left as f64,
                                bottom: bottom as f64,
                                right: right as f64,
                            };
                            sv.setContentInset(insets);
                        }
                    }
                    Attribute::OnScrollChange(cb) => {
                        if let Ok(sv) = view.clone().downcast::<UIScrollView>() {
                            self.install_scroll_delegate(id, &sv);
                            update_scroll_handlers(id.to_u64(), |handlers| {
                                handlers.on_scroll = Some(cb);
                            });
                        }
                    }
                    Attribute::OnScrollBegin(cb) => {
                        if let Ok(sv) = view.clone().downcast::<UIScrollView>() {
                            self.install_scroll_delegate(id, &sv);
                            update_scroll_handlers(id.to_u64(), |handlers| {
                                handlers.on_begin = Some(cb);
                            });
                        }
                    }
                    Attribute::OnScrollEnd(cb) => {
                        if let Ok(sv) = view.clone().downcast::<UIScrollView>() {
                            self.install_scroll_delegate(id, &sv);
                            update_scroll_handlers(id.to_u64(), |handlers| {
                                handlers.on_end = Some(cb);
                            });
                        }
                    }
                    Attribute::KeyboardDismissMode(mode) => {
                        if let Ok(sv) = view.clone().downcast::<UIScrollView>() {
                            unsafe {
                                let v: isize = match mode {
                                    crate::dom::KeyboardDismissMode::None => 0,
                                    crate::dom::KeyboardDismissMode::OnDrag => 1,
                                    crate::dom::KeyboardDismissMode::Interactive => 2,
                                };
                                let _: () = msg_send![&*sv, setKeyboardDismissMode: v];
                            }
                        }
                    }
                    Attribute::ReturnKey(ret) => {
                        if let Ok(field) = view.clone().downcast::<UITextField>() {
                            let v: isize = match ret {
                                crate::dom::ReturnKeyType::Default => 0,
                                crate::dom::ReturnKeyType::Go => 1,
                                crate::dom::ReturnKeyType::Next => 4,
                                crate::dom::ReturnKeyType::Search => 8,
                                crate::dom::ReturnKeyType::Send => 9,
                                crate::dom::ReturnKeyType::Done => 9,
                            };
                            unsafe {
                                send_set_int(
                                    &*field as *const _ as *const AnyObject,
                                    sel!(setReturnKeyType:),
                                    v,
                                );
                            }
                        }
                    }
                    Attribute::Secure(secure) => {
                        if let Ok(field) = view.clone().downcast::<UITextField>() {
                            unsafe {
                                field.setSecureTextEntry(secure);
                                if secure {
                                    // Install a native eye reveal toggle as the rightView.
                                    let btn =
                                        UIButton::buttonWithType(UIButtonType::System, self.mtm);
                                    let ns = NSString::from_str("eye.slash");
                                    if let Some(img) = UIImage::systemImageNamed(&ns) {
                                        let _: () =
                                            msg_send![&*btn, setImage: &*img, forState: 0usize];
                                    }
                                    btn.addTarget_action_forControlEvents(
                                        Some(&self.action_target),
                                        sel!(togglePassword:),
                                        UIControlEvents::TouchUpInside,
                                    );
                                    let frame = CGRect {
                                        origin: CGPoint { x: 0.0, y: 0.0 },
                                        size: CGSize {
                                            width: 38.0,
                                            height: 24.0,
                                        },
                                    };
                                    let _: () = msg_send![&*btn, setFrame: frame];
                                    let gray: Retained<UIColor> = msg_send![
                                        class!(UIColor),
                                        colorWithRed: 0.6f64,
                                        green: 0.6f64,
                                        blue: 0.62f64,
                                        alpha: 1.0f64
                                    ];
                                    let _: () = msg_send![&*btn, setTintColor: &*gray];
                                    let _: () = msg_send![&*field, setRightView: &*btn];
                                    let _: () = msg_send![&*field, setRightViewMode: 3isize];
                                } else {
                                    let nilv: *const AnyObject = std::ptr::null();
                                    let _: () = msg_send![&*field, setRightView: nilv];
                                    let _: () = msg_send![&*field, setRightViewMode: 0isize];
                                }
                            }
                        }
                    }
                    Attribute::QrScanning(enabled) => {
                        if enabled {
                            let view_ptr = &*view as *const UIView;
                            let tag = id.to_u64();
                            setup_qr_scanner(view_ptr, tag);
                        } else {
                            stop_qr_scanner(id.to_u64());
                        }
                    }
                    Attribute::RichText(spans) => {
                        if let Ok(label) = view.clone().downcast::<UILabel>() {
                            unsafe {
                                // Build NSMutableAttributedString by appending each span
                                let result: *mut AnyObject =
                                    msg_send![class!(NSMutableAttributedString), new];

                                for span in &spans {
                                    let ns_str = NSString::from_str(&span.text);

                                    // Build attributes dictionary
                                    let attrs: *mut AnyObject =
                                        msg_send![class!(NSMutableDictionary), new];

                                    // Font
                                    let font_size = span.font_size.unwrap_or(17.0) as f64;
                                    let font: *mut AnyObject = if span.bold && span.italic {
                                        msg_send![class!(UIFont), italicSystemFontOfSize: font_size]
                                    } else if span.bold {
                                        msg_send![class!(UIFont), boldSystemFontOfSize: font_size]
                                    } else {
                                        msg_send![class!(UIFont), systemFontOfSize: font_size]
                                    };
                                    let font_key = NSString::from_str("NSFont");
                                    let _: () =
                                        msg_send![attrs, setObject: font forKey: &*font_key];

                                    // Color
                                    if let Some(c) = span.color {
                                        let ui_color = to_ui_color(c);
                                        let color_key = NSString::from_str("NSColor");
                                        let _: () = msg_send![attrs, setObject: &*ui_color forKey: &*color_key];
                                    }

                                    // Underline
                                    if span.underline {
                                        let underline_key = NSString::from_str("NSUnderline");
                                        let underline_val: *mut AnyObject =
                                            msg_send![class!(NSNumber), numberWithInt: 1i32];
                                        let _: () = msg_send![attrs, setObject: underline_val forKey: &*underline_key];
                                    }

                                    // Strikethrough
                                    if span.strikethrough {
                                        let strike_key = NSString::from_str("NSStrikethrough");
                                        let strike_val: *mut AnyObject =
                                            msg_send![class!(NSNumber), numberWithInt: 1i32];
                                        let _: () = msg_send![attrs, setObject: strike_val forKey: &*strike_key];
                                    }

                                    // Letter spacing
                                    if let Some(kern) = span.letter_spacing {
                                        let kern_key = NSString::from_str("NSKern");
                                        let kern_val: *mut AnyObject = msg_send![class!(NSNumber), numberWithFloat: kern as f64];
                                        let _: () = msg_send![attrs, setObject: kern_val forKey: &*kern_key];
                                    }

                                    // Create attributed span and append
                                    let attr_str: *mut AnyObject =
                                        msg_send![class!(NSAttributedString), alloc];
                                    let attr_str: *mut AnyObject = msg_send![attr_str,
                                        initWithString: &*ns_str
                                        attributes: attrs
                                    ];
                                    let _: () = msg_send![result, appendAttributedString: attr_str];
                                }

                                let _: () = msg_send![&*label, setAttributedText: result];
                            }
                        }
                    }
                    Attribute::KeyboardType(kt) => {
                        // UIKeyboardType raw values (UITextInputTraits.h).
                        let ktype: isize = match kt {
                            KeyboardType::Default => 0,
                            KeyboardType::Ascii => 1,
                            KeyboardType::NumbersAndPunctuation => 2,
                            KeyboardType::Url => 3,
                            KeyboardType::NumberPad => 4,
                            KeyboardType::PhonePad => 5,
                            KeyboardType::NamePhonePad => 7,
                            KeyboardType::Email => 7, // UIKeyboardTypeEmailAddress = 7
                            KeyboardType::DecimalPad => 8,
                        };
                        if let Ok(field) = view.clone().downcast::<UITextField>() {
                            unsafe {
                                send_set_int(
                                    &*field as *const _ as *const AnyObject,
                                    sel!(setKeyboardType:),
                                    ktype,
                                );
                            }
                        } else if let Ok(tv) = view.clone().downcast::<UITextView>() {
                            unsafe {
                                send_set_int(
                                    &*tv as *const _ as *const AnyObject,
                                    sel!(setKeyboardType:),
                                    ktype,
                                );
                            }
                        }
                    }
                    Attribute::AccessibilityHint(hint) => {
                        let ns = NSString::from_str(&hint);
                        unsafe {
                            let _: () = msg_send![&*view, setIsAccessibilityElement: true];
                            let _: () = msg_send![&*view, setAccessibilityHint: &*ns];
                        }
                    }
                    Attribute::AccessibilityHidden(hidden) => unsafe {
                        let _: () = msg_send![&*view, setAccessibilityElementsHidden: hidden];
                    },
                    Attribute::Direction(dir) => {
                        // UISemanticContentAttribute:
                        //   UISemanticContentAttributeForceLeftToRight = 3
                        //   UISemanticContentAttributeForceRightToLeft = 4
                        let val: isize = match dir {
                            LayoutDirection::Ltr => 3,
                            LayoutDirection::Rtl => 4,
                        };
                        unsafe {
                            let _: () = msg_send![&*view, setSemanticContentAttribute: val];
                        }
                    }
                    Attribute::Url(url) => unsafe {
                        let ns_url_str = NSString::from_str(&url);
                        let ns_url: *mut AnyObject =
                            msg_send![class!(NSURL), URLWithString: &*ns_url_str];
                        if !ns_url.is_null() {
                            let request: *mut AnyObject =
                                msg_send![class!(NSURLRequest), requestWithURL: ns_url];
                            let _: () = msg_send![&*view, loadRequest: request];
                        }
                    },
                    Attribute::Html(html) => unsafe {
                        let ns_html = NSString::from_str(&html);
                        let base_url: *mut AnyObject = std::ptr::null_mut();
                        let _: () = msg_send![&*view, loadHTMLString: &*ns_html baseURL: base_url];
                    },
                    Attribute::TextStyle(style) => {
                        let style_name = match style {
                            crate::dom::TextStyle::LargeTitle => "UICTFontTextStyleLargeTitle",
                            crate::dom::TextStyle::Title1 => "UICTFontTextStyleTitle1",
                            crate::dom::TextStyle::Title2 => "UICTFontTextStyleTitle2",
                            crate::dom::TextStyle::Title3 => "UICTFontTextStyleTitle3",
                            crate::dom::TextStyle::Headline => "UIFontTextStyleHeadline",
                            crate::dom::TextStyle::Subheadline => "UIFontTextStyleSubheadline",
                            crate::dom::TextStyle::Body => "UIFontTextStyleBody",
                            crate::dom::TextStyle::Callout => "UIFontTextStyleCallout",
                            crate::dom::TextStyle::Footnote => "UIFontTextStyleFootnote",
                            crate::dom::TextStyle::Caption1 => "UIFontTextStyleCaption1",
                            crate::dom::TextStyle::Caption2 => "UIFontTextStyleCaption2",
                        };
                        if let Ok(label) = view.clone().downcast::<UILabel>() {
                            unsafe {
                                let ns_style = NSString::from_str(style_name);
                                let font: *mut AnyObject = msg_send![class!(UIFont), preferredFontForTextStyle: &*ns_style];
                                if !font.is_null() {
                                    let _: () = msg_send![&*label, setFont: font];
                                    // Enable dynamic type scaling with the font metrics.
                                    let _: () = msg_send![&*label, setAdjustsFontForContentSizeCategory: true];
                                }
                            }
                        }
                    }
                    Attribute::ItemCount(_count) => {
                        // For the UIScrollView-based LazyList, item count is handled
                        // at the view layer (lazy_column builds children directly).
                        // This attribute is a no-op on the iOS backend for now.
                    }
                    Attribute::EstimatedItemHeight(_height) => {
                        // Same as ItemCount: informational for a future UITableView backend.
                    }
                    Attribute::AnimateLayout(enabled) => {
                        // Track which views want layout animation.
                        let tag = unsafe { view.tag() } as u64;
                        ANIMATED_LAYOUT_VIEWS.with(|s| {
                            if enabled {
                                s.borrow_mut().insert(tag);
                            } else {
                                s.borrow_mut().remove(&tag);
                            }
                        });
                    }
                    Attribute::MapCenter {
                        latitude,
                        longitude,
                    } => unsafe {
                        #[repr(C)]
                        struct CLLocationCoordinate2D {
                            latitude: f64,
                            longitude: f64,
                        }
                        unsafe impl objc2::Encode for CLLocationCoordinate2D {
                            const ENCODING: objc2::encode::Encoding =
                                objc2::encode::Encoding::Struct(
                                    "CLLocationCoordinate2D",
                                    &[f64::ENCODING, f64::ENCODING],
                                );
                        }
                        let coord = CLLocationCoordinate2D {
                            latitude,
                            longitude,
                        };
                        let _: () = msg_send![&*view, setCenterCoordinate: coord animated: false];
                    },
                    Attribute::MapSpan { lat_span, lon_span } => unsafe {
                        #[repr(C)]
                        struct CLLocationCoordinate2D {
                            latitude: f64,
                            longitude: f64,
                        }
                        unsafe impl objc2::Encode for CLLocationCoordinate2D {
                            const ENCODING: objc2::encode::Encoding =
                                objc2::encode::Encoding::Struct(
                                    "CLLocationCoordinate2D",
                                    &[f64::ENCODING, f64::ENCODING],
                                );
                        }
                        #[repr(C)]
                        struct MKCoordinateSpan {
                            latitude_delta: f64,
                            longitude_delta: f64,
                        }
                        unsafe impl objc2::Encode for MKCoordinateSpan {
                            const ENCODING: objc2::encode::Encoding =
                                objc2::encode::Encoding::Struct(
                                    "MKCoordinateSpan",
                                    &[f64::ENCODING, f64::ENCODING],
                                );
                        }
                        #[repr(C)]
                        struct MKCoordinateRegion {
                            center: CLLocationCoordinate2D,
                            span: MKCoordinateSpan,
                        }
                        unsafe impl objc2::Encode for MKCoordinateRegion {
                            const ENCODING: objc2::encode::Encoding =
                                objc2::encode::Encoding::Struct(
                                    "MKCoordinateRegion",
                                    &[CLLocationCoordinate2D::ENCODING, MKCoordinateSpan::ENCODING],
                                );
                        }
                        let center: CLLocationCoordinate2D = msg_send![&*view, centerCoordinate];
                        let region = MKCoordinateRegion {
                            center,
                            span: MKCoordinateSpan {
                                latitude_delta: lat_span,
                                longitude_delta: lon_span,
                            },
                        };
                        let _: () = msg_send![&*view, setRegion: region animated: false];
                    },
                    Attribute::MapAnnotation {
                        annotation_id,
                        latitude,
                        longitude,
                        title,
                    } => {
                        unsafe {
                            #[repr(C)]
                            struct CLLocationCoordinate2D {
                                latitude: f64,
                                longitude: f64,
                            }
                            unsafe impl objc2::Encode for CLLocationCoordinate2D {
                                const ENCODING: objc2::encode::Encoding =
                                    objc2::encode::Encoding::Struct(
                                        "CLLocationCoordinate2D",
                                        &[f64::ENCODING, f64::ENCODING],
                                    );
                            }
                            let coord = CLLocationCoordinate2D {
                                latitude,
                                longitude,
                            };
                            let pin: *mut AnyObject = msg_send![class!(MKPointAnnotation), new];
                            let _: () = msg_send![pin, setCoordinate: coord];
                            let _: () = msg_send![pin, setTitle: &*NSString::from_str(&title)];
                            let _: () = msg_send![&*view, addAnnotation: pin];
                            let _ = annotation_id; // used as key for future update/remove
                        }
                    }
                    Attribute::AccessibilitySelected(selected) => {
                        unsafe {
                            // UIAccessibilityTraitSelected = 0x0000000000000020
                            let current_traits: u64 = msg_send![&*view, accessibilityTraits];
                            let trait_selected: u64 = 0x0000000000000020;
                            let new_traits = if selected {
                                current_traits | trait_selected
                            } else {
                                current_traits & !trait_selected
                            };
                            let _: () = msg_send![&*view, setAccessibilityTraits: new_traits];
                        }
                    }
                    Attribute::AccessibilityDisabled(disabled) => {
                        unsafe {
                            // UIAccessibilityTraitNotEnabled = 0x0000000000080000
                            let current_traits: u64 = msg_send![&*view, accessibilityTraits];
                            let trait_not_enabled: u64 = 0x0000000000080000;
                            let new_traits = if disabled {
                                current_traits | trait_not_enabled
                            } else {
                                current_traits & !trait_not_enabled
                            };
                            let _: () = msg_send![&*view, setAccessibilityTraits: new_traits];
                        }
                    }
                    Attribute::AccessibilityExpanded(expanded) => {
                        unsafe {
                            // UIAccessibilityTraitUpdatesFrequently not appropriate for expanded;
                            // set accessibilityValue to "expanded"/"collapsed" for VoiceOver
                            let val = if expanded { "expanded" } else { "collapsed" };
                            let ns = NSString::from_str(val);
                            let _: () = msg_send![&*view, setAccessibilityValue: &*ns];
                        }
                    }
                    Attribute::AccessibilityBusy(busy) => {
                        unsafe {
                            // UIAccessibilityTraitUpdatesFrequently = 0x0000000000040000
                            let current_traits: u64 = msg_send![&*view, accessibilityTraits];
                            let trait_busy: u64 = 0x0000000000040000;
                            let new_traits = if busy {
                                current_traits | trait_busy
                            } else {
                                current_traits & !trait_busy
                            };
                            let _: () = msg_send![&*view, setAccessibilityTraits: new_traits];
                        }
                    }
                    Attribute::HitSlop {
                        top,
                        right,
                        bottom,
                        left,
                    } => {
                        // UIView doesn't support hitSlop natively; we'd need a UIButton or a
                        // custom hit-test override. Store the values as associated object for
                        // subclasses to read via hitTest:withEvent:.
                        // For now set as a no-op with doc reference.
                        let _ = (top, right, bottom, left);
                    }
                    Attribute::LetterSpacing(kern) => {
                        // Letter spacing via NSKernAttributeName on UILabel's attributedText.
                        // We build an NSMutableAttributedString from the label's current text
                        // and apply kern to the full range.
                        if let Ok(label) = view.clone().downcast::<UILabel>() {
                            unsafe {
                                // Retrieve current plain text so we don't lose it.
                                let current_text: *mut AnyObject = msg_send![&*label, text];
                                let text_to_use: *mut AnyObject = if current_text.is_null() {
                                    let empty = NSString::from_str("");
                                    &*empty as *const _ as *mut AnyObject
                                } else {
                                    current_text
                                };

                                let len: usize = msg_send![text_to_use, length];
                                // NSRange { location: 0, length }
                                let full_range = NSRange::new(0, len);

                                let attr_str: *mut AnyObject =
                                    msg_send![class!(NSMutableAttributedString), alloc];
                                let attr_str: *mut AnyObject =
                                    msg_send![attr_str, initWithString: text_to_use];

                                let kern_key = NSString::from_str("NSKern");
                                let kern_val: *mut AnyObject =
                                    msg_send![class!(NSNumber), numberWithFloat: kern as f64];
                                let _: () = msg_send![attr_str,
                                    addAttribute: &*kern_key
                                    value: kern_val
                                    range: full_range
                                ];

                                let _: () = msg_send![&*label, setAttributedText: attr_str];
                            }
                        }
                    }
                    Attribute::LineHeight(h) => {
                        // Line height via NSParagraphStyle on UILabel's attributedText.
                        if let Ok(label) = view.clone().downcast::<UILabel>() {
                            unsafe {
                                let current_text: *mut AnyObject = msg_send![&*label, text];
                                let text_to_use: *mut AnyObject = if current_text.is_null() {
                                    let empty = NSString::from_str("");
                                    &*empty as *const _ as *mut AnyObject
                                } else {
                                    current_text
                                };

                                let len: usize = msg_send![text_to_use, length];
                                let full_range = NSRange::new(0, len);

                                let para_style: *mut AnyObject =
                                    msg_send![class!(NSMutableParagraphStyle), new];
                                // lineSpacing is additional points between lines; `h` is a
                                // multiplier so we leave it as an absolute value here.
                                // TODO: compute (h - 1.0) * fontSize for a true multiplier.
                                let _: () = msg_send![para_style, setLineSpacing: h as f64];

                                let attr_str: *mut AnyObject =
                                    msg_send![class!(NSMutableAttributedString), alloc];
                                let attr_str: *mut AnyObject =
                                    msg_send![attr_str, initWithString: text_to_use];

                                let para_key = NSString::from_str("NSParagraphStyle");
                                let _: () = msg_send![attr_str,
                                    addAttribute: &*para_key
                                    value: para_style
                                    range: full_range
                                ];

                                let _: () = msg_send![&*label, setAttributedText: attr_str];
                            }
                        }
                    }
                    Attribute::TextDecoration(decoration) => {
                        // Underline / strikethrough via NSAttributedString attribute keys.
                        if let Ok(label) = view.clone().downcast::<UILabel>() {
                            unsafe {
                                let current_text: *mut AnyObject = msg_send![&*label, text];
                                let text_to_use: *mut AnyObject = if current_text.is_null() {
                                    let empty = NSString::from_str("");
                                    &*empty as *const _ as *mut AnyObject
                                } else {
                                    current_text
                                };

                                let len: usize = msg_send![text_to_use, length];
                                let full_range = NSRange::new(0, len);

                                let attr_str: *mut AnyObject =
                                    msg_send![class!(NSMutableAttributedString), alloc];
                                let attr_str: *mut AnyObject =
                                    msg_send![attr_str, initWithString: text_to_use];

                                // NSUnderlineStyleNone = 0, NSUnderlineStyleSingle = 1,
                                // NSUnderlineStyleDouble = 9
                                let (underline_style, strike_style): (i32, i32) = match decoration {
                                    TextDecoration::None => (0, 0),
                                    TextDecoration::Underline => (1, 0),
                                    TextDecoration::Strikethrough => (0, 1),
                                    TextDecoration::UnderlineDouble => (9, 0),
                                };

                                let underline_key = NSString::from_str("NSUnderline");
                                let underline_val: *mut AnyObject =
                                    msg_send![class!(NSNumber), numberWithInt: underline_style];
                                let _: () = msg_send![attr_str,
                                    addAttribute: &*underline_key
                                    value: underline_val
                                    range: full_range
                                ];

                                let strike_key = NSString::from_str("NSStrikethrough");
                                let strike_val: *mut AnyObject =
                                    msg_send![class!(NSNumber), numberWithInt: strike_style];
                                let _: () = msg_send![attr_str,
                                    addAttribute: &*strike_key
                                    value: strike_val
                                    range: full_range
                                ];

                                let _: () = msg_send![&*label, setAttributedText: attr_str];
                            }
                        }
                    }
                    Attribute::OnPressIn(cb) => {
                        self.install_press_recognizer(id, &view);
                        update_press_handlers(id.to_u64(), |handlers| {
                            handlers.on_press_in = Some(cb);
                        });
                    }
                    Attribute::OnPressOut(cb) => {
                        self.install_press_recognizer(id, &view);
                        update_press_handlers(id.to_u64(), |handlers| {
                            handlers.on_press_out = Some(cb);
                        });
                    }
                    Attribute::Cursor(_style) => {
                        // Pointer cursor — no-op on touch-only iOS.
                    }
                    Attribute::OnSwipe { direction, handler } => {
                        self.install_swipe_recognizer(id, &view, direction);
                        set_swipe_handler(id.to_u64(), direction, handler);
                    }
                    Attribute::PlaceholderColor(color) => {
                        if let Ok(field) = view.clone().downcast::<UITextField>() {
                            unsafe {
                                // Build an NSAttributedString with the placeholder text and the
                                // requested color, then assign it to attributedPlaceholder.
                                let current_placeholder: *mut AnyObject =
                                    msg_send![&*field, placeholder];
                                let placeholder_str: *mut AnyObject =
                                    if current_placeholder.is_null() {
                                        let empty = NSString::from_str("");
                                        &*empty as *const _ as *mut AnyObject
                                    } else {
                                        current_placeholder
                                    };
                                let attrs: *mut AnyObject =
                                    msg_send![class!(NSMutableDictionary), new];
                                let ui_color = to_ui_color(color);
                                let color_key = NSString::from_str("NSColor");
                                let _: () =
                                    msg_send![attrs, setObject: &*ui_color forKey: &*color_key];
                                let attr_str: *mut AnyObject =
                                    msg_send![class!(NSAttributedString), alloc];
                                let attr_str: *mut AnyObject = msg_send![attr_str,
                                    initWithString: placeholder_str
                                    attributes: attrs
                                ];
                                let _: () = msg_send![&*field, setAttributedPlaceholder: attr_str];
                            }
                        }
                    }
                    Attribute::InputPrefix(text) => {
                        if let Ok(field) = view.clone().downcast::<UITextField>() {
                            if text.is_empty() {
                                field.setLeftView(None);
                                field.setLeftViewMode(UITextFieldViewMode::Never);
                            } else {
                                let accessory = self.text_field_accessory_view(&text);
                                field.setLeftView(Some(&accessory));
                                field.setLeftViewMode(UITextFieldViewMode::Always);
                            }
                        }
                    }
                    Attribute::InputSuffix(text) => {
                        if let Ok(field) = view.clone().downcast::<UITextField>() {
                            if text.is_empty() {
                                field.setRightView(None);
                                field.setRightViewMode(UITextFieldViewMode::Never);
                            } else {
                                let accessory = self.text_field_accessory_view(&text);
                                field.setRightView(Some(&accessory));
                                field.setRightViewMode(UITextFieldViewMode::Always);
                            }
                        }
                    }
                    Attribute::ClearButton(show) => {
                        if let Ok(field) = view.clone().downcast::<UITextField>() {
                            let mode = if show {
                                UITextFieldViewMode::WhileEditing
                            } else {
                                UITextFieldViewMode::Never
                            };
                            field.setClearButtonMode(mode);
                        }
                    }
                    Attribute::ReadOnly(read_only) => {
                        if let Ok(field) = view.clone().downcast::<UITextField>() {
                            unsafe {
                                let _: () = msg_send![&*field, setEnabled: !read_only];
                            }
                        } else if let Ok(tv) = view.clone().downcast::<UITextView>() {
                            unsafe {
                                let _: () = msg_send![&*tv, setEditable: !read_only];
                            }
                        }
                    }
                    Attribute::MaxLength(n) => {
                        if view.clone().downcast::<UITextField>().is_ok()
                            || view.clone().downcast::<UITextView>().is_ok()
                        {
                            set_text_max_length(id.to_u64(), n);
                        }
                    }
                    Attribute::TextShadow {
                        color,
                        offset_x,
                        offset_y,
                        blur,
                    } => {
                        // NSShadowAttributeName on UILabel's attributedText.
                        if let Ok(label) = view.clone().downcast::<UILabel>() {
                            unsafe {
                                let current_text: *mut AnyObject = msg_send![&*label, text];
                                let text_to_use: *mut AnyObject = if current_text.is_null() {
                                    let empty = NSString::from_str("");
                                    &*empty as *const _ as *mut AnyObject
                                } else {
                                    current_text
                                };

                                let len: usize = msg_send![text_to_use, length];
                                let full_range = NSRange::new(0, len);

                                let attr_str: *mut AnyObject =
                                    msg_send![class!(NSMutableAttributedString), alloc];
                                let attr_str: *mut AnyObject =
                                    msg_send![attr_str, initWithString: text_to_use];

                                // NSShadow object
                                let shadow: *mut AnyObject = msg_send![class!(NSShadow), new];
                                let ui_color = to_ui_color(color);
                                let _: () = msg_send![shadow, setShadowColor: &*ui_color];
                                let offset = CGSize {
                                    width: offset_x as f64,
                                    height: offset_y as f64,
                                };
                                let _: () = msg_send![shadow, setShadowOffset: offset];
                                let _: () = msg_send![shadow, setShadowBlurRadius: blur as f64];

                                let shadow_key = NSString::from_str("NSShadow");
                                let _: () = msg_send![attr_str,
                                    addAttribute: &*shadow_key
                                    value: shadow
                                    range: full_range
                                ];

                                let _: () = msg_send![&*label, setAttributedText: attr_str];
                            }
                        }
                    }
                    Attribute::StatusBarStyle(style) => {
                        // Set the status bar style via UIApplication on the main thread.
                        // This is a best-effort call; in apps that use per-VC preferred styles
                        // the view controller's `preferredStatusBarStyle` should be overridden.
                        unsafe {
                            let app: *mut AnyObject =
                                msg_send![class!(UIApplication), sharedApplication];
                            let style_val: i64 = match style {
                                crate::dom::StatusBarStyle::Dark => 3, // UIStatusBarStyleDarkContent
                                crate::dom::StatusBarStyle::Light => 1, // UIStatusBarStyleLightContent
                                crate::dom::StatusBarStyle::Auto => 0,  // UIStatusBarStyleDefault
                            };
                            let _: () =
                                msg_send![app, setStatusBarStyle: style_val animated: false];
                        }
                    }
                    Attribute::AspectRatio(_ratio) => {
                        // Aspect ratio is enforced via an NSLayoutConstraint on the native
                        // view. Since rax drives layout through its own flex pass and sets
                        // frames directly, we store the ratio as a hint for a future
                        // constraint-based layout path. For now this is a tracked no-op.
                    }
                    Attribute::BlurRadius(radius) => {
                        self.apply_blur(id, &view, radius);
                    }
                    Attribute::ClipToBounds(clip) => {
                        // clipsToBounds hides subviews that extend outside the view's frame.
                        unsafe {
                            let _: () = msg_send![&*view, setClipsToBounds: clip];
                        }
                    }
                    Attribute::ZIndex(z) => {
                        // CALayer.zPosition controls the rendering order within a superlayer.
                        unsafe {
                            let layer: *mut AnyObject = msg_send![&*view, layer];
                            let _: () = msg_send![layer, setZPosition: z as f64];
                        }
                    }
                    Attribute::FlexOrder(_) => {
                        // Flex order is handled by the layout engine; no native view property needed.
                    }
                    Attribute::UserSelectText(selectable) => unsafe {
                        let _: () = msg_send![&*view, setUserInteractionEnabled: selectable];
                    },
                    Attribute::ParagraphSpacing(spacing) => {
                        // TODO: apply NSParagraphStyle paragraphSpacing to UILabel/UITextView
                        let _ = spacing;
                    }
                    Attribute::FontStyle(style) => {
                        use crate::dom::FontStyle;
                        match style {
                            FontStyle::Italic | FontStyle::Oblique => {
                                // TODO: derive italic font from current font; for now no-op
                            }
                            FontStyle::Normal => {}
                        }
                    }
                    Attribute::AccessibilityGroup(is_group) => {
                        // When is_group is true the view is a container, not itself an element.
                        // isAccessibilityElement = false lets VoiceOver walk the children.
                        unsafe {
                            let _: () = msg_send![&*view, setIsAccessibilityElement: !is_group];
                        }
                    }
                    Attribute::AccessibilityHeadingLevel(level) => {
                        // UIAccessibilityTraitHeader = 0x10000
                        let header_trait: u64 = 0x10000;
                        unsafe {
                            let current_traits: u64 = msg_send![&*view, accessibilityTraits];
                            let new_traits = if level > 0 {
                                current_traits | header_trait
                            } else {
                                current_traits & !header_trait
                            };
                            let _: () = msg_send![&*view, setIsAccessibilityElement: true];
                            let _: () = msg_send![&*view, setAccessibilityTraits: new_traits];
                        }
                    }
                    Attribute::AccessibilityActions(actions) => {
                        self.install_accessibility_actions(id, &view, actions);
                    }
                    Attribute::DynamicType(dt) => {
                        // adjustsFontForContentSizeCategory scales the font with Dynamic Type.
                        unsafe {
                            let _: () = msg_send![&*view, setAdjustsFontForContentSizeCategory: dt];
                        }
                    }
                    Attribute::AccessibilityValueString(value) => {
                        let ns = NSString::from_str(&value);
                        unsafe {
                            let _: () = msg_send![&*view, setIsAccessibilityElement: true];
                            let _: () = msg_send![&*view, setAccessibilityValue: &*ns];
                        }
                    }
                }
            }
            Mutation::SetFrame { id, rect } => {
                if let Some(view) = self.view(id) {
                    let cg_rect = to_cg_rect(rect);
                    let animate = ANIMATED_LAYOUT_VIEWS.with(|s| s.borrow().contains(&id.to_u64()));
                    if animate {
                        unsafe {
                            let _: () = msg_send![class!(UIView), beginAnimations: std::ptr::null::<AnyObject>() context: std::ptr::null::<AnyObject>()];
                            let _: () = msg_send![class!(UIView), setAnimationDuration: 0.3f64];
                            view.setFrame(cg_rect);
                            let _: () = msg_send![class!(UIView), commitAnimations];
                        }
                    } else {
                        unsafe { view.setFrame(cg_rect) };
                    }
                    // If this is a Camera view, resize the AVCaptureVideoPreviewLayer
                    // to match the new bounds so the feed fills the container.
                    let has_session =
                        QR_SESSIONS.with(|map| map.borrow().contains_key(&id.to_u64()));
                    if has_session {
                        unsafe {
                            // The preview layer is always the last sublayer added.
                            let view_layer: *mut AnyObject = msg_send![&**view, layer];
                            let sublayers: *mut AnyObject = msg_send![view_layer, sublayers];
                            if !sublayers.is_null() {
                                let count: usize = msg_send![sublayers, count];
                                if count > 0 {
                                    let last: *mut AnyObject =
                                        msg_send![sublayers, objectAtIndex: count - 1];
                                    let new_bounds = CGRect {
                                        origin: CGPoint { x: 0.0, y: 0.0 },
                                        size: CGSize {
                                            width: rect.size.width as f64,
                                            height: rect.size.height as f64,
                                        },
                                    };
                                    let _: () = msg_send![last, setFrame: new_bounds];
                                }
                            }
                        }
                    }
                    self.update_blur_frame(id, cg_rect);
                }
                // Keep any gradient sublayer filling the view's new bounds.
                if let Some(layer) = self.gradient_layers.get(&id.to_u64()) {
                    layer.setFrame(CGRect {
                        origin: CGPoint { x: 0.0, y: 0.0 },
                        size: CGSize {
                            width: rect.size.width as f64,
                            height: rect.size.height as f64,
                        },
                    });
                }
            }
            Mutation::InsertChild { parent, child, .. } => {
                if let (Some(parent), Some(child)) =
                    (self.view(parent).cloned(), self.view(child).cloned())
                {
                    parent.addSubview(&child);
                }
            }
            Mutation::RemoveChild { child, .. } => {
                if let Some(view) = self.view(child) {
                    view.removeFromSuperview();
                }
            }
            Mutation::Destroy { id } => {
                // Stop any QR scanner session tied to this widget before
                // releasing the view so AVFoundation can clean up cleanly.
                stop_qr_scanner(id.to_u64());
                self.clear_scroll_delegate(id);
                self.clear_interaction_recognizers(id);
                clear_text_input_state(id.to_u64());
                clear_image_handlers(id.to_u64());
                self.clear_blur(id);
                self.clear_accessibility_actions(id);
                self.gradient_layers.remove(&id.to_u64());
                if let Some(view) = self.views.remove(&id.to_u64()) {
                    view.removeFromSuperview();
                }
            }
            Mutation::SetRoot { id } => {
                if let Some(view) = self.view(id).cloned() {
                    self.container.addSubview(&view);
                }
            }
            Mutation::AddGesture { id, gesture } => {
                let Some(view) = self.view(id).cloned() else {
                    return;
                };
                if gesture == GestureKind::Swipe {
                    return;
                }
                // Labels/images need interaction enabled to receive gestures.
                unsafe { view.setUserInteractionEnabled(true) };
                // Stamp the widget id onto the view's tag so `recognizer_tag`
                // can recover it on fire. Plain containers (WidgetKind::View)
                // never set a tag at creation, so without this every tap on a
                // container would route to WidgetId 0 — e.g. a tab bar built
                // from tappable `column`s would silently do nothing.
                unsafe { view.setTag(id.to_u64() as isize) };
                let recognizer: Retained<UIGestureRecognizer> = match gesture {
                    GestureKind::Tap => {
                        let r = unsafe {
                            UITapGestureRecognizer::initWithTarget_action(
                                self.mtm.alloc(),
                                Some(&self.action_target),
                                Some(sel!(tapRecognized:)),
                            )
                        };
                        unsafe {
                            let _: () = msg_send![&*r, setDelegate: &*self.action_target];
                        }
                        r.into_super()
                    }
                    GestureKind::DoubleTap => {
                        let r = unsafe {
                            UITapGestureRecognizer::initWithTarget_action(
                                self.mtm.alloc(),
                                Some(&self.action_target),
                                Some(sel!(doubleTapRecognized:)),
                            )
                        };
                        unsafe { r.setNumberOfTapsRequired(2) };
                        unsafe {
                            let _: () = msg_send![&*r, setDelegate: &*self.action_target];
                        }
                        r.into_super()
                    }
                    GestureKind::LongPress => {
                        let r = unsafe {
                            UILongPressGestureRecognizer::initWithTarget_action(
                                self.mtm.alloc(),
                                Some(&self.action_target),
                                Some(sel!(longPressRecognized:)),
                            )
                        };
                        unsafe {
                            let _: () = msg_send![&*r, setDelegate: &*self.action_target];
                        }
                        r.into_super()
                    }
                    GestureKind::Pan => {
                        let r = unsafe {
                            UIPanGestureRecognizer::initWithTarget_action(
                                self.mtm.alloc(),
                                Some(&self.action_target),
                                Some(sel!(panRecognized:)),
                            )
                        };
                        unsafe {
                            let _: () = msg_send![&*r, setDelegate: &*self.action_target];
                        }
                        r.into_super()
                    }
                    GestureKind::Pinch => {
                        let r = unsafe {
                            UIPinchGestureRecognizer::initWithTarget_action(
                                self.mtm.alloc(),
                                Some(&self.action_target),
                                Some(sel!(pinchRecognized:)),
                            )
                        };
                        unsafe {
                            let _: () = msg_send![&*r, setDelegate: &*self.action_target];
                        }
                        r.into_super()
                    }
                    GestureKind::Rotate => {
                        let r = unsafe {
                            UIRotationGestureRecognizer::initWithTarget_action(
                                self.mtm.alloc(),
                                Some(&self.action_target),
                                Some(sel!(rotateRecognized:)),
                            )
                        };
                        unsafe {
                            let _: () = msg_send![&*r, setDelegate: &*self.action_target];
                        }
                        r.into_super()
                    }
                    GestureKind::Swipe => unreachable!("swipe gestures are installed from OnSwipe"),
                };
                unsafe { view.addGestureRecognizer(&recognizer) };
            }
            Mutation::SetContentSize { id, size } => {
                if let Some(view) = self.view(id) {
                    if let Ok(sv) = view.clone().downcast::<UIScrollView>() {
                        unsafe {
                            sv.setContentSize(CGSize {
                                width: size.width as f64,
                                height: size.height as f64,
                            })
                        };
                    }
                }
            }
            Mutation::SetBackdrop { color } => {
                // The container fills the whole window (under the safe areas);
                // coloring it fills the notch/home-indicator region behind the
                // inset app content.
                unsafe { self.container.setBackgroundColor(Some(&to_ui_color(color))) };
            }
            Mutation::Haptic { style } => {
                unsafe {
                    match style {
                        HapticStyle::Selection => {
                            let gen: *mut AnyObject =
                                msg_send![class!(UISelectionFeedbackGenerator), new];
                            let _: () = msg_send![gen, selectionChanged];
                        }
                        HapticStyle::Success | HapticStyle::Warning | HapticStyle::Error => {
                            let v: isize = match style {
                                HapticStyle::Success => 0,
                                HapticStyle::Warning => 1,
                                _ => 2, // Error
                            };
                            let gen: *mut AnyObject =
                                msg_send![class!(UINotificationFeedbackGenerator), new];
                            let _: () = msg_send![gen, notificationOccurred: v];
                        }
                        _ => {
                            let v: isize = match style {
                                HapticStyle::Light => 0,
                                HapticStyle::Medium => 1,
                                _ => 2, // Heavy
                            };
                            let gen: *mut AnyObject =
                                msg_send![class!(UIImpactFeedbackGenerator), alloc];
                            let gen: *mut AnyObject = msg_send![gen, initWithStyle: v];
                            let _: () = msg_send![gen, impactOccurred];
                        }
                    }
                }
            }
            Mutation::ScheduleNotification(notif) => {
                unsafe {
                    // Get the shared notification center.
                    let center: *mut AnyObject =
                        msg_send![class!(UNUserNotificationCenter), currentNotificationCenter];

                    // Request authorization (sound | badge | alert = 0b111). Fire and forget.
                    // Pass null completion handler — we don't need the result.
                    let null_auth: *const block2::Block<
                        dyn Fn(objc2::runtime::Bool, *mut NSObject),
                    > = std::ptr::null();
                    let _: () = msg_send![
                        center,
                        requestAuthorizationWithOptions: 0b111usize
                        completionHandler: null_auth
                    ];

                    // Build mutable notification content.
                    let content: *mut AnyObject =
                        msg_send![class!(UNMutableNotificationContent), new];
                    let _: () = msg_send![content, setTitle: &*NSString::from_str(&notif.title)];
                    let _: () = msg_send![content, setBody: &*NSString::from_str(&notif.body)];
                    let default_sound: *mut AnyObject =
                        msg_send![class!(UNNotificationSound), defaultSound];
                    let _: () = msg_send![content, setSound: default_sound];

                    // Time interval trigger (minimum 1 second).
                    let delay = notif.delay_seconds.max(1) as f64;
                    let trigger: *mut AnyObject = msg_send![
                        class!(UNTimeIntervalNotificationTrigger),
                        triggerWithTimeInterval: delay
                        repeats: false
                    ];

                    // Build and add the request.
                    let id_ns = NSString::from_str(&notif.id);
                    let request: *mut AnyObject = msg_send![
                        class!(UNNotificationRequest),
                        requestWithIdentifier: &*id_ns
                        content: content
                        trigger: trigger
                    ];
                    // Pass null completion handler — we don't need the result.
                    let null_comp: *const block2::Block<dyn Fn(*mut NSObject)> = std::ptr::null();
                    let _: () = msg_send![
                        center,
                        addNotificationRequest: request
                        withCompletionHandler: null_comp
                    ];
                }
            }
            Mutation::CancelNotification { id } => unsafe {
                let center: *mut AnyObject =
                    msg_send![class!(UNUserNotificationCenter), currentNotificationCenter];
                let ids = NSMutableArray::<NSString>::new();
                ids.addObject(&NSString::from_str(&id));
                let _: () = msg_send![
                    center,
                    removePendingNotificationRequestsWithIdentifiers: &*ids
                ];
            },
            Mutation::StartLocation | Mutation::RequestLocation => {
                start_ios_location_updates();
            }
            Mutation::StopLocation | Mutation::StopLocationUpdates => {
                stop_ios_location_updates();
            }
            Mutation::StartMotion {
                accelerometer,
                gyroscope,
            } => {
                unsafe {
                    let mgr: *mut AnyObject = msg_send![class!(CMMotionManager), new];
                    if mgr.is_null() {
                        return;
                    }
                    // 60 Hz update interval
                    let interval: f64 = 1.0 / 60.0;
                    if accelerometer {
                        let _: () = msg_send![mgr, setAccelerometerUpdateInterval: interval];
                        let _: () = msg_send![mgr, startAccelerometerUpdates];
                    }
                    if gyroscope {
                        let _: () = msg_send![mgr, setGyroUpdateInterval: interval];
                        let _: () = msg_send![mgr, startGyroUpdates];
                    }
                    MOTION_MANAGER.with(|m| {
                        // Release any previous manager
                        if let Some(old) = m.borrow_mut().take() {
                            let _: () = msg_send![old, stopAccelerometerUpdates];
                            let _: () = msg_send![old, stopGyroUpdates];
                            objc_release(old);
                        }
                        *m.borrow_mut() = Some(mgr);
                    });
                }
            }
            Mutation::StopMotion => {
                MOTION_MANAGER.with(|m| {
                    if let Some(mgr) = m.borrow_mut().take() {
                        unsafe {
                            let _: () = msg_send![mgr, stopAccelerometerUpdates];
                            let _: () = msg_send![mgr, stopGyroUpdates];
                            objc_release(mgr);
                        }
                    }
                });
            }
            Mutation::PresentMediaPicker { max_selection } => {
                unsafe {
                    // PHPickerConfiguration
                    let config: *mut AnyObject = msg_send![class!(PHPickerConfiguration), new];
                    if config.is_null() {
                        // PHPhotosUI not linked — treat as cancelled.
                        PENDING_MEDIA_CANCEL.with(|c| c.set(true));
                        return;
                    }
                    let _: () = msg_send![config, setSelectionLimit: max_selection as isize];

                    let picker: *mut AnyObject = msg_send![class!(PHPickerViewController), alloc];
                    let picker: *mut AnyObject = msg_send![picker, initWithConfiguration: config];
                    if picker.is_null() {
                        PENDING_MEDIA_CANCEL.with(|c| c.set(true));
                        return;
                    }

                    // Create and store the delegate.
                    let delegate: Retained<MediaPickerDelegate> =
                        msg_send![MediaPickerDelegate::class(), new];
                    let _: () = msg_send![picker, setDelegate: &*delegate];
                    MEDIA_PICKER_DELEGATE.with(|d| *d.borrow_mut() = Some(delegate));

                    // Present using the root view controller (the UIViewController
                    // created in setup(), which owns the app's content view).
                    STATE.with(|s| {
                        if let Some(state) = s.borrow().as_ref() {
                            let vc: &UIViewController = &state._view_controller;
                            let _: () = msg_send![vc, presentViewController: picker animated: true completion: std::ptr::null::<AnyObject>()];
                        }
                    });
                }
            }
            Mutation::PresentDocumentPicker { types } => {
                unsafe {
                    // Build a [UTType] array from the requested identifiers; an
                    // empty list means "any file" (public.item).
                    let id_list = if types.is_empty() {
                        vec!["public.item".to_string()]
                    } else {
                        types
                    };
                    let ut_types = NSMutableArray::<AnyObject>::new();
                    for ident in &id_list {
                        let ns = NSString::from_str(ident);
                        let ut: *mut AnyObject =
                            msg_send![class!(UTType), typeWithIdentifier: &*ns];
                        if !ut.is_null() {
                            let obj: &AnyObject = &*ut;
                            ut_types.addObject(obj);
                        }
                    }

                    let picker: *mut AnyObject =
                        msg_send![class!(UIDocumentPickerViewController), alloc];
                    let picker: *mut AnyObject =
                        msg_send![picker, initForOpeningContentTypes: &*ut_types];
                    if picker.is_null() {
                        // Deliver an empty pick so the app isn't left hanging.
                        PENDING_DOCUMENTS.with(|q| q.borrow_mut().push(Vec::new()));
                        return;
                    }
                    let _: () = msg_send![picker, setAllowsMultipleSelection: true];

                    let delegate: Retained<DocumentPickerDelegate> =
                        msg_send![DocumentPickerDelegate::class(), new];
                    let _: () = msg_send![picker, setDelegate: &*delegate];
                    DOCUMENT_PICKER_DELEGATE.with(|d| *d.borrow_mut() = Some(delegate));

                    STATE.with(|s| {
                        if let Some(state) = s.borrow().as_ref() {
                            let vc: &UIViewController = &state._view_controller;
                            let _: () = msg_send![vc, presentViewController: picker animated: true completion: std::ptr::null::<AnyObject>()];
                        }
                    });
                }
            }
            Mutation::RegisterBackgroundTask { identifier } => {
                // BGTaskScheduler is available on iOS 13+.
                // Registering requires a launch handler block; without one the
                // call is a no-op on the simulator and stores the identifier
                // for BGAppRefreshTask scheduling below.
                // We record registered identifiers so ScheduleBackgroundTask
                // can validate them; on a real device the app also needs
                // BGTaskSchedulerPermittedIdentifiers in Info.plist.
                unsafe {
                    let scheduler: *mut AnyObject =
                        msg_send![class!(BGTaskScheduler), sharedScheduler];
                    if !scheduler.is_null() {
                        // Register without a handler block (queue = nil means
                        // the system uses the main queue). Passing null for the
                        // block is accepted — the system records the identifier
                        // for future task delivery even without a launch handler
                        // pre-registered here; the actual handler should be
                        // registered via UIBackgroundModes and the app delegate.
                        // This call intentionally no-ops on the simulator where
                        // BGTaskScheduler is not fully functional.
                        let id_ns = NSString::from_str(&identifier);
                        let _: () = msg_send![
                            scheduler,
                            registerForTaskWithIdentifier: &*id_ns
                            usingQueue: std::ptr::null::<AnyObject>()
                            launchHandler: std::ptr::null::<AnyObject>()
                        ];
                    }
                }
            }
            Mutation::ScheduleBackgroundTask {
                identifier,
                earliest_seconds,
            } => {
                unsafe {
                    let scheduler: *mut AnyObject =
                        msg_send![class!(BGTaskScheduler), sharedScheduler];
                    if scheduler.is_null() {
                        return;
                    }
                    // BGAppRefreshTaskRequest
                    let id_ns = NSString::from_str(&identifier);
                    let req: *mut AnyObject = msg_send![class!(BGAppRefreshTaskRequest), alloc];
                    if req.is_null() {
                        return;
                    }
                    let req: *mut AnyObject = msg_send![req, initWithIdentifier: &*id_ns];
                    if req.is_null() {
                        return;
                    }
                    // Set the earliest begin date: [NSDate date] + offset.
                    let now: *mut AnyObject = msg_send![class!(NSDate), date];
                    let date: *mut AnyObject =
                        msg_send![now, dateByAddingTimeInterval: earliest_seconds];
                    let _: () = msg_send![req, setEarliestBeginDate: date];
                    // Submit. Errors are silently ignored (common on simulator).
                    let mut err: *mut AnyObject = std::ptr::null_mut();
                    let _: bool = msg_send![scheduler, submitTaskRequest: req error: &mut err];
                }
            }
            Mutation::AuthenticateBiometric { reason } => {
                unsafe {
                    let ctx: *mut AnyObject = msg_send![class!(LAContext), new];
                    // LAPolicyDeviceOwnerAuthenticationWithBiometrics = 1
                    let policy: isize = 1;
                    let mut err: *mut AnyObject = std::ptr::null_mut();
                    let can: bool = msg_send![ctx, canEvaluatePolicy: policy error: &mut err];
                    if !can {
                        PENDING_BIOMETRIC.with(|q| {
                            q.borrow_mut()
                                .push((false, Some("Biometrics not available".to_string())));
                        });
                        return;
                    }
                    let reason_ns = NSString::from_str(&reason);
                    // The reply block fires on an arbitrary thread; push result to
                    // PENDING_BIOMETRIC so it is dispatched safely on the next tick.
                    // LAContext reply signature: (BOOL success, NSError * _Nullable error)
                    // objc2 represents ObjC BOOL as objc2::runtime::Bool.
                    let reply =
                        RcBlock::new(|success: objc2::runtime::Bool, error: *mut NSObject| {
                            let ok = success.as_bool();
                            let err_msg = if ok {
                                None
                            } else if error.is_null() {
                                Some("Authentication failed".to_string())
                            } else {
                                // Pull localizedDescription from NSError.
                                let desc: *mut AnyObject = msg_send![error, localizedDescription];
                                if desc.is_null() {
                                    Some("Authentication failed".to_string())
                                } else {
                                    let ns: *const NSString = desc.cast();
                                    Some((*ns).to_string())
                                }
                            };
                            PENDING_BIOMETRIC.with(|q| q.borrow_mut().push((ok, err_msg)));
                        });
                    let _: () = msg_send![
                        ctx,
                        evaluatePolicy: policy
                        localizedReason: &*reason_ns
                        reply: &*reply
                    ];
                }
            }
            Mutation::CheckPermission { permission } => {
                check_ios_permission(permission);
            }
            Mutation::RequestPermission { permission } => {
                request_ios_permission(permission);
            }
            Mutation::SetClipboard { text } => {
                // UIPasteboard.general.string = text
                unsafe {
                    let pb: *mut AnyObject = msg_send![class!(UIPasteboard), generalPasteboard];
                    let ns = NSString::from_str(&text);
                    let _: () = msg_send![pb, setString: &*ns];
                }
            }
            Mutation::ShareText { text } => {
                // Present UIActivityViewController from the root view controller.
                unsafe {
                    let ns_text = NSString::from_str(&text);
                    // NSArray arrayWithObject: wraps a single item.
                    let items: *mut AnyObject =
                        msg_send![class!(NSArray), arrayWithObject: &*ns_text];
                    let vc: *mut AnyObject = msg_send![class!(UIActivityViewController), alloc];
                    let vc: *mut AnyObject = msg_send![
                        vc,
                        initWithActivityItems: items
                        applicationActivities: std::ptr::null::<AnyObject>()
                    ];
                    // Present from the root view controller stored in IosState.
                    STATE.with(|s| {
                        if let Some(state) = s.borrow().as_ref() {
                            let root_vc: &UIViewController = &state._view_controller;
                            let _: () = msg_send![
                                root_vc,
                                presentViewController: vc
                                animated: true
                                completion: std::ptr::null::<AnyObject>()
                            ];
                        }
                    });
                }
            }
            Mutation::OpenExternalUrl { url } => unsafe {
                let ns_url_str = NSString::from_str(&url);
                let ns_url: *mut AnyObject = msg_send![class!(NSURL), URLWithString: &*ns_url_str];
                if !ns_url.is_null() {
                    let app: *mut AnyObject = msg_send![class!(UIApplication), sharedApplication];
                    let _: bool = msg_send![app, openURL: ns_url];
                }
            },
            Mutation::AnnounceAccessibility { message } => {
                // UIAccessibilityAnnouncementNotification = 1008
                // UIAccessibilityPostNotification(notification, argument)
                // where argument is an NSString for announcements.
                unsafe {
                    extern "C" {
                        fn UIAccessibilityPostNotification(
                            notification: u32,
                            argument: *mut AnyObject,
                        );
                    }
                    let ns = NSString::from_str(&message);
                    UIAccessibilityPostNotification(
                        1008u32,
                        &*ns as *const NSString as *mut AnyObject,
                    );
                }
            }
            Mutation::RequestFocus { id } => {
                // UIAccessibilityScreenChangedNotification = 1000
                // Pass the native view as the argument to move VoiceOver focus.
                if let Some(view) = self.view(id).cloned() {
                    unsafe {
                        extern "C" {
                            fn UIAccessibilityPostNotification(
                                notification: u32,
                                argument: *mut AnyObject,
                            );
                        }
                        UIAccessibilityPostNotification(
                            1000u32,
                            &*view as *const UIView as *mut AnyObject,
                        );
                    }
                }
            }
            Mutation::SetTorch { on } => {
                set_ios_torch(on);
            }
            Mutation::RegisterForPushNotifications => {
                register_ios_remote_notifications();
            }
            Mutation::SetAppBadge { count } => {
                set_ios_app_badge(count);
            }
            Mutation::ScrollTo {
                id,
                offset_x,
                offset_y,
                animated,
            } => {
                if let Some(view) = self.views.get(&id.to_u64()) {
                    if let Ok(sv) = view.clone().downcast::<UIScrollView>() {
                        unsafe {
                            // CGPoint layout: {x: CGFloat, y: CGFloat}
                            let point: [f64; 2] = [offset_x as f64, offset_y as f64];
                            let _: () = msg_send![&*sv, setContentOffset: point animated: animated];
                        }
                    }
                }
            }
            Mutation::ScrollToTop { id, animated } => {
                if let Some(view) = self.views.get(&id.to_u64()) {
                    if let Ok(sv) = view.clone().downcast::<UIScrollView>() {
                        unsafe {
                            let point: [f64; 2] = [0.0_f64, 0.0_f64];
                            let _: () = msg_send![&*sv, setContentOffset: point animated: animated];
                        }
                    }
                }
            }
        }
    }
}

// `Retained<UIView>` upcast helper used above relies on `into_super`, provided
// by objc2 for the class hierarchy (UILabel/UIButton -> ... -> UIView).
