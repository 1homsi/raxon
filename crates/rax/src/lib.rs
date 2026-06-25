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

/// Plugin system for registering native modules with the rax runtime.
pub use rax_plugin as plugin;

/// Compile-time platform detection helpers.
pub mod platform;

/// Secure key-value storage (iOS Keychain on device, in-memory elsewhere).
pub use rax_keychain as keychain;

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
    pub use rax_runtime::{
        authenticate_biometric, cancel_notification, clear_ui_state, haptic,
        install_error_overlay, last_panic, on_deep_link, restore_ui_state, save_ui_state,
        schedule_notification, set_backdrop, start_location, start_motion, stop_location,
        stop_motion, use_color_scheme, App, Backdrop, HapticStyle, KeyboardType, LocalNotification,
        TextStyle,
    };

    // High-frequency helpers from the satellite crates. Full surfaces live in
    // the namespaced modules (`rax::nav`, `rax::store`, …).
    pub use rax_anim::{animate, decay, delayed, oscillate, parallel, sequence, spring, stagger, Easing, Spring};
    pub use rax_async::{create_resource, Resource};
    // HTTP client helpers — `get`/`post` return a `Resource<Response>`.
    pub use rax_net::{
        connect_sse, connect_ws, get, invalidate_query, post, send, use_query, Method, Request,
        Response, SseEvent, WsHandle, WsMessage,
    };
    // Async resource state (needed to match on Loading/Ready/Failed).
    pub use rax_async::ResourceState;
    pub use rax_intl::{t, t_args, t_plural};
    pub use rax_nav::{
        create_navigator, routes, transition_routes, use_navigator, NavigationTransition,
        Navigator,
    };
    pub use rax_store::{persisted, store_get, store_set};
    pub use rax_style::{theme, use_theme, Theme};
    pub use rax_plugin::{dispatch_plugin_event, register_plugin, Plugin};
    pub use rax_keychain::{delete_secret, get_secret, set_secret};

    // Platform detection helpers.
    pub use crate::platform::platform_value;

    // The iOS launcher, so `run(app)` is in scope on device/simulator.
    #[cfg(target_os = "ios")]
    pub use rax_ios::run;
}
