//! Universal layout modifiers via [`ViewExt`]. Any view can be sized, grown,
//! margined, aligned, or positioned by wrapping it in a [`Styled`] that overrides
//! specific [`LayoutStyle`] fields after the inner view builds. This is a core
//! customizability mechanism: layout control on *every* view, not just containers.

use rax_core::{AlignItems, Color, Dimension, EdgeInsets, LayoutStyle, Point, Position};
use rax_dom::{
    Attribute, EventKind, GesturePhase, GestureKind, LayoutDirection, LinearGradient, Role, Shadow,
    Transform, Tree, WidgetId,
};

use crate::view::View;

/// Payload delivered to [`ViewExt::on_pan`] on each pan-gesture update.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanInfo {
    /// Cumulative translation from the gesture start, in points.
    pub translation: Point,
    /// Current drag velocity, in points/second.
    pub velocity: Point,
    /// Lifecycle phase (began / changed / ended).
    pub phase: GesturePhase,
}

/// Payload delivered to [`ViewExt::on_pinch`] on each pinch-gesture update.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PinchInfo {
    /// Cumulative scale factor since gesture began (1.0 = no change).
    pub scale: f32,
    /// Scale velocity in scale-units per second.
    pub velocity: f32,
    /// Lifecycle phase (began / changed / ended).
    pub phase: GesturePhase,
}

/// Payload delivered to [`ViewExt::on_rotate`] on each rotation-gesture update.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RotateInfo {
    /// Cumulative rotation in radians.
    pub rotation: f32,
    /// Rotation velocity (radians/second).
    pub velocity: f32,
    /// Gesture phase.
    pub phase: GesturePhase,
}

/// A view whose layout style is post-processed by `apply` after it builds.
pub struct Styled<V, F> {
    inner: V,
    apply: F,
}

impl<V: View, F: FnOnce(&mut LayoutStyle)> View for Styled<V, F> {
    fn build(self, tree: &mut Tree) -> WidgetId {
        let id = self.inner.build(tree);
        let mut style = tree.style_of(id).unwrap_or_default();
        (self.apply)(&mut style);
        tree.set_style(id, style);
        id
    }
}

/// A view that runs `decorate` (typically emitting paint attributes) after it
/// builds. Powers per-view paint modifiers like `.background`/`.border`.
pub struct Decorated<V, F> {
    inner: V,
    decorate: F,
}

impl<V: View, F: FnOnce(&mut Tree, WidgetId)> View for Decorated<V, F> {
    fn build(self, tree: &mut Tree) -> WidgetId {
        let id = self.inner.build(tree);
        (self.decorate)(tree, id);
        id
    }
}

/// Layout modifiers available on every [`View`].
pub trait ViewExt: View + Sized {
    /// Override arbitrary layout-style fields.
    fn style_with(
        self,
        f: impl FnOnce(&mut LayoutStyle),
    ) -> Styled<Self, impl FnOnce(&mut LayoutStyle)> {
        Styled {
            inner: self,
            apply: f,
        }
    }

    /// Flex-grow factor: how much of the parent's free main-axis space this view
    /// claims, relative to its siblings. Works on any view (text, input, image,
    /// …), not just containers.
    fn grow(self, factor: f32) -> Styled<Self, impl FnOnce(&mut LayoutStyle)> {
        self.style_with(move |s| s.flex_grow = factor)
    }

