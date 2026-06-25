//! Flexbox layout for `rax`, wrapping [`taffy`].
//!
//! The engine stores a neutral [`LayoutStyle`](rax_core::LayoutStyle) on each
//! element; this crate is the *only* place that knows about taffy. [`compute`]
//! mirrors the element tree into a taffy tree, runs layout against an available
//! size, and returns each widget's frame **relative to its parent** (which is
//! exactly what native subview frames expect).
//!
//! Leaf measurement is intentionally simple for now: text/button heights derive
//! from font size, and the cross-axis size is stretched by the container. Exact
//! text measurement requires a platform round-trip and lands with the backends.

#![forbid(unsafe_code)]

use rax_core::{FlexDirection, LayoutStyle, Rect, Size};
use rax_dom::{Tree, WidgetId, WidgetKind};
use taffy::prelude::*;

/// Per-leaf context handed to taffy's measure function.
struct LeafContext {
    kind: WidgetKind,
    font_size: f32,
    text: Option<String>,
}

/// Computes layout for the subtree rooted at `root`, filling `available`.
///
/// Returns `(WidgetId, frame)` for every widget in the subtree, with each frame
/// expressed in its parent's coordinate space.
pub fn compute(tree: &Tree, root: WidgetId, available: Size) -> Vec<(WidgetId, Rect)> {
    let mut taffy: TaffyTree<LeafContext> = TaffyTree::new();
    let mut mapping: Vec<(NodeId, WidgetId)> = Vec::new();

    let root_node = build_node(&mut taffy, tree, root, &mut mapping);

    // Force the root to exactly fill the available space.
    if let Ok(style) = taffy.style(root_node).cloned() {
        let _ = taffy.set_style(
            root_node,
            Style {
                size: taffy::Size {
                    width: length(available.width),
                    height: length(available.height),
                },
                ..style
            },
        );
    }

    let space = taffy::Size {
        width: AvailableSpace::Definite(available.width),
        height: AvailableSpace::Definite(available.height),
    };
    let _ = taffy.compute_layout_with_measure(
        root_node,
        space,
        |known, _available, _node, context, _style| measure_leaf(known, context),
    );

    mapping
        .iter()
        .filter_map(|(node, id)| {
            taffy.layout(*node).ok().map(|layout| {
                (
                    *id,
                    Rect::new(
                        layout.location.x,
                        layout.location.y,
                        layout.size.width,
                        layout.size.height,
                    ),
                )
            })
        })
        .collect()
}

/// Recursively mirrors `id` and its descendants into the taffy tree, recording
/// the node↔widget mapping.
fn build_node(
    taffy: &mut TaffyTree<LeafContext>,
    tree: &Tree,
    id: WidgetId,
    mapping: &mut Vec<(NodeId, WidgetId)>,
) -> NodeId {
    let style = to_taffy_style(tree.style_of(id).unwrap_or_default());
    let children = tree.children_of(id);

    let node = if children.is_empty() {
        let context = LeafContext {
            kind: tree.kind_of(id).unwrap_or(WidgetKind::View),
            font_size: tree.font_size_of(id),
            text: tree.measure_text_of(id),
        };
        taffy
            .new_leaf_with_context(style, context)
            .expect("taffy leaf")
    } else {
        let child_nodes: Vec<NodeId> = children
            .iter()
            .map(|child| build_node(taffy, tree, *child, mapping))
            .collect();
        taffy
            .new_with_children(style, &child_nodes)
            .expect("taffy node")
    };

    mapping.push((node, id));
    node
}

