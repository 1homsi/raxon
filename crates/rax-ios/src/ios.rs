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
    UIApplication, UIApplicationDelegate, UIButton, UIButtonType, UIColor, UIControlEvents,
    UIControlState, UIFont, UILabel, UIScreen, UIView, UIViewController, UIWindow,
};

use rax_core::{Color, Rect, Size};
use rax_dom::{Attribute, Backend, Event, EventSink, Host, Mutation, WidgetId, WidgetKind};
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
            App::new(host, viewport, make_view())
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
                        }
                    }
                    Attribute::FontSize(size) => {
                        let font = unsafe { UIFont::systemFontOfSize(size as f64) };
                        if let Ok(label) = view.clone().downcast::<UILabel>() {
                            unsafe { label.setFont(Some(&font)) };
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
        }
    }
}

// `Retained<UIView>` upcast helper used above relies on `into_super`, provided
// by objc2 for the class hierarchy (UILabel/UIButton -> ... -> UIView).
