//! # rax
//!
//! The framework facade. Depend on this one crate and write your whole app
//! against [`rax::prelude`](prelude); reach for the namespaced modules
//! ([`reactive`], [`view`], [`style`], …) when you want the full surface of a
//! subsystem.
//!
//! ```
//! use rax::prelude::*;
//!
//! fn counter(count: Signal<i32>) -> impl View {
//!     column((
//!         text(move || format!("Count: {}", count.get())).font_size(24.0),
//!         row((
//!             button("−", move || count.update(|c| *c -= 1)),
//!             button("+", move || count.update(|c| *c += 1)),
//!         ))
//!         .gap(12.0),
//!     ))
//!     .gap(8.0)
//!     .padding(16.0)
//! }
//! ```
//!
//! ## Building your own components
//!
//! Everything user-facing is a [`View`](view::View). To make a reusable
//! component, write a function returning `impl View` (or a struct implementing
//! `View`) using only this public surface — the same way the built-in
//! [`checkbox`](view::checkbox) / [`radio`](view::radio) are composed from
//! `icon` + `text` + `row` + `dynamic` + `on_tap`. There is no privileged
//! internal API: your components are first-class peers of ours.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

// --- subsystems, namespaced ------------------------------------------------

/// Core value types: colors, geometry, and the neutral layout style model.
pub use rax_core as core;
/// Fine-grained reactivity: signals, memos, effects, context, scopes.
pub use rax_reactive as reactive;
/// The declarative, macro-free view builder.
pub use rax_view as view;
/// The app runtime that drives layout, events, and frames.
pub use rax_runtime as runtime;
/// Navigation: stack/tab navigators and typed routes.
pub use rax_nav as nav;
/// Key-value storage and persisted signals.
pub use rax_store as store;
/// HTTP client and request/response types.
pub use rax_net as net;
/// Animation: tweens and easing.
pub use rax_anim as anim;
/// Cooperative async: the executor and `Resource`.
pub use rax_async as async_rt;
/// Internationalization: message catalogs and lookup.
pub use rax_intl as intl;
/// Theming: palettes, spacing, typography, and the `Theme` context.
pub use rax_style as style;

/// Host-side testing harness (enable the `testing` feature).
#[cfg(feature = "testing")]
pub use rax_test as test;

// The iOS backend's entry point, surfaced at the crate root on Apple targets so
// apps call `rax::run(app)` without naming the backend crate.
#[cfg(target_os = "ios")]
pub use rax_ios::run;

/// The common surface for building an app: import this and start writing views.
///
/// Bundles the view builders + modifiers, the reactive primitives, the core
/// value types, and the most-used helpers from each subsystem. Anything not
/// here is one module away (e.g. `rax::net::MockClient`, `rax::style::Theme`).
pub mod prelude {
    // The whole view builder API (containers, controls, modifiers, the `View`
    // trait, and the re-exported style enums like `AlignItems`/`Position`).
    pub use rax_view::*;

    // Reactivity: signals, memos, effects, context, batching, roots.
    pub use rax_reactive::*;

    // Core value types not already surfaced via the view layer.
    pub use rax_core::{Color, ColorScheme, FlexDirection, LayoutStyle, Point, Rect, Size};

    // The app entry point and runtime, plus appearance controls.
    pub use rax_runtime::{set_backdrop, use_color_scheme, App, Backdrop};

    // High-frequency helpers from the satellite crates. Full surfaces live in
    // the namespaced modules (`rax::nav`, `rax::store`, …).
    pub use rax_anim::{animate, Easing};
    pub use rax_async::{create_resource, Resource};
    pub use rax_intl::{t, t_args};
    pub use rax_nav::{create_navigator, routes, use_navigator, Navigator};
    pub use rax_store::{persisted, store_get, store_set};
    pub use rax_style::{theme, use_theme, Theme};

    // The iOS launcher, so `run(app)` is in scope on device/simulator.
    #[cfg(target_os = "ios")]
    pub use rax_ios::run;
}
