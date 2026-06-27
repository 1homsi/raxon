//! Declarative, **macro-free** view builder for `rax`.
//!
//! This is the foundational developer API: a typed tuple-builder that lowers
//! directly to the [`rax_dom`] element tree. The framework never *requires* a
//! macro — an optional `rsx!` (a separate crate) will expand into exactly these
//! calls, the way JSX expands to `createElement` and Dioxus RSX expands to
//! builder code. You can always drop to this layer.
//!
//! ```
//! use raxon::view::{column, text, button, mount, View};
//! use raxon::dom::{Host, RecordingBackend, Tree};
//! use raxon::reactive::create_signal;
//!
//! fn counter(count: raxon::reactive::Signal<i32>) -> impl View {
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
mod camera;
pub mod canvas;
mod composite;
mod container;
mod controls;
mod dynamic;
mod extras;
mod image;
mod indicators;
pub mod layout;
mod list;
pub mod map;
mod modifier;
mod qr;
pub mod registry;
mod scroll;
mod spacer;
mod text;
mod text_input;
mod theme;
mod view;
mod web_view;

pub use button::{button, Button};
pub use map::{map_view, MapView};
pub use camera::{camera_scanner, CameraScanner};
pub use canvas::{canvas, charts, Canvas};
pub use composite::{
    accordion, action_sheet, alert, app_bar, avatar, badge, banner, backdrop, bottom_sheet,
    breadcrumbs, calendar, card, carousel, checkbox, chip, collapsible, color_picker, dev_tools,
    drawer, empty_state, error_boundary, error_overlay, fade_transition, grid, infinite_scroll,
    item_separator, keyboard_avoiding_view, lazy_column, lazy_row, list_with_header, modal,
    multi_select, network_image, pan_animation, pdf_view, picker, pressable, radio, rating_bar,
    reactive_list, reorderable_list, search_bar, section_list, segmented_control, skeleton,
    status_bar, status_bar_spacer, sticky_header, swipe_actions, tab, tab_bar, tabs, toast,
    tooltip, wrap, AccordionSection, Avatar, Badge, BannerKind, Card, Checkbox, Chip, Radio,
    Section, Skeleton, TabItem,
};
pub use web_view::{web_view, web_view_html, WebView};
pub use container::{column, row, stack, Container, Stack};
pub use controls::{
    date_picker, date_range_picker, segmented, slider, stepper, switch, DatePicker, Segmented,
    Slider, Stepper, Switch,
};
pub use dynamic::{dynamic, Dynamic};
pub use extras::{divider, vertical_divider};
pub use image::{icon, image, Image};
pub use indicators::{activity_indicator, progress, ActivityIndicator, Progress};
pub use list::{each, show};
pub use modifier::{Decorated, PanInfo, PinchInfo, RotateInfo, Styled, ViewExt};
pub use qr::{qr_code, qr_matrix, QrCode, QrError, QrMatrix};
pub use scroll::{scroll, Scroll};
pub use spacer::{spacer, Spacer};
pub use text::{rich_text, text, DynamicText, IntoText, RichText, StaticText, Text};
pub use crate::dom::{DrawCmd, MenuItem, Stroke, TextSpan};
pub use text_input::{text_area, text_input, TextArea, TextInput};
pub use theme::{
    provide_theme, try_use_theme, use_theme, ColorTokens, CustomTokens, MotionTokens, RadiusTokens,
    ShadowToken, ShadowTokens, SpacingTokens, Theme, ThemeBuilder, TypographyTokens,
};
pub use registry::{
    is_registered, register_component, resolve_component, unregister_component, ComponentProps,
};
pub use view::{boxed, BoxedView, View, ViewSequence};
pub use layout::{
    adaptive_split_view, aspect_ratio, center, expanded, flexible, master_detail, responsive,
    safe_area_bottom, safe_area_top, safe_area_view, split_view_mode_for_width,
    update_layout_direction, update_window_size, use_layout_direction, use_orientation,
    use_size_class, use_window_width, LayoutDirection as AppLayoutDirection, Orientation,
    SizeClass, SplitPane, SplitViewConfig, SplitViewMode,
};

// Re-export the style enums used by the builder API for convenience.
pub use crate::core::{AlignItems, Dimension, EdgeInsets, FlexWrap, JustifyContent, Position};
pub use crate::dom::{
    DatePickerMode, DatePickerStyle, GesturePhase, ImageResizeMode, KeyboardType, LayoutDirection,
    LinearGradient, ReturnKeyType, Role, TextAlign, TextStyle, Transform,
};

use crate::dom::{Tree, WidgetId};

// ---------------------------------------------------------------------------
// FPS tracking (updated by the platform backend each tick)
// ---------------------------------------------------------------------------

thread_local! {
    static FPS_SIGNAL: std::cell::Cell<Option<crate::reactive::Signal<f32>>> =
        const { std::cell::Cell::new(None) };
}

/// Returns a reactive [`Signal<f32>`] that is updated each frame with the
/// current frames-per-second count.
///
/// Call from within a reactive context (e.g. `dev_tools()` or a custom debug
/// overlay). The signal is lazily created on first use.
pub fn use_fps() -> crate::reactive::Signal<f32> {
    let existing = FPS_SIGNAL.with(|s| s.get());
    if let Some(sig) = existing {
        return sig;
    }
    let sig = crate::reactive::create_signal(60.0f32);
    FPS_SIGNAL.with(|s| s.set(Some(sig)));
    sig
}

/// Updates the FPS signal. Called by the platform backend each tick.
///
/// This is a public but low-level hook — app code should use [`use_fps`]
/// to read the value reactively.
pub fn update_fps(fps: f32) {
    if let Some(sig) = FPS_SIGNAL.with(|s| s.get()) {
        sig.set(fps);
    }
}

/// Builds `view` into `tree` and marks it as the tree root. Returns the root id.
pub fn mount(tree: &mut Tree, view: impl View) -> WidgetId {
    let id = view.build(tree);
    tree.set_root(id);
    id
}
