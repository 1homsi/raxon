//! The retained element tree and render seam for `rax`.
//!
//! This crate ties the reactive runtime ([`rax_reactive`]) to a tree of widgets
//! and a backend-agnostic [`Mutation`] stream. It contains **no platform code**:
//! a [`Backend`] is the single trait platforms implement, and [`RecordingBackend`]
//! lets the whole pipeline be tested on the host.
//!
//! ```
//! use rax_dom::{Tree, Host, RecordingBackend, Attribute, Mutation, WidgetKind};
//! use rax_reactive::create_signal;
//!
//! let backend = RecordingBackend::new();
//! let log = backend.log();
//! let mut tree = Tree::new(Host::new(backend));
//!
//! let count = create_signal(0);
//! let label = tree.create_text();
//! tree.bind(label, move || Attribute::Text(format!("Count: {}", count.get())));
//!
//! // The initial bind emitted Create + the first SetAttribute.
//! count.set(1); // exactly one more SetAttribute — no tree diff.
//!
//! let muts = log.borrow();
//! assert!(matches!(muts.last(), Some(Mutation::SetAttribute { .. })));
//! ```

#![forbid(unsafe_code)]

mod backend;
mod event;
mod mutation;
mod tree;

pub use backend::{Backend, Host, RecordingBackend};
pub use event::{Event, EventKind, EventSink, Lifecycle, PointerId, TextSelection};
pub use mutation::{Attribute, Mutation, WidgetId, WidgetKind};
pub use tree::Tree;
