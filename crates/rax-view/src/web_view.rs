//! Embedded web view component backed by WKWebView on iOS.

use rax_dom::{Attribute, Tree, WidgetId};

use crate::view::View;

/// An embedded web view that loads a URL or renders raw HTML.
/// Build via [`web_view`] or [`web_view_html`].
pub struct WebView {
    pub(crate) url: Option<String>,
    pub(crate) html: Option<String>,
}

/// Create a WebView that loads the given URL (e.g. `"https://example.com"`).
pub fn web_view(url: impl Into<String>) -> WebView {
    WebView {
        url: Some(url.into()),
        html: None,
    }
}

/// Create a WebView that renders raw HTML content.
pub fn web_view_html(html: impl Into<String>) -> WebView {
    WebView {
        url: None,
        html: Some(html.into()),
    }
}

impl View for WebView {
    fn build(self, tree: &mut Tree) -> WidgetId {
        let id = tree.create_web_view();
        if let Some(url) = self.url {
            tree.set(id, Attribute::Url(url));
        }
        if let Some(html) = self.html {
            tree.set(id, Attribute::Html(html));
        }
        id
    }
}
