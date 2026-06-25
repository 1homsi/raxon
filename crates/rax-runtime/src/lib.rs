//! The `rax` app driver: it owns the element tree, mounts the root view inside a
//! reactive ownership scope, runs layout, and drains platform events each frame.
//!
//! A platform backend creates an [`App`], hands it the viewport size, pushes
//! events through [`App::event_sink`], and calls [`App::tick`] once per frame
//! (driven by `CADisplayLink`/`Choreographer`). The runtime is intentionally
//! backend-agnostic — it talks only to the [`Host`] and the layout engine.

#![forbid(unsafe_code)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use rax_core::{Color, ColorScheme, EdgeInsets, Rect, Size};
use rax_dom::{EventSink, Host, Tree, WidgetId, WidgetKind};
use rax_reactive::{create_root, create_signal, provide_context, use_context, Scope, Signal};
use rax_view::{mount, View};

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

/// Sets the [`Backdrop`] (the fill behind the safe area) from within view code.
///
/// Call this while building your root view; the running [`App`] picks it up.
/// Use [`Backdrop::System`] to auto-follow the OS light/dark appearance.
///
/// ```no_run
/// use rax_runtime::{set_backdrop, Backdrop};
/// use rax_core::Color;
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

/// A running application: a mounted view tree plus the per-frame drive loop.
pub struct App {
    tree: Tree,
    root: WidgetId,
    /// Owns all reactivity created while mounting; disposed when the app drops.
    _scope: Scope,
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
    last_tick: Option<std::time::Instant>,
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
        let (root, scope) = create_root(|| {
            // Provide the context handles before building, so view code can call
            // set_backdrop()/use_color_scheme() during construction.
            provide_context(BackdropSlot(backdrop_for_ctx));
            let scheme = create_signal(ColorScheme::Light);
            provide_context(ColorSchemeCtx(scheme));
            scheme_slot = Some(scheme);
            mount(&mut tree, make_view())
        });
        let mut app = App {
            tree,
            root,
            _scope: scope,
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
        };
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
        rax_async::run_until_stalled(); // advance async tasks (may resolve resources)

        // Advance animations by the wall-clock delta since the last frame.
        let now = std::time::Instant::now();
        let dt = self
            .last_tick
            .map(|prev| now.duration_since(prev).as_secs_f32())
            .unwrap_or(0.0);
        self.last_tick = Some(now);
        rax_anim::tick(dt);

        self.tree.drain_events();
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
        let computed = rax_layout::compute(&self.tree, self.root, avail);
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