    /// Fixed width in points.
    fn width(self, w: f32) -> Styled<Self, impl FnOnce(&mut LayoutStyle)> {
        self.style_with(move |s| s.width = Dimension::Points(w))
    }
    /// Fixed height in points.
    fn height(self, h: f32) -> Styled<Self, impl FnOnce(&mut LayoutStyle)> {
        self.style_with(move |s| s.height = Dimension::Points(h))
    }
    /// Fixed width and height.
    fn size(self, w: f32, h: f32) -> Styled<Self, impl FnOnce(&mut LayoutStyle)> {
        self.style_with(move |s| {
            s.width = Dimension::Points(w);
            s.height = Dimension::Points(h);
        })
    }
    /// Width as a percent (0..=100) of the parent.
    fn width_percent(self, p: f32) -> Styled<Self, impl FnOnce(&mut LayoutStyle)> {
        self.style_with(move |s| s.width = Dimension::Percent(p))
    }
    /// Minimum width.
    fn min_width(self, w: f32) -> Styled<Self, impl FnOnce(&mut LayoutStyle)> {
        self.style_with(move |s| s.min_width = Dimension::Points(w))
    }
    /// Maximum width.
    fn max_width(self, w: f32) -> Styled<Self, impl FnOnce(&mut LayoutStyle)> {
        self.style_with(move |s| s.max_width = Dimension::Points(w))
    }
    /// Minimum height.
    fn min_height(self, h: f32) -> Styled<Self, impl FnOnce(&mut LayoutStyle)> {
        self.style_with(move |s| s.min_height = Dimension::Points(h))
    }
    /// Maximum height.
    fn max_height(self, h: f32) -> Styled<Self, impl FnOnce(&mut LayoutStyle)> {
        self.style_with(move |s| s.max_height = Dimension::Points(h))
    }
    /// Flex-grow factor.
    fn flex_grow(self, g: f32) -> Styled<Self, impl FnOnce(&mut LayoutStyle)> {
        self.style_with(move |s| s.flex_grow = g)
    }
    /// Flex-shrink factor.
    fn flex_shrink(self, sh: f32) -> Styled<Self, impl FnOnce(&mut LayoutStyle)> {
        self.style_with(move |s| s.flex_shrink = sh)
    }
    /// Flex-basis (main-axis start size) in points.
    fn flex_basis(self, b: f32) -> Styled<Self, impl FnOnce(&mut LayoutStyle)> {
        self.style_with(move |s| s.flex_basis = Dimension::Points(b))
    }
    /// Uniform margin on all edges.
    fn margin(self, m: f32) -> Styled<Self, impl FnOnce(&mut LayoutStyle)> {
        self.style_with(move |s| s.margin = EdgeInsets::all(m))
    }
    /// Per-edge margin.
    fn margin_insets(self, e: EdgeInsets) -> Styled<Self, impl FnOnce(&mut LayoutStyle)> {
        self.style_with(move |s| s.margin = e)
    }
    /// Override this view's cross-axis alignment within its parent.
    fn align_self(self, a: AlignItems) -> Styled<Self, impl FnOnce(&mut LayoutStyle)> {
        self.style_with(move |s| s.align_self = Some(a))
    }
    /// Take this view out of flow (absolutely positioned).
    fn absolute(self) -> Styled<Self, impl FnOnce(&mut LayoutStyle)> {
        self.style_with(|s| s.position = Position::Absolute)
    }
    /// Set inset offsets (for absolute positioning / nudging).
    fn inset(self, e: EdgeInsets) -> Styled<Self, impl FnOnce(&mut LayoutStyle)> {
        self.style_with(move |s| s.inset = e)
    }
    /// Constrain to a width/height aspect ratio.
    fn aspect_ratio(self, r: f32) -> Styled<Self, impl FnOnce(&mut LayoutStyle)> {
        self.style_with(move |s| s.aspect_ratio = Some(r))
    }

    // --- paint modifiers (emit attributes after build) ---

    /// Run an arbitrary decoration after the view builds.
    fn decorate(
        self,
        f: impl FnOnce(&mut Tree, WidgetId),
    ) -> Decorated<Self, impl FnOnce(&mut Tree, WidgetId)> {
        Decorated {
            inner: self,
            decorate: f,
        }
    }
    /// Background fill color.
    fn background(self, color: Color) -> Decorated<Self, impl FnOnce(&mut Tree, WidgetId)> {
        self.decorate(move |t, id| t.set(id, Attribute::BackgroundColor(color)))
    }
    /// Rounded corners.
    fn corner_radius(self, radius: f32) -> Decorated<Self, impl FnOnce(&mut Tree, WidgetId)> {
        self.decorate(move |t, id| t.set(id, Attribute::CornerRadius(radius)))
    }
    /// Opacity, `0.0`..`1.0`.
    fn opacity(self, o: f32) -> Decorated<Self, impl FnOnce(&mut Tree, WidgetId)> {
        self.decorate(move |t, id| t.set(id, Attribute::Opacity(o)))
    }
    /// A uniform border of `width` and `color`.
    fn border(self, width: f32, color: Color) -> Decorated<Self, impl FnOnce(&mut Tree, WidgetId)> {
        self.decorate(move |t, id| {
            t.set(id, Attribute::BorderWidth(width));
            t.set(id, Attribute::BorderColor(color));
        })
    }
    /// A drop shadow.
    fn shadow(
        self,
        color: Color,
        radius: f32,
        dx: f32,
        dy: f32,
    ) -> Decorated<Self, impl FnOnce(&mut Tree, WidgetId)> {
        self.decorate(move |t, id| {
            t.set(
                id,
                Attribute::Shadow(Shadow {
                    color,
                    radius,
                    dx,
                    dy,
                }),
            )
        })
    }

