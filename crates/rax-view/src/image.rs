//! The `image` view (asset or system-symbol source, optional tint).

use rax_core::Color;
use rax_dom::{Attribute, Tree, WidgetId};

use crate::view::View;

/// An image view. Build via [`image`].
pub struct Image {
    source: String,
    tint: Option<Color>,
}

/// Creates an image from an asset name or a system-symbol name.
pub fn image(source: impl Into<String>) -> Image {
    Image {
        source: source.into(),
        tint: None,
    }
}

impl Image {
    /// Tints a template/symbol image with `color`.
    #[must_use]
    pub fn tint(mut self, color: Color) -> Self {
        self.tint = Some(color);
        self
    }
}

impl View for Image {
    fn build(self, tree: &mut Tree) -> WidgetId {
        let id = tree.create_image();
        tree.set(id, Attribute::ImageSource(self.source));
        if let Some(tint) = self.tint {
            tree.set(id, Attribute::TintColor(tint));
        }
        id
    }
}
