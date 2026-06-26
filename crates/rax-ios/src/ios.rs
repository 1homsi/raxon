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
use std::rc::Rc;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol};
use objc2::{class, define_class, msg_send, sel, ClassType, MainThreadMarker, MainThreadOnly};
use objc2_core_foundation::{CGAffineTransform, CGPoint, CGRect, CGSize};
use objc2_foundation::{
    NSData, NSMutableArray, NSNotification, NSNotificationCenter, NSRange, NSString,
};
use objc2_quartz_core::{CADisplayLink, CAGradientLayer};
use objc2_ui_kit::{
    NSTextAlignment, UIActivityIndicatorView, UIApplication, UIApplicationDelegate, UIButton,
    UIButtonType, UIColor, UIControl, UIControlEvents, UIControlState, UIFont, UIGestureRecognizer,
    UIGestureRecognizerState, UIImage, UIImageView, UILabel, UILongPressGestureRecognizer,
    UIPanGestureRecognizer, UIPinchGestureRecognizer, UIProgressView, UIRotationGestureRecognizer,
    UIScreen, UIScrollView,
    UISegmentedControl, UISlider, UIStepper, UISwitch, UITapGestureRecognizer, UITextBorderStyle,
    UITextField, UITextInputTraits, UITextView, UITraitEnvironment, UIUserInterfaceStyle, UIView,
    UIViewController, UIWindow,
};

use block2::RcBlock;

use rax_core::{Color, ColorScheme, EdgeInsets, Point, Rect, Size};
use rax_dom::{
    Attribute, Backend, Event, EventSink, GestureKind, GesturePhase, HapticStyle, Host,
    KeyboardType, LayoutDirection, Mutation, TextDecoration, TextSelection, WidgetId, WidgetKind,
};
// TextStyle is referenced as rax_dom::TextStyle in the match arms.
use rax_runtime::App;
use rax_view::View;

// ---------------------------------------------------------------------------
// Per-thread state. Everything here lives on the main thread.
// ---------------------------------------------------------------------------

