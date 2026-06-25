//! Declarative, **macro-free** view builder for `rax`.
//!
//! This is the foundational developer API: a typed tuple-builder that lowers
//! directly to the [`rax_dom`] element tree. The framework never *requires* a
//! macro — an optional `rsx!` (a separate crate) will expand into exactly these
//! calls, the way JSX expands to `createElement` and Dioxus RSX expands to
//! builder code. You can always drop to this layer.
//!
//! ```
//! use rax_view::{column, text, button, mount, View};
//! use rax_dom::{Host, RecordingBackend, Tree};
//! use rax_reactive::create_signal;
//!
//! fn counter(count: rax_reactive::Signal<i32>) -> impl View {
//!     column((
//!         text(move || format!("Count: {}", count.get())),
//!         button("+1", move || count.update(|c| *c += 1)),
//!     ))
//!     .padding(16.0)
//!     .gap(8.0)
//! }
//!
//! let mut tree = Tree::new(Host::new(RecordingBackend::new()));
//! let count = create_signal(0);
//! mount(&mut tree, counter(count));
//! ```
//!
//! # The model
//!
//! Structure builds **once**; reactive values update through signal bindings
//! (one mutation per change). Dynamic structure (lists, conditionals) will be
//! provided by dedicated views in a later increment.

#![forbid(unsafe_code)]

mod button;
mod container;
mod controls;
mod dynamic;
mod extras;
mod image;
mod indicators;
mod list;
mod modifier;
mod scroll;
mod spacer;
mod text;
mod text_input;
mod view;

pub use button::{button, Button};
pub use container::{column, row, Container};
pub use controls::{slider, switch, Slider, Switch};
pub use dynamic::{dynamic, Dynamic};
pub use extras::{divider, vertical_divider};
pub use image::{icon, image, Image};
pub use indicators::{activity_indicator, progress, ActivityIndicator, Progress};
pub use list::{each, show};
pub use modifier::{Decorated, Styled, ViewExt};
pub use scroll::{scroll, Scroll};
pub use spacer::{spacer, Spacer};
pub use text::{text, DynamicText, IntoText, StaticText, Text};
pub use text_input::{text_input, TextInput};
pub use view::{boxed, BoxedView, View, ViewSequence};

// Re-export the style enums used by the builder API for convenience.
pub use rax_core::{AlignItems, Dimension, EdgeInsets, FlexWrap, JustifyContent, Position};
pub use rax_dom::{Role, TextAlign};

use rax_dom::{Tree, WidgetId};

/// Builds `view` into `tree` and marks it as the tree root. Returns the root id.
pub fn mount(tree: &mut Tree, view: impl View) -> WidgetId {
    let id = view.build(tree);
    tree.set_root(id);
    id
}
