//! Universal layout modifiers via [`ViewExt`]. Any view can be sized, grown,
//! margined, aligned, or positioned by wrapping it in a [`Styled`] that overrides
//! specific [`LayoutStyle`] fields after the inner view builds. This is a core
//! customizability mechanism: layout control on *every* view, not just containers.

use rax_core::{AlignItems, Color, Dimension, EdgeInsets, LayoutStyle, Position};
use rax_dom::{Attribute, EventKind, GestureKind, Role, Shadow, Tree, WidgetId};

use crate::view::View;

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

    // --- reactive paint modifiers (re-emit when their signals change) ---

    /// Reactive background: re-applies whenever the signals `f` reads change.
    fn background_fn(
        self,
        mut f: impl FnMut() -> Color + 'static,
    ) -> Decorated<Self, impl FnOnce(&mut Tree, WidgetId)> {
        self.decorate(move |t, id| t.bind(id, move || Attribute::BackgroundColor(f())))
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

    /// Sets the accessibility role (mapped to platform traits).
    fn role(self, role: Role) -> Decorated<Self, impl FnOnce(&mut Tree, WidgetId)> {
        self.decorate(move |t, id| t.set(id, Attribute::AccessibilityRole(role)))
    }
}

impl<V: View> ViewExt for V {}