type ViewFactory = Box<dyn FnOnce(Host, Size) -> App>;

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
    // Biometric authentication results queued from the reply block (arbitrary thread).
    // Drained in handle_tick and dispatched as Event::BiometricResult.
    static PENDING_BIOMETRIC: RefCell<Vec<(bool, Option<String>)>> = const { RefCell::new(Vec::new()) };
    // GPS location fixes queued from the CLLocationManager delegate.
    static PENDING_LOCATIONS: RefCell<Vec<(f64, f64, f64)>> = const { RefCell::new(Vec::new()) };
    // Set to true if location permission was denied.
    static PENDING_LOCATION_DENIED: Cell<bool> = const { Cell::new(false) };
    // The CLLocationManager instance (raw pointer, retained manually).
    static LOCATION_MANAGER: RefCell<Option<*mut AnyObject>> = const { RefCell::new(None) };
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
    // Frame timestamps for FPS calculation (ring buffer of the last 1 second).
    static FRAME_TIMESTAMPS: RefCell<std::collections::VecDeque<std::time::Instant>> =
        RefCell::new(std::collections::VecDeque::new());
    // Tick counter used to throttle UIDevice battery polling (once per ~60 ticks).
    static BATTERY_TICK: Cell<u64> = const { Cell::new(0) };
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
        const ENCODING: objc2::encode::Encoding =
            objc2::encode::Encoding::Struct("CMAcceleration", &[
                f64::ENCODING,
                f64::ENCODING,
                f64::ENCODING,
            ]);
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
                    q.borrow_mut().push((accel.0, accel.1, accel.2, gyro.0, gyro.1, gyro.2));
                });
            }
        }
    });

    // Drain motion events queued above.
    let motion_events: Vec<(Option<f64>, Option<f64>, Option<f64>, Option<f64>, Option<f64>, Option<f64>)> =
        PENDING_MOTION.with(|q| {
            let mut v = q.borrow_mut();
            std::mem::take(&mut *v)
        });

    // Drain media picker results and cancellation flag.
    let media_results: Vec<Vec<std::sync::Arc<Vec<u8>>>> = PENDING_MEDIA.with(|q| {
        let mut v = q.borrow_mut();
        std::mem::take(&mut *v)
    });
    let media_cancelled = PENDING_MEDIA_CANCEL.with(|c| c.replace(false));

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
                    fn UIAccessibilityIsDarkerSystemColorsEnabled() -> bool;
                }
                UIAccessibilityIsDarkerSystemColorsEnabled()
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
            // Dispatch queued biometric results into the event system.
            for (success, error) in biometric_results {
                state.event_sink.dispatch(Event::BiometricResult { success, error });
            }
            // Dispatch queued GPS location fixes.
            for (latitude, longitude, accuracy) in location_fixes {
                state.event_sink.dispatch(Event::LocationUpdated { latitude, longitude, accuracy });
            }
            if location_denied {
                state.event_sink.dispatch(Event::LocationDenied);
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
            app.tick();
            rax_plugin::tick_plugins();

            // Poll UIDevice battery state roughly once per second (~60 ticks).
            // Battery monitoring must be enabled before reading level/state.
            BATTERY_TICK.with(|c| {
                let tick = c.get();
                c.set(tick.wrapping_add(1));
                if tick % 60 == 0 {
                    unsafe {
                        let device: *mut AnyObject =
                            msg_send![class!(UIDevice), currentDevice];
                        let _: () = msg_send![device, setBatteryMonitoringEnabled: true];
                        // batteryLevel returns -1.0 when monitoring is not enabled
                        // or the level is unknown; clamp to 0.0 in that case.
                        let level: f32 = msg_send![device, batteryLevel];
                        // batteryState: 0=unknown, 1=unplugged, 2=charging, 3=full
                        let state: isize = msg_send![device, batteryState];
                        let charging = state == 2 || state == 3;
                        rax_runtime::update_battery(level.max(0.0), charging);
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
                while deq.front().map(|t| now.duration_since(*t) > std::time::Duration::from_secs(1)).unwrap_or(false) {
                    deq.pop_front();
                }
                deq.len() as f32
            });
            rax_view::update_fps(fps);
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

        #[unsafe(method(handleRefresh:))]
        fn handle_refresh(&self, sender: &UIControl) {
            let tag = unsafe { sender.tag() } as u64;
            dispatch_target_event(|target| Event::Refresh { target }, tag);
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
                move |target| rax_dom::Event::PinchChanged {
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
                move |target| rax_dom::Event::RotateChanged {
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
        }

        /// `locationManagerDidChangeAuthorization:` — fires on iOS 14+ when status changes.
        #[unsafe(method(locationManagerDidChangeAuthorization:))]
        fn did_change_auth(&self, manager: &AnyObject) {
            // kCLAuthorizationStatusDenied = 2, kCLAuthorizationStatusRestricted = 1
            let status: isize = unsafe { msg_send![manager, authorizationStatus] };
            if status == 1 || status == 2 {
                PENDING_LOCATION_DENIED.with(|c| c.set(true));
            }
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
                PENDING_MEDIA_CANCEL.with(|c| c.set(true));
                // Drop the delegate reference now that we are done.
                MEDIA_PICKER_DELEGATE.with(|d| *d.borrow_mut() = None);
                return;
            }

            // First version: signal that media was picked with empty byte payloads.
            // Full async loading via NSItemProvider requires dispatch queues and is
            // planned as a follow-up (see roadmap). The app receives MediaPicked with
            // an empty images vec and can display a confirmation or trigger its own load.
            PENDING_MEDIA.with(|q| q.borrow_mut().push(vec![]));
            // Drop the delegate now that the pick is complete.
            MEDIA_PICKER_DELEGATE.with(|d| *d.borrow_mut() = None);
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
        let main_queue: *mut AnyObject = {
            extern "C" {
                fn dispatch_get_main_queue() -> *mut AnyObject;
            }
            dispatch_get_main_queue()
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
            map.borrow_mut().insert(widget_tag, QrEntry { session, delegate });
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
    rax_net::set_client(crate::http::UreqClient);
    // Persist rax-store keys to NSUserDefaults across launches.
    rax_store::set_storage(crate::storage::UiKitStorage);

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
    };

    let viewport = Size::new(bounds.size.width as f32, bounds.size.height as f32);
    let factory = FACTORY
        .with(|f| f.borrow_mut().take())
        .expect("run() set the factory");
    rax_plugin::start_plugins();
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
        let range = CAFrameRateRange { minimum: 60.0, maximum: 120.0, preferred: 120.0 };
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
}

impl UiKitBackend {
    fn view(&self, id: WidgetId) -> Option<&Retained<UIView>> {
        self.views.get(&id.to_u64())
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
                    WidgetKind::MapView => {
                        unsafe {
                            let frame = CGRect {
                                origin: CGPoint { x: 0.0, y: 0.0 },
                                size: CGSize { width: 100.0, height: 100.0 },
                            };
                            let mv: *mut AnyObject = msg_send![class!(MKMapView), alloc];
                            let mv: *mut AnyObject = msg_send![mv, initWithFrame: frame];
                            let mv_view: *mut UIView = mv as *mut UIView;
                            Retained::retain(mv_view).expect("MKMapView init failed")
                        }
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
                                .unwrap_or_else(|| unsafe {
                                    UIFont::systemFontOfSize(size)
                                });
                            unsafe { label.setFont(Some(&font)) };
                        }
                    }
                    Attribute::TextAlign(align) => {
                        if let Ok(label) = view.clone().downcast::<UILabel>() {
                            let a = match align {
                                rax_dom::TextAlign::Start => NSTextAlignment::Natural,
                                rax_dom::TextAlign::Center => NSTextAlignment::Center,
                                rax_dom::TextAlign::End => NSTextAlignment::Right,
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
                            rax_dom::Role::None => 0,
                            rax_dom::Role::Button => 1 << 0,
                            rax_dom::Role::Link => 1 << 1,
                            rax_dom::Role::Image => 1 << 2,
                            rax_dom::Role::Search => 1 << 10,
                            rax_dom::Role::Adjustable => 1 << 12,
                            rax_dom::Role::Header => 1 << 15,
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
                            }
                        }
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
                                    let _: () =
                                        msg_send![new_rc, setTag: id.to_u64() as isize];
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
                            unsafe { let _: () = msg_send![&*sv, setScrollEnabled: enabled]; }
                        }
                    }
                    Attribute::ShowsScrollIndicator(show) => {
                        if let Ok(sv) = view.clone().downcast::<UIScrollView>() {
                            unsafe {
                                let _: () = msg_send![&*sv, setShowsHorizontalScrollIndicator: show];
                                let _: () = msg_send![&*sv, setShowsVerticalScrollIndicator: show];
                            }
                        }
                    }
                    Attribute::PagingEnabled(enabled) => {
                        if let Ok(sv) = view.clone().downcast::<UIScrollView>() {
                            unsafe { let _: () = msg_send![&*sv, setPagingEnabled: enabled]; }
                        }
                    }
                    Attribute::ContentInset { top, right, bottom, left } => {
                        if let Ok(sv) = view.clone().downcast::<UIScrollView>() {
                            unsafe {
                                // UIEdgeInsets memory layout: {top, left, bottom, right} (all CGFloat / f64)
                                let insets: [f64; 4] = [
                                    top as f64,
                                    left as f64,
                                    bottom as f64,
                                    right as f64,
                                ];
                                // TODO: call setContentInset: with UIEdgeInsets struct via objc2
                                // when the binding stabilises. For now we record the values so
                                // the attribute round-trips correctly through the mutation stream.
                                let _ = (sv, insets);
                            }
                        }
                    }
                    Attribute::ReturnKey(ret) => {
                        if let Ok(field) = view.clone().downcast::<UITextField>() {
                            let v: isize = match ret {
                                rax_dom::ReturnKeyType::Default => 0,
                                rax_dom::ReturnKeyType::Go => 1,
                                rax_dom::ReturnKeyType::Next => 4,
                                rax_dom::ReturnKeyType::Search => 8,
                                rax_dom::ReturnKeyType::Send => 9,
                                rax_dom::ReturnKeyType::Done => 9,
                            };
                            unsafe {
                                let _: () = msg_send![&*field, setReturnKeyType: v];
                            }
                        }
                    }
                    Attribute::Secure(secure) => {
                        if let Ok(field) = view.clone().downcast::<UITextField>() {
                            unsafe { field.setSecureTextEntry(secure) };
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
                                    let _: () = msg_send![attrs, setObject: font forKey: &*font_key];

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
                                        let kern_val: *mut AnyObject =
                                            msg_send![class!(NSNumber), numberWithFloat: kern as f64];
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
                            unsafe { let _: () = msg_send![&*field, setKeyboardType: ktype]; }
                        } else if let Ok(tv) = view.clone().downcast::<UITextView>() {
                            unsafe { let _: () = msg_send![&*tv, setKeyboardType: ktype]; }
                        }
                    }
                    Attribute::AccessibilityHint(hint) => {
                        let ns = NSString::from_str(&hint);
                        unsafe {
                            let _: () = msg_send![&*view, setIsAccessibilityElement: true];
                            let _: () = msg_send![&*view, setAccessibilityHint: &*ns];
                        }
                    }
                    Attribute::AccessibilityHidden(hidden) => {
                        unsafe {
                            let _: () = msg_send![&*view, setAccessibilityElementsHidden: hidden];
                        }
                    }
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
                    Attribute::Url(url) => {
                        unsafe {
                            let ns_url_str = NSString::from_str(&url);
                            let ns_url: *mut AnyObject = msg_send![class!(NSURL), URLWithString: &*ns_url_str];
                            if !ns_url.is_null() {
                                let request: *mut AnyObject = msg_send![class!(NSURLRequest), requestWithURL: ns_url];
                                let _: () = msg_send![&*view, loadRequest: request];
                            }
                        }
                    }
                    Attribute::Html(html) => {
                        unsafe {
                            let ns_html = NSString::from_str(&html);
                            let base_url: *mut AnyObject = std::ptr::null_mut();
                            let _: () = msg_send![&*view, loadHTMLString: &*ns_html baseURL: base_url];
                        }
                    }
                    Attribute::TextStyle(style) => {
                        let style_name = match style {
                            rax_dom::TextStyle::LargeTitle => "UICTFontTextStyleLargeTitle",
                            rax_dom::TextStyle::Title1 => "UICTFontTextStyleTitle1",
                            rax_dom::TextStyle::Title2 => "UICTFontTextStyleTitle2",
                            rax_dom::TextStyle::Title3 => "UICTFontTextStyleTitle3",
                            rax_dom::TextStyle::Headline => "UIFontTextStyleHeadline",
                            rax_dom::TextStyle::Subheadline => "UIFontTextStyleSubheadline",
                            rax_dom::TextStyle::Body => "UIFontTextStyleBody",
                            rax_dom::TextStyle::Callout => "UIFontTextStyleCallout",
                            rax_dom::TextStyle::Footnote => "UIFontTextStyleFootnote",
                            rax_dom::TextStyle::Caption1 => "UIFontTextStyleCaption1",
                            rax_dom::TextStyle::Caption2 => "UIFontTextStyleCaption2",
                        };
                        if let Ok(label) = view.clone().downcast::<UILabel>() {
                            unsafe {
                                let ns_style = NSString::from_str(style_name);
                                let font: *mut AnyObject =
                                    msg_send![class!(UIFont), preferredFontForTextStyle: &*ns_style];
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
                    Attribute::MapCenter { latitude, longitude } => {
                        unsafe {
                            #[repr(C)]
                            struct CLLocationCoordinate2D { latitude: f64, longitude: f64 }
                            unsafe impl objc2::Encode for CLLocationCoordinate2D {
                                const ENCODING: objc2::encode::Encoding =
                                    objc2::encode::Encoding::Struct("CLLocationCoordinate2D", &[
                                        f64::ENCODING, f64::ENCODING,
                                    ]);
                            }
                            let coord = CLLocationCoordinate2D { latitude, longitude };
                            let _: () = msg_send![&*view, setCenterCoordinate: coord animated: false];
                        }
                    }
                    Attribute::MapSpan { lat_span, lon_span } => {
                        unsafe {
                            #[repr(C)]
                            struct CLLocationCoordinate2D { latitude: f64, longitude: f64 }
                            unsafe impl objc2::Encode for CLLocationCoordinate2D {
                                const ENCODING: objc2::encode::Encoding =
                                    objc2::encode::Encoding::Struct("CLLocationCoordinate2D", &[
                                        f64::ENCODING, f64::ENCODING,
                                    ]);
                            }
                            #[repr(C)]
                            struct MKCoordinateSpan { latitude_delta: f64, longitude_delta: f64 }
                            unsafe impl objc2::Encode for MKCoordinateSpan {
                                const ENCODING: objc2::encode::Encoding =
                                    objc2::encode::Encoding::Struct("MKCoordinateSpan", &[
                                        f64::ENCODING, f64::ENCODING,
                                    ]);
                            }
                            #[repr(C)]
                            struct MKCoordinateRegion {
                                center: CLLocationCoordinate2D,
                                span: MKCoordinateSpan,
                            }
                            unsafe impl objc2::Encode for MKCoordinateRegion {
                                const ENCODING: objc2::encode::Encoding =
                                    objc2::encode::Encoding::Struct("MKCoordinateRegion", &[
                                        CLLocationCoordinate2D::ENCODING,
                                        MKCoordinateSpan::ENCODING,
                                    ]);
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
                        }
                    }
                    Attribute::MapAnnotation { annotation_id, latitude, longitude, title } => {
                        unsafe {
                            #[repr(C)]
                            struct CLLocationCoordinate2D { latitude: f64, longitude: f64 }
                            unsafe impl objc2::Encode for CLLocationCoordinate2D {
                                const ENCODING: objc2::encode::Encoding =
                                    objc2::encode::Encoding::Struct("CLLocationCoordinate2D", &[
                                        f64::ENCODING, f64::ENCODING,
                                    ]);
                            }
                            let coord = CLLocationCoordinate2D { latitude, longitude };
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
                    Attribute::HitSlop { top, right, bottom, left } => {
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

                                let para_key =
                                    NSString::from_str("NSParagraphStyle");
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
                                let (underline_style, strike_style): (i32, i32) =
                                    match decoration {
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
                        // TODO: attach UIControl touchDown or a zero-duration long-press
                        // recognizer to invoke the callback on touch-down. Stored as no-op
                        // so the attribute round-trips without crash.
                        let _ = cb;
                    }
                    Attribute::OnPressOut(cb) => {
                        // TODO: attach UIControl touchUpInside/touchUpOutside to invoke cb.
                        let _ = cb;
                    }
                    Attribute::Cursor(_style) => {
                        // Pointer cursor — no-op on touch-only iOS.
                    }
                    Attribute::OnSwipe { direction, handler } => {
                        unsafe {
                            view.setUserInteractionEnabled(true);
                            // UISwipeGestureRecognizerDirection:
                            //   Right = 1, Left = 2, Up = 4, Down = 8
                            let dir_bits: usize = match direction {
                                rax_dom::SwipeDirection::Right => 1,
                                rax_dom::SwipeDirection::Left => 2,
                                rax_dom::SwipeDirection::Up => 4,
                                rax_dom::SwipeDirection::Down => 8,
                            };
                            let recognizer: *mut AnyObject =
                                msg_send![class!(UISwipeGestureRecognizer), alloc];
                            let recognizer: *mut AnyObject = msg_send![recognizer, init];
                            let _: () = msg_send![recognizer, setDirection: dir_bits];
                            // TODO: wire to ActionTarget so the callback fires on recognition.
                            let _ = handler;
                            view.addGestureRecognizer(
                                &*(recognizer as *const UIGestureRecognizer),
                            );
                        }
                    }
                    Attribute::PlaceholderColor(color) => {
                        if let Ok(field) = view.clone().downcast::<UITextField>() {
                            unsafe {
                                // Build an NSAttributedString with the placeholder text and the
                                // requested color, then assign it to attributedPlaceholder.
                                let current_placeholder: *mut AnyObject =
                                    msg_send![&*field, placeholder];
                                let placeholder_str: *mut AnyObject = if current_placeholder.is_null() {
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
                                let _: () =
                                    msg_send![&*field, setAttributedPlaceholder: attr_str];
                            }
                        }
                    }
                    Attribute::InputPrefix(text) => {
                        // TODO: create a UILabel and set it as the field's leftView
                        // with leftViewMode = UITextFieldViewModeAlways (raw value 1).
                        let _ = text;
                    }
                    Attribute::InputSuffix(text) => {
                        // TODO: create a UILabel and set it as the field's rightView.
                        let _ = text;
                    }
                    Attribute::ClearButton(show) => {
                        if let Ok(field) = view.clone().downcast::<UITextField>() {
                            unsafe {
                                // UITextFieldViewModeWhileEditing = 1, UITextFieldViewModeNever = 0
                                let mode: usize = if show { 1 } else { 0 };
                                let _: () = msg_send![&*field, setClearButtonMode: mode];
                            }
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
                        // TODO: implement via UITextFieldDelegate
                        // textField:shouldChangeCharactersInRange:replacementString: — check
                        // (current.count - range.length + replacement.count) <= n.
                        let _ = n;
                    }
                    Attribute::TextShadow { color, offset_x, offset_y, blur } => {
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
                            let app: *mut AnyObject = msg_send![class!(UIApplication), sharedApplication];
                            let style_val: i64 = match style {
                                rax_dom::StatusBarStyle::Dark => 3,  // UIStatusBarStyleDarkContent
                                rax_dom::StatusBarStyle::Light => 1, // UIStatusBarStyleLightContent
                                rax_dom::StatusBarStyle::Auto => 0,  // UIStatusBarStyleDefault
                            };
                            let _: () = msg_send![app, setStatusBarStyle: style_val animated: false];
                        }
                    }
                    Attribute::AspectRatio(_ratio) => {
                        // Aspect ratio is enforced via an NSLayoutConstraint on the native
                        // view. Since rax drives layout through its own flex pass and sets
                        // frames directly, we store the ratio as a hint for a future
                        // constraint-based layout path. For now this is a tracked no-op.
                    }
                    Attribute::BlurRadius(radius) => {
                        // Apply a UIVisualEffectView blur overlay as a subview sized to
                        // fill the parent. For now this is a tracked no-op — the radius
                        // value is stored but not yet passed to UIBlurEffect.
                        // TODO: create a UIVisualEffectView with UIBlurEffect(style: .regular)
                        // and overlay it; scale opacity by radius/20.0.
                        let _ = radius;
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
                    Attribute::UserSelectText(selectable) => {
                        unsafe {
                            let _: () = msg_send![&*view, setUserInteractionEnabled: selectable];
                        }
                    }
                    Attribute::ParagraphSpacing(spacing) => {
                        // TODO: apply NSParagraphStyle paragraphSpacing to UILabel/UITextView
                        let _ = spacing;
                    }
                    Attribute::FontStyle(style) => {
                        use rax_dom::FontStyle;
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
                        // TODO: create UIAccessibilityCustomAction objects for each name
                        // and set them via setAccessibilityCustomActions: on the view.
                        let _ = actions;
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
                    let has_session = QR_SESSIONS
                        .with(|map| map.borrow().contains_key(&id.to_u64()));
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
                        unsafe { let _: () = msg_send![&*r, setDelegate: &*self.action_target]; }
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
                        unsafe { let _: () = msg_send![&*r, setDelegate: &*self.action_target]; }
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
                        unsafe { let _: () = msg_send![&*r, setDelegate: &*self.action_target]; }
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
                        unsafe { let _: () = msg_send![&*r, setDelegate: &*self.action_target]; }
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
                        unsafe { let _: () = msg_send![&*r, setDelegate: &*self.action_target]; }
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
                        unsafe { let _: () = msg_send![&*r, setDelegate: &*self.action_target]; }
                        r.into_super()
                    }
                    GestureKind::Swipe => {
                        // Create a UISwipeGestureRecognizer via raw msg_send! (no direction
                        // stored here — direction is set on the Attribute::OnSwipe arm which
                        // creates its own recognizer). This arm satisfies the exhaustive match;
                        // a default left-swipe recognizer is attached as a placeholder.
                        // TODO: thread the SwipeDirection through GestureKind::Swipe.
                        unsafe {
                            let r: *mut AnyObject =
                                msg_send![class!(UISwipeGestureRecognizer), alloc];
                            let r: *mut AnyObject = msg_send![r, init];
                            let r_view: *mut UIGestureRecognizer = r as *mut UIGestureRecognizer;
                            Retained::retain(r_view)
                                .expect("UISwipeGestureRecognizer init returned a valid object")
                        }
                    }
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
                    let null_auth: *const block2::Block<dyn Fn(objc2::runtime::Bool, *mut NSObject)> =
                        std::ptr::null();
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
                    let null_comp: *const block2::Block<dyn Fn(*mut NSObject)> =
                        std::ptr::null();
                    let _: () = msg_send![
                        center,
                        addNotificationRequest: request
                        withCompletionHandler: null_comp
                    ];
                }
            }
            Mutation::CancelNotification { id } => {
                unsafe {
                    let center: *mut AnyObject =
                        msg_send![class!(UNUserNotificationCenter), currentNotificationCenter];
                    let ids = NSMutableArray::<NSString>::new();
                    ids.addObject(&NSString::from_str(&id));
                    let _: () = msg_send![
                        center,
                        removePendingNotificationRequestsWithIdentifiers: &*ids
                    ];
                }
            }
            Mutation::StartLocation => {
                unsafe {
                    let mgr: *mut AnyObject = msg_send![class!(CLLocationManager), new];
                    if mgr.is_null() {
                        return;
                    }
                    let delegate: *mut AnyObject = msg_send![LocationDelegate::class(), new];
                    let _: () = msg_send![mgr, setDelegate: delegate];
                    // Request when-in-use authorization (matches typical app usage).
                    let _: () = msg_send![mgr, requestWhenInUseAuthorization];
                    // Start streaming location updates.
                    let _: () = msg_send![mgr, startUpdatingLocation];
                    // Store the manager so it stays alive. objc `new` gives +1 retain;
                    // we take ownership here without an extra retain.
                    LOCATION_MANAGER.with(|lm| {
                        // Release any previous manager first.
                        if let Some(old) = lm.borrow_mut().take() {
                            let _: () = msg_send![old, stopUpdatingLocation];
                            objc_release(old);
                        }
                        *lm.borrow_mut() = Some(mgr);
                    });
                    // The delegate was created with `new` (+1) — keep it alive via the
                    // manager's delegate property (which retains it). Release our +1.
                    objc_release(delegate);
                }
            }
            Mutation::StopLocation => {
                LOCATION_MANAGER.with(|lm| {
                    if let Some(mgr) = lm.borrow_mut().take() {
                        unsafe {
                            let _: () = msg_send![mgr, stopUpdatingLocation];
                            objc_release(mgr);
                        }
                    }
                });
            }
            Mutation::StartMotion { accelerometer, gyroscope } => {
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
            Mutation::ScheduleBackgroundTask { identifier, earliest_seconds } => {
                unsafe {
                    let scheduler: *mut AnyObject =
                        msg_send![class!(BGTaskScheduler), sharedScheduler];
                    if scheduler.is_null() {
                        return;
                    }
                    // BGAppRefreshTaskRequest
                    let id_ns = NSString::from_str(&identifier);
                    let req: *mut AnyObject =
                        msg_send![class!(BGAppRefreshTaskRequest), alloc];
                    if req.is_null() {
                        return;
                    }
                    let req: *mut AnyObject =
                        msg_send![req, initWithIdentifier: &*id_ns];
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
                    let _: bool =
                        msg_send![scheduler, submitTaskRequest: req error: &mut err];
                }
            }
            Mutation::AuthenticateBiometric { reason } => {
                unsafe {
                    let ctx: *mut AnyObject = msg_send![class!(LAContext), new];
                    // LAPolicyDeviceOwnerAuthenticationWithBiometrics = 1
                    let policy: isize = 1;
                    let mut err: *mut AnyObject = std::ptr::null_mut();
                    let can: bool =
                        msg_send![ctx, canEvaluatePolicy: policy error: &mut err];
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
                    let reply = RcBlock::new(
                        |success: objc2::runtime::Bool, error: *mut NSObject| {
                            let ok = success.as_bool();
                            let err_msg = if ok {
                                None
                            } else if error.is_null() {
                                Some("Authentication failed".to_string())
                            } else {
                                // Pull localizedDescription from NSError.
                                let desc: *mut AnyObject =
                                    msg_send![error, localizedDescription];
                                if desc.is_null() {
                                    Some("Authentication failed".to_string())
                                } else {
                                    let ns: *const NSString = desc.cast();
                                    Some((*ns).to_string())
                                }
                            };
                            PENDING_BIOMETRIC
                                .with(|q| q.borrow_mut().push((ok, err_msg)));
                        },
                    );
                    let _: () = msg_send![
                        ctx,
                        evaluatePolicy: policy
                        localizedReason: &*reason_ns
                        reply: &*reply
                    ];
                }
            }
            Mutation::SetClipboard { text } => {
                // UIPasteboard.general.string = text
                unsafe {
                    let pb: *mut AnyObject =
                        msg_send![class!(UIPasteboard), generalPasteboard];
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
                    let vc: *mut AnyObject =
                        msg_send![class!(UIActivityViewController), alloc];
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
            Mutation::RequestLocation => {
                // TODO: call [CLLocationManager startUpdatingLocation]
            }
            Mutation::StopLocationUpdates => {
                // TODO: call [CLLocationManager stopUpdatingLocation]
            }
            Mutation::SetTorch { on } => {
                // TODO: set AVCaptureDevice torch mode
                let _ = on;
            }
            Mutation::RegisterForPushNotifications => {
                // TODO: call [UIApplication registerForRemoteNotifications]
            }
            Mutation::SetAppBadge { count } => {
                // TODO: set [[UIApplication sharedApplication] applicationIconBadgeNumber]
                let _ = count;
            }
        }
    }
}

// `Retained<UIView>` upcast helper used above relies on `into_super`, provided
// by objc2 for the class hierarchy (UILabel/UIButton -> ... -> UIView).