/// Maps our neutral style onto taffy's flex style.
fn to_taffy_style(style: LayoutStyle) -> Style {
    Style {
        display: Display::Flex,
        position: match style.position {
            rax_core::Position::Relative => taffy::Position::Relative,
            rax_core::Position::Absolute => taffy::Position::Absolute,
        },
        flex_direction: match style.direction {
            FlexDirection::Row => taffy::FlexDirection::Row,
            FlexDirection::Column => taffy::FlexDirection::Column,
        },
        flex_wrap: match style.wrap {
            rax_core::FlexWrap::NoWrap => taffy::FlexWrap::NoWrap,
            rax_core::FlexWrap::Wrap => taffy::FlexWrap::Wrap,
            rax_core::FlexWrap::WrapReverse => taffy::FlexWrap::WrapReverse,
        },
        align_items: Some(to_align(style.align_items)),
        align_self: style.align_self.map(to_align),
        justify_content: Some(to_justify(style.justify_content)),
        padding: to_lp_rect(style.padding),
        margin: taffy::Rect {
            left: length(style.margin.left),
            right: length(style.margin.right),
            top: length(style.margin.top),
            bottom: length(style.margin.bottom),
        },
        inset: taffy::Rect {
            left: length(style.inset.left),
            right: length(style.inset.right),
            top: length(style.inset.top),
            bottom: length(style.inset.bottom),
        },
        gap: taffy::Size {
            width: length(style.gap),
            height: length(style.gap),
        },
        flex_grow: style.flex_grow,
        flex_shrink: style.flex_shrink,
        flex_basis: to_dim(style.flex_basis),
        size: taffy::Size {
            width: to_dim(style.width),
            height: to_dim(style.height),
        },
        min_size: taffy::Size {
            width: to_dim(style.min_width),
            height: to_dim(style.min_height),
        },
        max_size: taffy::Size {
            width: to_dim(style.max_width),
            height: to_dim(style.max_height),
        },
        aspect_ratio: style.aspect_ratio,
        ..Style::DEFAULT
    }
}

fn to_align(a: rax_core::AlignItems) -> taffy::AlignItems {
    use rax_core::AlignItems as A;
    match a {
        A::Stretch => taffy::AlignItems::Stretch,
        A::Start => taffy::AlignItems::FlexStart,
        A::Center => taffy::AlignItems::Center,
        A::End => taffy::AlignItems::FlexEnd,
        A::Baseline => taffy::AlignItems::Baseline,
    }
}

fn to_justify(j: rax_core::JustifyContent) -> taffy::JustifyContent {
    use rax_core::JustifyContent as J;
    match j {
        J::Start => taffy::JustifyContent::FlexStart,
        J::Center => taffy::JustifyContent::Center,
        J::End => taffy::JustifyContent::FlexEnd,
        J::SpaceBetween => taffy::JustifyContent::SpaceBetween,
        J::SpaceAround => taffy::JustifyContent::SpaceAround,
        J::SpaceEvenly => taffy::JustifyContent::SpaceEvenly,
    }
}

fn to_lp_rect(e: rax_core::EdgeInsets) -> taffy::Rect<LengthPercentage> {
    taffy::Rect {
        left: length(e.left),
        right: length(e.right),
        top: length(e.top),
        bottom: length(e.bottom),
    }
}

fn to_dim(d: rax_core::Dimension) -> Dimension {
    match d {
        rax_core::Dimension::Auto => auto(),
        rax_core::Dimension::Points(p) => length(p),
        rax_core::Dimension::Percent(p) => percent(p / 100.0),
    }
}

/// Intrinsic size for a leaf, estimated from its text and font size. A known
/// (stretched) dimension always wins; otherwise we fall back to the content
/// estimate — which is what makes rows and centered content lay out correctly.
///
/// This is a heuristic (average glyph advance ≈ 0.6em). Pixel-accurate text
/// measurement needs a platform round-trip and lands with richer text support.
fn measure_leaf(
    known: taffy::Size<Option<f32>>,
    context: Option<&mut LeafContext>,
) -> taffy::Size<f32> {
    let Some(context) = context else {
        return taffy::Size::ZERO;
    };
    let glyphs = context
        .text
        .as_ref()
        .map(|t| t.chars().count())
        .unwrap_or(0) as f32;
    // Generous average glyph advance so labels don't truncate; pixel-accurate
    // measurement is a platform round-trip we defer.
    let glyph_w = context.font_size * 0.62;
    let line_h = (context.font_size * 1.35).ceil();

    let (content_w, content_h) = match context.kind {
        // Buttons add horizontal title padding and have a minimum tap height.
        WidgetKind::Button => (glyphs * glyph_w + 36.0, line_h.max(44.0)),
        WidgetKind::Text => (glyphs * glyph_w + 6.0, line_h),
        WidgetKind::View => (0.0, 0.0),
    };

    taffy::Size {
        width: known.width.unwrap_or(content_w),
        height: known.height.unwrap_or(content_h),
    }
}

#[cfg(test)]
mod tests;
