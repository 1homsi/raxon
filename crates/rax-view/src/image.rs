//! The `image` view (asset or system-symbol source, optional tint).

use std::sync::Arc;

use rax_core::Color;
use rax_dom::{Attribute, ImageErrorCallback, ImageLoadCallback, ImageResizeMode, Tree, WidgetId};

use crate::view::View;

/// An image view. Build via [`image`].
pub struct Image {
    source: String,
    tint: Option<Color>,
    data: Option<Arc<Vec<u8>>>,
    resize_mode: Option<ImageResizeMode>,
    on_load: Option<ImageLoadCallback>,
    on_error: Option<ImageErrorCallback>,
}

/// Creates an image from an asset name or a system-symbol name.
pub fn image(source: impl Into<String>) -> Image {
    Image {
        source: source.into(),
        tint: None,
        data: None,
        resize_mode: None,
        on_load: None,
        on_error: None,
    }
}

/// Creates a vector icon by name (e.g. an SF Symbol such as `"gearshape.fill"`).
/// Same as [`image`], but reads as an icon at the call site; size it with
/// `.size(..)` and color it with `.tint(..)`.
pub fn icon(name: impl Into<String>) -> Image {
    image(name)
}

impl Image {
    /// Tints a template/symbol image with `color`.
    #[must_use]
    pub fn tint(mut self, color: Color) -> Self {
        self.tint = Some(color);
        self
    }

    /// Sets a raw image from bytes (PNG/JPEG). Takes precedence over `src`.
    #[must_use]
    pub fn data(mut self, bytes: Arc<Vec<u8>>) -> Self {
        self.data = Some(bytes);
        self
    }

    /// Controls how the image is scaled/positioned within its bounds.
    ///
    /// Maps to `UIView.contentMode` on iOS.
    #[must_use]
    pub fn resize_mode(mut self, mode: ImageResizeMode) -> Self {
        self.resize_mode = Some(mode);
        self
    }

    /// Called when the image loads successfully.
    ///
    /// iOS: stub — TODO: wire up via image-load observer pattern.
    #[must_use]
    pub fn on_load(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_load = Some(ImageLoadCallback(Arc::new(f)));
        self
    }

    /// Called when the image fails to load. The argument is a short error description.
    ///
    /// iOS: stub — TODO: wire up via image-load observer pattern.
    #[must_use]
    pub fn on_error(mut self, f: impl Fn(String) + Send + Sync + 'static) -> Self {
        self.on_error = Some(ImageErrorCallback(Arc::new(f)));
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
        if let Some(bytes) = self.data {
            tree.set(id, Attribute::ImageData(bytes));
        }
        if let Some(mode) = self.resize_mode {
            tree.set(id, Attribute::ImageResizeMode(mode));
        }
        if let Some(cb) = self.on_load {
            tree.set(id, Attribute::ImageOnLoad(cb));
        }
        if let Some(cb) = self.on_error {
            tree.set(id, Attribute::ImageOnError(cb));
        }
        id
    }
}
