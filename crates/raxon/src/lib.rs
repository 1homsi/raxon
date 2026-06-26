//! # raxon
//!
//! A reactive, signal-driven native UI framework for Rust.
//!
//! ```
//! use raxon::prelude::*;
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

#![forbid(unsafe_code)]
#![warn(missing_docs)]

// --- subsystems, namespaced ------------------------------------------------

/// Core value types: colors, geometry, and the neutral layout style model.
pub mod core;
/// Fine-grained reactivity: signals, memos, effects, context, scopes.
pub mod reactive;
/// The virtual DOM: element tree, mutations, events, and backend trait.
pub mod dom;
/// The declarative, macro-free view builder.
pub mod view;
/// The app runtime that drives layout, events, and frames.
pub mod runtime;
/// Navigation: stack/tab navigators and typed routes.
pub mod nav;
/// Key-value storage and persisted signals.
pub mod store;
/// HTTP client and request/response types.
pub mod net;
/// Animation: tweens and easing.
pub mod anim;
/// Cooperative async: the executor and `Resource`.
pub mod async_rt;
/// Internationalization: message catalogs and lookup.
pub mod intl;
/// Theming: palettes, spacing, typography, and the `Theme` context.
pub mod style;
/// Plugin system for registering native modules with the raxon runtime.
pub mod plugin;
/// Secure key-value storage (iOS Keychain on device, in-memory elsewhere).
pub mod keychain;
/// Structured logging.
pub mod log;
/// i18n message format helpers.
pub mod i18n;
/// SQLite database access.
pub mod sqlite;
/// Filesystem utilities.
pub mod fs;
/// Form validation helpers.
pub mod form;
/// Frame scheduler and clock.
pub mod scheduler;
/// Layout engine integration.
pub mod layout;

/// Host-side testing harness (enable the `testing` feature).
#[cfg(feature = "testing")]
pub mod testing;

/// Compile-time platform detection helpers.
pub mod platform;

// The iOS backend's entry point, surfaced at the crate root on Apple targets so
// apps call `raxon::run(app)` without naming the backend crate.
#[cfg(target_os = "ios")]
pub mod ios;

#[cfg(target_os = "ios")]
pub use ios::run;

/// The common surface for building an app: import this and start writing views.
///
/// Bundles the view builders + modifiers, the reactive primitives, the core
/// value types, and the most-used helpers from each subsystem.
pub mod prelude {
    pub use crate::view::*;
    pub use crate::reactive::*;
    pub use crate::core::{Color, ColorScheme, FlexDirection, LayoutStyle, Point, Rect, Size};

    pub use crate::runtime::{
        authenticate_biometric, cancel_notification, clear_ui_state, haptic,
        install_error_overlay, last_panic, on_deep_link, register_background_task,
        restore_ui_state, save_ui_state, schedule_background_task, schedule_notification,
        set_backdrop, start_location, start_motion, stop_location, stop_motion, use_color_scheme,
        use_safe_area_insets, App, Backdrop, HapticStyle, KeyboardType, LocalNotification,
        TextStyle,
    };

    pub use crate::anim::{
        animate, animate_offthread, decay, delayed, oscillate, parallel, sequence, spring, stagger,
        start_animation_thread, Easing, OffThreadValue, Spring,
    };
    pub use crate::async_rt::{create_resource, Resource, ResourceState};
    pub use crate::net::{
        connect_sse, connect_ws, gc_query_cache, get, invalidate_query, post, send, use_query,
        use_query_stale, Method, Request, Response, SseEvent, WsHandle, WsMessage,
    };
    pub use crate::intl::{t, t_args, t_plural};
    pub use crate::nav::{
        create_navigator, routes, transition_routes, use_navigator, NavigationTransition,
        Navigator,
    };
    pub use crate::store::{persisted, store_get, store_set};
    pub use crate::style::{theme, use_theme, Theme};
    pub use crate::plugin::{dispatch_plugin_event, register_plugin, Plugin};
    pub use crate::keychain::{delete_secret, get_secret, set_secret};
    pub use crate::platform::platform_value;

    #[cfg(target_os = "ios")]
    pub use crate::ios::run;
}