    /// Applies a 2D affine [`Transform`] (scale/rotate/translate) to rendering.
    fn transform(self, t: Transform) -> Decorated<Self, impl FnOnce(&mut Tree, WidgetId)> {
        self.decorate(move |tree, id| tree.set(id, Attribute::Transform(t)))
    }

    /// Fills the background with a [`LinearGradient`].
    fn gradient(self, g: LinearGradient) -> Decorated<Self, impl FnOnce(&mut Tree, WidgetId)> {
        self.decorate(move |tree, id| tree.set(id, Attribute::Gradient(g)))
    }

    // --- reactive paint modifiers (re-emit when their signals change) ---

    /// Reactive background: re-applies whenever the signals `f` reads change.
    fn background_fn(
        self,
        mut f: impl FnMut() -> Color + 'static,
    ) -> Decorated<Self, impl FnOnce(&mut Tree, WidgetId)> {
        self.decorate(move |t, id| t.bind(id, move || Attribute::BackgroundColor(f())))
    }

    /// Reactive transform (great for rotate/scale animations driven by
    /// `rax-anim` — e.g. a spinner or a press-to-scale effect).
    fn transform_fn(
        self,
        mut f: impl FnMut() -> Transform + 'static,
    ) -> Decorated<Self, impl FnOnce(&mut Tree, WidgetId)> {
        self.decorate(move |t, id| t.bind(id, move || Attribute::Transform(f())))
    }

    /// Reactive opacity (great for fade animations driven by `rax-anim`).
    fn opacity_fn(
        self,
        mut f: impl FnMut() -> f32 + 'static,
    ) -> Decorated<Self, impl FnOnce(&mut Tree, WidgetId)> {
        self.decorate(move |t, id| t.bind(id, move || Attribute::Opacity(f())))
    }

    /// Reactive text color.
    fn text_color_fn(
        self,
        mut f: impl FnMut() -> Color + 'static,
    ) -> Decorated<Self, impl FnOnce(&mut Tree, WidgetId)> {
        self.decorate(move |t, id| t.bind(id, move || Attribute::TextColor(f())))
    }

    // --- gesture modifiers (work on any view, not just buttons) ---

    /// Runs `f` when this view is tapped.
    fn on_tap(
        self,
        mut f: impl FnMut() + 'static,
    ) -> Decorated<Self, impl FnOnce(&mut Tree, WidgetId)> {
        self.decorate(move |t, id| {
            t.on(id, EventKind::Tap, move |_| f());
            t.enable_gesture(id, GestureKind::Tap);
        })
    }

    /// Runs `f` when this view is double-tapped.
    fn on_double_tap(
        self,
        mut f: impl FnMut() + 'static,
    ) -> Decorated<Self, impl FnOnce(&mut Tree, WidgetId)> {
        self.decorate(move |t, id| {
            t.on(id, EventKind::DoubleTap, move |_| f());
            t.enable_gesture(id, GestureKind::DoubleTap);
        })
    }

    /// Runs `f` on each update of a pan/drag gesture, passing the cumulative
    /// translation, velocity, and phase. Enables drag-to-move, swipe-to-dismiss,
    /// and gesture-driven animation.
    fn on_pan(
        self,
        mut f: impl FnMut(PanInfo) + 'static,
    ) -> Decorated<Self, impl FnOnce(&mut Tree, WidgetId)> {
        self.decorate(move |t, id| {
            t.on(id, EventKind::Pan, move |event| {
                if let rax_dom::Event::PanChanged {
                    translation,
                    velocity,
                    phase,
                    ..
                } = event
                {
                    f(PanInfo {
                        translation: *translation,
                        velocity: *velocity,
                        phase: *phase,
                    });
                }
            });
            t.enable_gesture(id, GestureKind::Pan);
        })
    }

