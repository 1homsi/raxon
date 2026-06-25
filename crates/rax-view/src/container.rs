//! Flex containers: `column` (vertical) and `row` (horizontal), with layout and
//! paint modifiers. Children are a [`ViewSequence`] (a tuple).

use rax_core::{Color, EdgeInsets, FlexDirection, LayoutStyle};
use rax_dom::{Attribute, Tree, WidgetId};

use crate::view::{View, ViewSequence};

/// A flex container view. Build via [`column`] or [`row`].
pub struct Container<C: ViewSequence> {
    direction: FlexDirection,
    children: C,
    padding: EdgeInsets,
    gap: f32,
    background: Option<Color>,
}

fn container<C: ViewSequence>(direction: FlexDirection, children: C) -> Container<C> {
    Container {
        direction,
        children,
        padding: EdgeInsets::ZERO,
        gap: 0.0,
        background: None,
    }
}

/// A vertically-stacked container.
pub fn column<C: ViewSequence>(children: C) -> Container<C> {
    container(FlexDirection::Column, children)
}

/// A horizontally-stacked container.
pub fn row<C: ViewSequence>(children: C) -> Container<C> {
    container(FlexDirection::Row, children)
}

impl<C: ViewSequence> Container<C> {
    /// Uniform padding on all edges.
    #[must_use]
    pub fn padding(mut self, value: f32) -> Self {
        self.padding = EdgeInsets::all(value);
        self
    }

    /// Explicit per-edge padding.
    #[must_use]
    pub fn padding_insets(mut self, insets: EdgeInsets) -> Self {
        self.padding = insets;
        self
    }

    /// Spacing between children along the primary axis.
    #[must_use]
    pub fn gap(mut self, value: f32) -> Self {
        self.gap = value;
        self
    }

    /// Background fill color.
    #[must_use]
    pub fn background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }
}

impl<C: ViewSequence> View for Container<C> {
    fn build(self, tree: &mut Tree) -> WidgetId {
        let id = tree.create_view();
        tree.set_style(
            id,
            LayoutStyle {
                direction: self.direction,
                padding: self.padding,
                gap: self.gap,
                ..LayoutStyle::default()
            },
        );
        if let Some(background) = self.background {
            tree.set(id, Attribute::BackgroundColor(background));
        }
        self.children.build_into(tree, id);
        id
    }
}
