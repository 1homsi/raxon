//! The iOS implementation (compiled only for `target_os = "ios"`).

// objc2 marks many UIKit accessors safe, but their safety is version-dependent;
// we keep FFI calls inside `unsafe` blocks to document intent and stay robust
// across objc2 upgrades.
#![allow(unused_unsafe)]
// TODO: migrate the window/screen bootstrap to UIWindowScene (scene manifest).
// The deprecated path works on current simulators and keeps the demo simple.
#![allow(deprecated)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use objc2::rc::Retained;
use objc2::runtime::{NSObject, NSObjectProtocol};
use objc2::{define_class, msg_send, sel, ClassType, MainThreadMarker, MainThreadOnly};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::{NSNotification, NSString};
use objc2_quartz_core::CADisplayLink;
use objc2_ui_kit::{
    NSTextAlignment, UIActivityIndicatorView, UIApplication, UIApplicationDelegate, UIButton,
    UIButtonType, UIColor, UIControl, UIControlEvents, UIControlState, UIFont, UIGestureRecognizer,
    UIGestureRecognizerState, UIImage, UIImageView, UILabel, UILongPressGestureRecognizer,
    UIProgressView, UIScreen, UIScrollView, UISegmentedControl, UISlider, UIStepper, UISwitch,
    UITapGestureRecognizer, UITextBorderStyle, UITextField, UITraitEnvironment,
    UIUserInterfaceStyle, UIView, UIViewController, UIWindow,
};

use rax_core::{Color, ColorScheme, EdgeInsets, Rect, Size};
use rax_dom::{
    Attribute, Backend, Event, EventSink, GestureKind, Host, Mutation, TextSelection, WidgetId,
    WidgetKind,
};
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
}

fn handle_tap(tag_bits: u64) {
    STATE.with(|s| {
        if let Some(state) = s.borrow().as_ref() {
            state.event_sink.dispatch(Event::Tap {
                target: WidgetId::from_u64(tag_bits),
            });
            // Process immediately so the tap feels responsive.
            state.app.borrow_mut().tick();
        }
    });
}

fn handle_tick() {
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
            app.set_safe_area(EdgeInsets {
                top: insets.top as f32,
                right: insets.right as f32,
                bottom: insets.bottom as f32,
                left: insets.left as f32,
            });
            app.tick();
        }
    });
}

fn handle_value_changed(tag_bits: u64, value: f64) {
    STATE.with(|s| {
        if let Some(state) = s.borrow().as_ref() {
            state.event_sink.dispatch(Event::ValueChanged {
                target: WidgetId::from_u64(tag_bits),
                value,
            });
            state.app.borrow_mut().tick();
        }
    });
}

fn dispatch_target_event(make: impl FnOnce(WidgetId) -> Event, tag_bits: u64) {
    STATE.with(|s| {
        if let Some(state) = s.borrow().as_ref() {
            state
                .event_sink
                .dispatch(make(WidgetId::from_u64(tag_bits)));
            state.app.borrow_mut().tick();
        }
    });
}

fn recognizer_tag(recognizer: &UIGestureRecognizer) -> Option<u64> {
    unsafe { recognizer.view() }.map(|v| unsafe { v.tag() } as u64)
}

fn handle_text_changed(tag_bits: u64, value: String) {
    STATE.with(|s| {
        if let Some(state) = s.borrow().as_ref() {
            let selection = TextSelection::caret(value.chars().count());
            state.event_sink.dispatch(Event::TextChanged {
                target: WidgetId::from_u64(tag_bits),
                value,
                selection,
            });
            state.app.borrow_mut().tick();
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

        #[unsafe(method(textChanged:))]
        fn text_changed(&self, sender: &UITextField) {
            let tag = unsafe { sender.tag() } as u64;
            let text = unsafe { sender.text() }.map(|s| s.to_string()).unwrap_or_default();
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
    }
);

fn new_instance<T: ClassType>() -> Retained<T> {
    unsafe { msg_send![T::class(), new] }
}

// ---------------------------------------------------------------------------
// Bootstrap
// ---------------------------------------------------------------------------

fn setup(mtm: MainThreadMarker) {
    let screen = UIScreen::mainScreen(mtm);
    let bounds = screen.bounds();

    let window: Retained<UIWindow> = unsafe { UIWindow::initWithFrame(mtm.alloc(), bounds) };
    let view_controller: Retained<UIViewController> =
        unsafe { UIViewController::initWithNibName_bundle(mtm.alloc(), None, None) };
    let container = view_controller
        .view()
        .expect("view controller has a content view");

    let action_target: Retained<ActionTarget> = new_instance();

    let backend = UiKitBackend {
        mtm,
        container: container.clone(),
        action_target: action_target.clone(),
        views: HashMap::new(),
    };

    let viewport = Size::new(bounds.size.width as f32, bounds.size.height as f32);
    let factory = FACTORY
        .with(|f| f.borrow_mut().take())
        .expect("run() set the factory");
    let app = factory(Host::new(backend), viewport);
    let event_sink = app.event_sink();
    let app = Rc::new(RefCell::new(app));

    unsafe { window.setRootViewController(Some(&view_controller)) };
    window.makeKeyAndVisible();

    let ticker: Retained<Ticker> = new_instance();
    let display_link =
        unsafe { CADisplayLink::displayLinkWithTarget_selector(&ticker, sel!(tick:)) };
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
                    WidgetKind::View => unsafe { UIView::initWithFrame(self.mtm.alloc(), zero) },
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
                        }
                        field.into_super().into_super()
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
                            if let Some(img) = unsafe { UIImage::systemImageNamed(&ns) } {
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
                }
            }
            Mutation::SetFrame { id, rect } => {
                if let Some(view) = self.view(id) {
                    unsafe { view.setFrame(to_cg_rect(rect)) };
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
                        r.into_super()
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
        }
    }
}

// `Retained<UIView>` upcast helper used above relies on `into_super`, provided
// by objc2 for the class hierarchy (UILabel/UIButton -> ... -> UIView).
