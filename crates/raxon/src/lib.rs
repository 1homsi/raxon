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

// The pure-Rust subsystems are unsafe-free; `deny` (not `forbid`) so the iOS
// backend module can locally `allow` the objc2 FFI it necessarily requires.
#![deny(unsafe_code)]
#![warn(missing_docs)]

// --- subsystems, namespaced ------------------------------------------------

/// Android backend command queue and driver foundation.
pub mod android;
/// Animation: tweens and easing.
pub mod anim;
/// Cooperative async: the executor and `Resource`.
pub mod async_rt;
/// Core value types: colors, geometry, and the neutral layout style model.
pub mod core;
/// The virtual DOM: element tree, mutations, events, and backend trait.
pub mod dom;
/// Form validation helpers.
pub mod form;
/// Filesystem utilities.
pub mod fs;
/// Shared host-session lifecycle for generated platform glue.
pub mod host;
/// i18n message format helpers.
pub mod i18n;
/// Internationalization: message catalogs and lookup.
pub mod intl;
/// Secure key-value storage (iOS Keychain on device, in-memory elsewhere).
pub mod keychain;
/// Layout engine integration.
pub mod layout;
/// Structured logging.
pub mod log;
/// Navigation: stack/tab navigators and typed routes.
pub mod nav;
/// HTTP client and request/response types.
pub mod net;
/// Plugin system for registering native modules with the raxon runtime.
pub mod plugin;
/// Fine-grained reactivity: signals, memos, effects, context, scopes.
pub mod reactive;
/// The app runtime that drives layout, events, and frames.
pub mod runtime;
/// Frame scheduler and clock.
pub mod scheduler;
/// SQLite database access.
pub mod sqlite;
/// Key-value storage and persisted signals.
pub mod store;
/// Theming: palettes, spacing, typography, and the `Theme` context.
pub mod style;
/// The declarative, macro-free view builder.
pub mod view;
/// WebAssembly DOM backend command queue and driver foundation.
pub mod web;
/// Versioned JSON wire protocol for host-originated platform events.
pub mod wire;

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
    pub use crate::core::{Color, ColorScheme, FlexDirection, LayoutStyle, Point, Rect, Size};
    pub use crate::reactive::*;
    pub use crate::view::*;

    pub use crate::runtime::{
        authenticate_biometric, cancel_notification, check_permission, clear_ui_state, haptic,
        install_error_overlay, last_panic, on_deep_link, present_document_picker,
        register_background_task, request_permission, restore_ui_state, save_ui_state,
        schedule_background_task, schedule_notification, set_backdrop, start_location,
        start_motion, stop_location, stop_motion, use_app_lifecycle, use_color_scheme,
        use_permission, use_safe_area_insets, use_system_locale, App, Backdrop, HapticStyle,
        KeyboardType, Lifecycle, LocalNotification, NetworkStatus, PermissionKind,
        PermissionStatus, TextStyle,
    };

    pub use crate::anim::{
        animate, animate_offthread, decay, delayed, oscillate, parallel, sequence, spring, stagger,
        start_animation_thread, Easing, OffThreadValue, Spring,
    };
    pub use crate::async_rt::{create_resource, Resource, ResourceState};
    pub use crate::intl::{t, t_args, t_plural};
    pub use crate::keychain::{delete_secret, get_secret, set_secret};
    pub use crate::nav::{
        apply_navigation_command, apply_navigation_command_json, apply_navigation_commands,
        bind_web_history, build_route, build_route_with_query, can_go_back, cancel_route_result,
        clear_saved_navigation_state, create_navigator, create_tab_stack_navigator,
        create_tab_stack_navigator_at, current_route, current_route_location,
        decode_navigation_command, decode_navigation_commands, decode_navigation_debug_snapshot,
        decode_navigation_state, encode_navigation_command, encode_navigation_debug_snapshot,
        encode_navigation_state, fire_transition_complete, fire_transition_start, go_back,
        has_pending_route_result, keep_alive_tab_stack, keep_alive_tab_stack_routes, match_route,
        match_route_location, navigate, navigate_for_result, navigation_debug_snapshot,
        navigation_debug_snapshot_json, navigation_state,
        on_transition_complete, on_transition_start, parse_deep_link, parse_query,
        parse_query_all, parse_route_location, pending_route_result_route,
        pending_route_result_type, remove_fragment, remove_query_param, replace_fragment,
        replace_query_param, replace_query_param_values, replace_remove_fragment,
        replace_remove_query_param, replace_route, reset_route, restore_navigation_state,
        restore_saved_navigation_state, return_route_result, route, routes, save_navigation_state,
        set_fragment, set_query_param, set_query_param_values, stack, tab_stack, tab_stack_routes,
        transition_routes, transition_routes_with, try_navigate_for_result, url_routes,
        use_navigator, use_params, use_query_params, use_tab_stack_navigator, NavigationCommand,
        NavigationCommandKind,
        NavigationCommandOutcome, NavigationDebugSnapshot, NavigationState, NavigationTransition,
        NavigationTransitionContext, Navigator, NavigatorDebugSnapshot, NavigatorTransitionKind,
        RouteLocation, RouteMatch, RouteTransitionEvent, RouteTransitionKind,
        TabStackDebugSnapshot, TabStackNavigator, UrlRoute, NAVIGATION_STATE_KEY,
    };
    pub use crate::net::{
        connect_sse, connect_ws, gc_query_cache, get, invalidate_query, post, send, use_query,
        use_query_stale, Method, Request, Response, SseEvent, WsHandle, WsMessage,
    };
    pub use crate::platform::{platform_choice, platform_value};
    pub use crate::plugin::{dispatch_plugin_event, register_plugin, Plugin};
    pub use crate::store::{persisted, store_get, store_set};
    pub use crate::style::{theme, use_theme, Theme};

    #[cfg(target_os = "ios")]
    pub use crate::ios::run;
}
