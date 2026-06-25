//! Universal layout modifiers via [`ViewExt`]. Any view can be sized, grown,
//! margined, aligned, or positioned by wrapping it in a [`Styled`] that overrides
//! specific [`LayoutStyle`] fields after the inner view builds. This is a core
//! customizability mechanism: layout control on *every* view, not just containers.

use rax_core::{AlignItems, Dimension, EdgeInsets, LayoutStyle, Position};
use rax_dom::{Tree, WidgetId};

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
}

impl<V: View> ViewExt for V {}