    /// Runs `f` on each update of a pinch/scale gesture, passing the cumulative
    /// scale factor, velocity, and phase. Enables zoom-to-scale interactions and
    /// pinch-to-dismiss.
    fn on_pinch(
        self,
        mut f: impl FnMut(PinchInfo) + 'static,
    ) -> Decorated<Self, impl FnOnce(&mut Tree, WidgetId)> {
        self.decorate(move |t, id| {
            t.on(id, EventKind::Pinch, move |event| {
                if let rax_dom::Event::PinchChanged {
                    scale,
                    velocity,
                    phase,
                    ..
                } = event
                {
                    f(PinchInfo {
                        scale: *scale,
                        velocity: *velocity,
                        phase: *phase,
                    });
                }
            });
            t.enable_gesture(id, GestureKind::Pinch);
        })
    }

    /// Runs `f` on each update of a rotation gesture, passing the cumulative
    /// rotation in radians, velocity, and phase.
    fn on_rotate(
        self,
        mut f: impl FnMut(RotateInfo) + 'static,
    ) -> Decorated<Self, impl FnOnce(&mut Tree, WidgetId)> {
        self.decorate(move |t, id| {
            t.on(id, EventKind::RotateChanged, move |event| {
                if let rax_dom::Event::RotateChanged {
                    rotation,
                    velocity,
                    phase,
                    ..
                } = event
                {
                    f(RotateInfo {
                        rotation: *rotation,
                        velocity: *velocity,
                        phase: *phase,
                    });
                }
            });
            t.enable_gesture(id, GestureKind::Rotate);
        })
    }

    /// Runs `f` when this view is long-pressed.
    fn on_long_press(
        self,
        mut f: impl FnMut() + 'static,
    ) -> Decorated<Self, impl FnOnce(&mut Tree, WidgetId)> {
        self.decorate(move |t, id| {
            t.on(id, EventKind::LongPress, move |_| f());
            t.enable_gesture(id, GestureKind::LongPress);
        })
    }

    // --- accessibility ---

    /// Sets the screen-reader label for this view.
    fn accessibility_label(
        self,
        label: impl Into<String>,
    ) -> Decorated<Self, impl FnOnce(&mut Tree, WidgetId)> {
        let label = label.into();
        self.decorate(move |t, id| t.set(id, Attribute::AccessibilityLabel(label)))
    }

    /// Sets the accessibility hint — a brief phrase that describes the result
    /// of performing an action. VoiceOver reads this after the label.
    fn accessibility_hint(
        self,
        hint: impl Into<String>,
    ) -> Decorated<Self, impl FnOnce(&mut Tree, WidgetId)> {
        let hint = hint.into();
        self.decorate(move |t, id| t.set(id, Attribute::AccessibilityHint(hint)))
    }

    /// Sets the accessibility role (mapped to platform traits).
    fn role(self, role: Role) -> Decorated<Self, impl FnOnce(&mut Tree, WidgetId)> {
        self.decorate(move |t, id| t.set(id, Attribute::AccessibilityRole(role)))
    }

    // --- conditional style helpers ---

    /// Reduce opacity to 0.4 when `is_disabled()` returns `true`.
    ///
    /// Reactive: re-evaluates whenever the signals read inside `is_disabled` change.
    ///
    /// # Example
    /// ```no_run
    /// button("Save", on_save).disabled_opacity(move || is_saving.get())
    /// ```
    fn disabled_opacity(
        self,
        is_disabled: impl FnMut() -> bool + 'static,
    ) -> Decorated<Self, impl FnOnce(&mut Tree, WidgetId)> {
        let mut is_disabled = is_disabled;
        self.opacity_fn(move || if is_disabled() { 0.4 } else { 1.0 })
    }

