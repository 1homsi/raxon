//! Backend-agnostic layout style: the *inputs* to the layout engine.
//!
//! These are retained on each element (unlike paint attributes, which are
//! forwarded to the backend and forgotten). `rax-layout` maps this neutral type
//! onto `taffy`, so the rest of the framework never depends on the layout
//! engine's own types — we can swap engines without touching the public API.

use crate::EdgeInsets;

/// Primary axis along which a container lays out its children.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlexDirection {
    /// Left-to-right (a `row`).
    Row,
    /// Top-to-bottom (a `column`).
    #[default]
    Column,
}

/// How children align on the cross axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlignItems {
    /// Stretch to fill the cross axis (the default — labels span the column).
    #[default]
    Stretch,
    /// Pack at the start of the cross axis.
    Start,
    /// Center on the cross axis.
    Center,
    /// Pack at the end of the cross axis.
    End,
}

/// A length: either automatic (content/▢-derived) or a fixed number of points.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Dimension {
    /// Sized by content / flex rules.
    #[default]
    Auto,
    /// A fixed length in logical points.
    Points(f32),
}

/// The retained layout inputs for one element.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutStyle {
    /// Primary layout axis (for containers).
    pub direction: FlexDirection,
    /// Cross-axis alignment of children.
    pub align_items: AlignItems,
    /// Inner padding.
    pub padding: EdgeInsets,
    /// Outer margin.
    pub margin: EdgeInsets,
    /// Spacing between children along the primary axis.
    pub gap: f32,
    /// Flex grow factor (share of free space).
    pub flex_grow: f32,
    /// Explicit width, if any.
    pub width: Dimension,
    /// Explicit height, if any.
    pub height: Dimension,
}

impl Default for LayoutStyle {
    fn default() -> Self {
        LayoutStyle {
            direction: FlexDirection::default(),
            align_items: AlignItems::default(),
            padding: EdgeInsets::ZERO,
            margin: EdgeInsets::ZERO,
            gap: 0.0,
            flex_grow: 0.0,
            width: Dimension::Auto,
            height: Dimension::Auto,
        }
    }
}
