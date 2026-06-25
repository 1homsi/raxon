//! Small composed widgets built from primitives (no new backend support).

use rax_core::Color;

use crate::container::column;
use crate::modifier::ViewExt;
use crate::view::View;

/// A 1pt horizontal divider line in `color`.
pub fn divider(color: Color) -> impl View {
    column(()).height(1.0).background(color)
}

/// A vertical divider of the given `width` in `color`.
pub fn vertical_divider(color: Color, width: f32) -> impl View {
    column(()).width(width).background(color)
}
