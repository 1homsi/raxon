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
    use rax_core::AlignItems as A;
    Style {
        display: Display::Flex,
        flex_direction: match style.direction {
            FlexDirection::Row => taffy::FlexDirection::Row,
            FlexDirection::Column => taffy::FlexDirection::Column,
        },
        align_items: Some(match style.align_items {
            A::Stretch => taffy::AlignItems::Stretch,
            A::Start => taffy::AlignItems::FlexStart,
            A::Center => taffy::AlignItems::Center,
            A::End => taffy::AlignItems::FlexEnd,
        }),
        padding: taffy::Rect {
            left: length(style.padding.left),
            right: length(style.padding.right),
            top: length(style.padding.top),
            bottom: length(style.padding.bottom),
        },
        margin: taffy::Rect {
            left: length(style.margin.left),
            right: length(style.margin.right),
            top: length(style.margin.top),
            bottom: length(style.margin.bottom),
        },
        gap: taffy::Size {
            width: length(style.gap),
            height: length(style.gap),
        },
        flex_grow: style.flex_grow,
        size: taffy::Size {
            width: to_dim(style.width),
            height: to_dim(style.height),
        },
        ..Style::DEFAULT
    }
}

fn to_dim(d: rax_core::Dimension) -> Dimension {
    match d {
        rax_core::Dimension::Auto => auto(),
        rax_core::Dimension::Points(p) => length(p),
    }
}

/// Heights for leaves; cross-axis width is left to the container's stretch.
fn measure_leaf(
    known: taffy::Size<Option<f32>>,
    context: Option<&mut LeafContext>,
) -> taffy::Size<f32> {
    let Some(context) = context else {
        return taffy::Size::ZERO;
    };
    let height = match context.kind {
        WidgetKind::Button => 44.0,
        WidgetKind::Text => (context.font_size * 1.4).ceil(),
        WidgetKind::View => 0.0,
    };
    taffy::Size {
        width: known.width.unwrap_or(0.0),
        height,
    }
}

#[cfg(test)]
mod tests;
