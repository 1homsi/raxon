//! The `scroll` view: a scrolling container (vertical or horizontal),
//! optionally with pull-to-refresh.

use std::sync::Arc;

use rax_core::{FlexDirection, LayoutStyle};
use rax_dom::{Attribute, Callback, Event, EventKind, KeyboardDismissMode, ScrollCallback, ScrollInfo, Tree, WidgetId};

use crate::view::View;

/// A scroll container wrapping a single child (usually a column or row).
/// Build via [`scroll`].
pub struct Scroll<V> {
    child: V,
    grow: f32,
    horizontal: bool,
    refreshing: Option<bool>,
    on_refresh: Option<Box<dyn FnMut()>>,
    scroll_enabled: Option<bool>,
    shows_indicator: Option<bool>,
    paging: bool,
    content_inset: Option<(f32, f32, f32, f32)>,
    on_scroll: Option<ScrollCallback>,
    on_scroll_begin: Option<Callback>,
    on_scroll_end: Option<Callback>,
    keyboard_dismiss_mode: Option<KeyboardDismissMode>,
}

/// Wraps `child` in a vertically-scrolling container that fills its parent.
pub fn scroll<V: View>(child: V) -> Scroll<V> {
    Scroll {
        child,
        grow: 1.0,
        horizontal: false,
        refreshing: None,
        on_refresh: None,
        scroll_enabled: None,
        shows_indicator: None,
        paging: false,
        content_inset: None,
        on_scroll: None,
        on_scroll_begin: None,
        on_scroll_end: None,
        keyboard_dismiss_mode: None,
    }
}

impl<V: View> Scroll<V> {
    /// Sets the flex-grow factor of the scroll container (default `1.0`).
    #[must_use]
    pub fn grow(mut self, factor: f32) -> Self {
        self.grow = factor;
        self
    }

    /// Makes this a horizontal scroll view (content lays out in a row).
    #[must_use]
    pub fn horizontal(mut self) -> Self {
        self.horizontal = true;
        self
    }

    /// Enables pull-to-refresh. `is_refreshing` controls the spinner visibility;
    /// `on_refresh` is called when the user pulls to refresh.
    #[must_use]
    pub fn refreshable(mut self, is_refreshing: bool, on_refresh: impl FnMut() + 'static) -> Self {
        self.refreshing = Some(is_refreshing);
        self.on_refresh = Some(Box::new(on_refresh));
        self
    }

    /// Enable or disable scrolling (`UIScrollView.isScrollEnabled`).
    #[must_use]
    pub fn scroll_enabled(mut self, enabled: bool) -> Self {
        self.scroll_enabled = Some(enabled);
        self
    }

    /// Show or hide the scroll indicator (`UIScrollView.shows{Horizontal,Vertical}ScrollIndicator`).
    #[must_use]
    pub fn shows_indicator(mut self, show: bool) -> Self {
        self.shows_indicator = Some(show);
        self
    }

    /// Enable paged scrolling — the scroll view snaps to page boundaries
    /// (`UIScrollView.isPagingEnabled`). Ideal for carousel layouts.
    #[must_use]
    pub fn paging(mut self) -> Self {
        self.paging = true;
        self
    }

    /// Set the content inset (padding inside the scroll area, in points).
    #[must_use]
    pub fn content_inset(mut self, top: f32, right: f32, bottom: f32, left: f32) -> Self {
        self.content_inset = Some((top, right, bottom, left));
        self
    }

    /// Register a callback that fires continuously while the user scrolls,
    /// reporting the current content offset and estimated velocity.
    #[must_use]
    pub fn on_scroll(mut self, f: impl Fn(ScrollInfo) + Send + Sync + 'static) -> Self {
        self.on_scroll = Some(ScrollCallback(Arc::new(f)));
        self
    }

    /// Register a callback that fires when the user begins dragging the scroll view.
    #[must_use]
    pub fn on_scroll_begin(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_scroll_begin = Some(Callback(Arc::new(f)));
        self
    }

    /// Register a callback that fires when the scroll view comes to rest.
    #[must_use]
    pub fn on_scroll_end(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_scroll_end = Some(Callback(Arc::new(f)));
        self
    }

    /// Set how the keyboard is dismissed when the user drags this scroll view.
    /// Maps to `UIScrollView.keyboardDismissMode` on iOS.
    #[must_use]
    pub fn keyboard_dismiss_mode(mut self, mode: KeyboardDismissMode) -> Self {
        self.keyboard_dismiss_mode = Some(mode);
        self
    }
}

impl<V: View> View for Scroll<V> {
    fn build(self, tree: &mut Tree) -> WidgetId {
        let id = tree.create_scroll();
        tree.set_style(
            id,
            LayoutStyle {
                scroll: true,
                flex_grow: self.grow,
                direction: if self.horizontal {
                    FlexDirection::Row
                } else {
                    FlexDirection::Column
                },
                ..LayoutStyle::default()
            },
        );
        if self.horizontal {
            tree.set(id, Attribute::Horizontal(true));
        }
        if let Some(refreshing) = self.refreshing {
            tree.set(id, Attribute::Refreshing(refreshing));
            if let Some(mut on_refresh) = self.on_refresh {
                tree.on(id, EventKind::Refresh, move |event| {
                    if matches!(event, Event::Refresh { .. }) {
                        on_refresh();
                    }
                });
            }
        }
        if let Some(enabled) = self.scroll_enabled {
            tree.set(id, Attribute::ScrollEnabled(enabled));
        }
        if let Some(show) = self.shows_indicator {
            tree.set(id, Attribute::ShowsScrollIndicator(show));
        }
        if self.paging {
            tree.set(id, Attribute::PagingEnabled(true));
        }
        if let Some((top, right, bottom, left)) = self.content_inset {
            tree.set(id, Attribute::ContentInset { top, right, bottom, left });
        }
        if let Some(cb) = self.on_scroll {
            tree.set(id, Attribute::OnScrollChange(cb));
        }
        if let Some(cb) = self.on_scroll_begin {
            tree.set(id, Attribute::OnScrollBegin(cb));
        }
        if let Some(cb) = self.on_scroll_end {
            tree.set(id, Attribute::OnScrollEnd(cb));
        }
        if let Some(mode) = self.keyboard_dismiss_mode {
            tree.set(id, Attribute::KeyboardDismissMode(mode));
        }
        let child = self.child.build(tree);
        tree.append(id, child);
        id
    }
}