    /// Make this view fully visible (`opacity = 1.0`) when `condition()` is `true`,
    /// and invisible (`opacity = 0.0`) — but still laid-out — when it is `false`.
    ///
    /// Reactive: re-evaluates whenever the signals read inside `condition` change.
    ///
    /// # Example
    /// ```no_run
    /// error_label.visible_when(move || has_error.get())
    /// ```
    fn visible_when(
        self,
        condition: impl FnMut() -> bool + 'static,
    ) -> Decorated<Self, impl FnOnce(&mut Tree, WidgetId)> {
        let mut condition = condition;
        self.opacity_fn(move || if condition() { 1.0 } else { 0.0 })
    }

    /// Make this view invisible (`opacity = 0.0`) — but still laid-out — when
    /// `condition()` is `true`, and fully visible when it is `false`.
    ///
    /// This is the complement of [`visible_when`](ViewExt::visible_when).
    ///
    /// Reactive: re-evaluates whenever the signals read inside `condition` change.
    ///
    /// # Example
    /// ```no_run
    /// placeholder_text.hidden_when(move || has_content.get())
    /// ```
    fn hidden_when(
        self,
        condition: impl FnMut() -> bool + 'static,
    ) -> Decorated<Self, impl FnOnce(&mut Tree, WidgetId)> {
        let mut condition = condition;
        self.opacity_fn(move || if condition() { 0.0 } else { 1.0 })
    }

    /// Hides or shows this element from assistive technologies. Set `true` for
    /// decorative elements that add no semantic information.
    fn accessibility_hidden(
        self,
        hidden: bool,
    ) -> Decorated<Self, impl FnOnce(&mut Tree, WidgetId)> {
        self.decorate(move |t, id| t.set(id, Attribute::AccessibilityHidden(hidden)))
    }

    /// Reactively marks this element as selected (e.g. a selected list row,
    /// tab, or chip). Re-applies whenever the signal changes.
    fn accessibility_selected(
        self,
        signal: impl Fn() -> bool + 'static,
    ) -> Decorated<Self, impl FnOnce(&mut Tree, WidgetId)> {
        self.decorate(move |t, id| t.bind(id, move || Attribute::AccessibilitySelected(signal())))
    }

    /// Reactively marks this element as disabled. Re-applies whenever the
    /// signal changes.
    fn accessibility_disabled(
        self,
        signal: impl Fn() -> bool + 'static,
    ) -> Decorated<Self, impl FnOnce(&mut Tree, WidgetId)> {
        self.decorate(move |t, id| t.bind(id, move || Attribute::AccessibilityDisabled(signal())))
    }

    /// Reactively marks this element as expanded (e.g. an accordion header).
    /// Re-applies whenever the signal changes.
    fn accessibility_expanded(
        self,
        signal: impl Fn() -> bool + 'static,
    ) -> Decorated<Self, impl FnOnce(&mut Tree, WidgetId)> {
        self.decorate(move |t, id| t.bind(id, move || Attribute::AccessibilityExpanded(signal())))
    }

    /// Reactively marks this element as busy / updating content. Re-applies
    /// whenever the signal changes.
    fn accessibility_busy(
        self,
        signal: impl Fn() -> bool + 'static,
    ) -> Decorated<Self, impl FnOnce(&mut Tree, WidgetId)> {
        self.decorate(move |t, id| t.bind(id, move || Attribute::AccessibilityBusy(signal())))
    }

    /// Expands the touchable hit area beyond the view's visual bounds by the
    /// given insets (in points). Useful for small tap targets.
    fn hit_slop(
        self,
        top: f32,
        right: f32,
        bottom: f32,
        left: f32,
    ) -> Decorated<Self, impl FnOnce(&mut Tree, WidgetId)> {
        self.decorate(move |t, id| t.set(id, Attribute::HitSlop { top, right, bottom, left }))
    }

    /// Sets the layout direction for this view and all of its descendants.
    ///
    /// Use [`LayoutDirection::Rtl`] for right-to-left locales (Arabic, Hebrew,
    /// Persian, …) to flip text direction and child ordering automatically.
    fn direction(
        self,
        dir: LayoutDirection,
    ) -> Decorated<Self, impl FnOnce(&mut Tree, WidgetId)> {
        self.decorate(move |t, id| t.set(id, Attribute::Direction(dir)))
    }
}

impl<V: View> ViewExt for V {}
