//! Flex containers: `column` (vertical) and `row` (horizontal), with layout and
//! paint modifiers. Children are a [`ViewSequence`] (a tuple).

use rax_core::{
    AlignItems, Color, EdgeInsets, FlexDirection, FlexWrap, JustifyContent, LayoutStyle,
};
use rax_dom::{Attribute, Tree, WidgetId};

use crate::view::{View, ViewSequence};

/// A flex container view. Build via [`column`] or [`row`].
pub struct Container<C: ViewSequence> {
    direction: FlexDirection,
    children: C,
    padding: EdgeInsets,
    gap: f32,
    flex_grow: f32,
    align: AlignItems,
    justify: JustifyContent,
    wrap: FlexWrap,
    background: Option<Color>,
    corner_radius: Option<f32>,
}

fn container<C: ViewSequence>(direction: FlexDirection, children: C) -> Container<C> {
    Container {
        direction,
        children,
        padding: EdgeInsets::ZERO,
        gap: 0.0,
        flex_grow: 0.0,
        align: AlignItems::Stretch,
        justify: JustifyContent::Start,
        wrap: FlexWrap::NoWrap,
        background: None,
        corner_radius: None,
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

    /// Makes this container expand to fill available space (flex-grow `1.0`).
    #[must_use]
    pub fn grow(mut self) -> Self {
        self.flex_grow = 1.0;
        self
    }

    /// Sets an explicit flex-grow factor.
    #[must_use]
    pub fn grow_by(mut self, factor: f32) -> Self {
        self.flex_grow = factor;
        self
    }

    /// Sets cross-axis alignment of children.
    #[must_use]
    pub fn align(mut self, align: AlignItems) -> Self {
        self.align = align;
        self
    }

    /// Sets main-axis distribution of children.
    #[must_use]
    pub fn justify(mut self, justify: JustifyContent) -> Self {
        self.justify = justify;
        self
    }

    /// Enables wrapping of children onto multiple lines.
    #[must_use]
    pub fn wrap(mut self) -> Self {
        self.wrap = FlexWrap::Wrap;
        self
    }

    /// Background fill color.
    #[must_use]
    pub fn background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }

    /// Rounds the container's corners by `radius` points.
    #[must_use]
    pub fn corner_radius(mut self, radius: f32) -> Self {
        self.corner_radius = Some(radius);
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
                align_items: self.align,
                justify_content: self.justify,
                wrap: self.wrap,
                padding: self.padding,
                gap: self.gap,
                flex_grow: self.flex_grow,
                ..LayoutStyle::default()
            },
        );
        if let Some(background) = self.background {
            tree.set(id, Attribute::BackgroundColor(background));
        }
        if let Some(radius) = self.corner_radius {
            tree.set(id, Attribute::CornerRadius(radius));
        }
        self.children.build_into(tree, id);
        id
    }
}
