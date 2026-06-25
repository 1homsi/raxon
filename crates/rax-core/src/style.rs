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
    /// Align by text baseline.
    Baseline,
}

/// How children are distributed along the main axis (free space handling).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JustifyContent {
    /// Pack at the start.
    #[default]
    Start,
    /// Center.
    Center,
    /// Pack at the end.
    End,
    /// First/last flush to edges, equal space between.
    SpaceBetween,
    /// Equal space around each child.
    SpaceAround,
    /// Equal space between and at edges.
    SpaceEvenly,
}

/// Whether children wrap onto multiple lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlexWrap {
    /// Single line, may overflow.
    #[default]
    NoWrap,
    /// Wrap onto multiple lines.
    Wrap,
    /// Wrap, reversed cross-axis order.
    WrapReverse,
}

/// Box positioning scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Position {
    /// In normal flow.
    #[default]
    Relative,
    /// Out of flow, positioned by `inset` relative to the nearest container.
    Absolute,
}

/// A length: automatic, a fixed number of points, or a percentage of the parent.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Dimension {
    /// Sized by content / flex rules.
    #[default]
    Auto,
    /// A fixed length in logical points.
    Points(f32),
    /// A percentage (`0.0..=100.0`) of the parent's corresponding dimension.
    Percent(f32),
}

/// The retained layout inputs for one element. Mirrors the CSS flexbox model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutStyle {
    /// Positioning scheme.
    pub position: Position,
    /// Primary layout axis (for containers).
    pub direction: FlexDirection,
    /// Whether children wrap.
    pub wrap: FlexWrap,
    /// Cross-axis alignment of children.
    pub align_items: AlignItems,
    /// Override cross-axis alignment for *this* element within its parent.
    pub align_self: Option<AlignItems>,
    /// Main-axis distribution of children.
    pub justify_content: JustifyContent,
    /// Inner padding.
    pub padding: EdgeInsets,
    /// Outer margin.
    pub margin: EdgeInsets,
    /// Offsets for `Position::Absolute` (and relative nudging).
    pub inset: EdgeInsets,
    /// Spacing between children along both axes.
    pub gap: f32,
    /// Flex grow factor (share of free space).
    pub flex_grow: f32,
    /// Flex shrink factor.
    pub flex_shrink: f32,
    /// Flex basis (main-axis starting size).
    pub flex_basis: Dimension,
    /// Explicit width.
    pub width: Dimension,
    /// Explicit height.
    pub height: Dimension,
    /// Minimum width.
    pub min_width: Dimension,
    /// Minimum height.
    pub min_height: Dimension,
    /// Maximum width.
    pub max_width: Dimension,
    /// Maximum height.
    pub max_height: Dimension,
    /// Width/height ratio, if constrained.
    pub aspect_ratio: Option<f32>,
}

impl Default for LayoutStyle {
    fn default() -> Self {
        LayoutStyle {
            position: Position::Relative,
            direction: FlexDirection::default(),
            wrap: FlexWrap::NoWrap,
            align_items: AlignItems::default(),
            align_self: None,
            justify_content: JustifyContent::default(),
            padding: EdgeInsets::ZERO,
            margin: EdgeInsets::ZERO,
            inset: EdgeInsets::ZERO,
            gap: 0.0,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: Dimension::Auto,
            width: Dimension::Auto,
            height: Dimension::Auto,
            min_width: Dimension::Auto,
            min_height: Dimension::Auto,
            max_width: Dimension::Auto,
            max_height: Dimension::Auto,
            aspect_ratio: None,
        }
    }
}
