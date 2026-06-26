//! Layout helper views and responsive utilities.
//!
//! This module provides convenience wrappers for common layout patterns:
//!
//! - [`expanded`] / [`flexible`] — fill available main-axis space
//! - [`aspect_ratio`] — constrain width/height ratio
//! - [`center`] — center a child on both axes
//! - [`safe_area_top`] / [`safe_area_bottom`] / [`safe_area_view`] — iOS safe-area insets
//! - [`use_orientation`] / [`use_window_width`] / [`use_size_class`] — reactive device info
//! - [`update_window_size`] — called by the platform backend on resize/rotation
//!
//! All helpers are built from the existing [`ViewExt`] modifier API, so they
//! compose freely with every other modifier in the crate.

use std::cell::RefCell;

use rax_core::{AlignItems, JustifyContent};
use rax_reactive::{create_memo, create_signal, Memo, Signal};

use crate::container::column;
use crate::modifier::ViewExt;
use crate::spacer::spacer;
use crate::view::View;

// ---------------------------------------------------------------------------
// RTL-aware layout direction
// ---------------------------------------------------------------------------

/// The logical layout direction for the application (or a subtree).
///
/// This is an app-level signal; changing it causes every reactive consumer
/// (dynamic views, direction modifiers) to re-evaluate. Typically set once at
/// startup based on the device locale, and updated whenever the locale changes
/// (e.g. via rax-i18n's locale signal).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum LayoutDirection {
    /// Left-to-right layout (the default for most locales).
    #[default]
    Ltr,
    /// Right-to-left layout (Arabic, Hebrew, Persian, …).
    Rtl,
}

thread_local! {
    static LAYOUT_DIRECTION: RefCell<Option<Signal<LayoutDirection>>> =
        RefCell::new(None);
}

/// Returns the app-wide reactive [`Signal<LayoutDirection>`].
///
/// The signal is created on first call and shared across all callers on the
/// same thread. Start as [`LayoutDirection::Ltr`]; call
/// [`update_layout_direction`] to change it.
///
/// # Example
/// ```rust
/// use rax_view::layout::{use_layout_direction, LayoutDirection};
///
/// let dir = use_layout_direction();
/// // Inside a dynamic() closure:
/// // if dir.get() == LayoutDirection::Rtl { … }
/// ```
pub fn use_layout_direction() -> Signal<LayoutDirection> {
    LAYOUT_DIRECTION.with(|cell| {
        let mut borrow = cell.borrow_mut();
        if let Some(sig) = *borrow {
            return sig;
        }
        let sig = create_signal(LayoutDirection::default());
        *borrow = Some(sig);
        sig
    })
}

/// Sets the app-wide layout direction signal.
///
/// Call this from your i18n/locale layer when the locale changes (e.g. via
/// rax-i18n's locale signal effect), or once at startup for a fixed-locale app.
///
/// # Example (platform bootstrap)
/// ```rust
/// use rax_view::layout::{update_layout_direction, LayoutDirection};
///
/// // Detected Arabic locale → RTL.
/// update_layout_direction(LayoutDirection::Rtl);
/// ```
pub fn update_layout_direction(dir: LayoutDirection) {
    let sig = use_layout_direction();
    sig.set(dir);
}

// ---------------------------------------------------------------------------
// Expanded / Flexible
// ---------------------------------------------------------------------------

/// Wraps `child` so it expands to fill all available space along the parent's
/// main axis (equivalent to `flex-grow: 1` in CSS).
///
/// Use inside a [`column`](crate::column) or [`row`](crate::row) alongside
/// fixed-size siblings to push content to either end, or to fill the remaining
/// viewport height/width.
///
/// # Example
/// ```rust
/// use rax_view::{column, text, layout::expanded};
///
/// let v = column((
///     text("Header"),
///     expanded(text("Fills remaining space")),
///     text("Footer"),
/// ));
/// ```
pub fn expanded<V: View>(child: V) -> impl View {
    child.flex_grow(1.0)
}

/// Wraps `child` with a specific flex-grow `factor`.
///
/// A factor of `1.0` is equivalent to [`expanded`]. Larger values claim a
/// proportionally larger share of free space relative to sibling flexible views.
///
/// # Example
/// ```rust
/// use rax_view::{row, text, layout::flexible};
///
/// // Left column takes 2× as much space as the right column.
/// let v = row((
///     flexible(text("Wide"), 2.0),
///     flexible(text("Narrow"), 1.0),
/// ));
/// ```
pub fn flexible<V: View>(child: V, factor: f32) -> impl View {
    child.flex_grow(factor)
}

// ---------------------------------------------------------------------------
// AspectRatio
// ---------------------------------------------------------------------------

/// Constrains `child` to a fixed `ratio` (width ÷ height).
///
/// This delegates directly to the layout engine's native aspect-ratio support
/// via [`LayoutStyle::aspect_ratio`](rax_core::LayoutStyle::aspect_ratio).
/// The child's width (or height, whichever is determined first by its parent)
/// is used to compute the other dimension automatically.
///
/// # Example
/// ```rust
/// use rax_view::{image, layout::aspect_ratio};
///
/// // Force a banner image to always be 16:9.
/// let v = aspect_ratio(image("banner"), 16.0 / 9.0);
/// ```
pub fn aspect_ratio<V: View>(child: V, ratio: f32) -> impl View {
    child.aspect_ratio(ratio)
}

// ---------------------------------------------------------------------------
// Center
// ---------------------------------------------------------------------------

/// Centers `child` on both axes, expanding to fill the parent.
///
/// Wraps `child` in a [`column`] with `align_items: Center` and
/// `justify_content: Center`, and applies `flex-grow: 1` so the container
/// takes up all available space.
///
/// # Example
/// ```rust
/// use rax_view::{text, layout::center};
///
/// let v = center(text("I'm centered!"));
/// ```
pub fn center<V: View>(child: V) -> impl View {
    column((child,))
        .align(AlignItems::Center)
        .justify(JustifyContent::Center)
        .grow_by(1.0)
}

// ---------------------------------------------------------------------------
// Safe-area spacers
// ---------------------------------------------------------------------------

/// A fixed-height spacer for the iOS top safe area (status bar + notch/Dynamic
/// Island). Standard height is 44 pt on notched devices.
///
/// For a pixel-perfect inset, wire the actual safe-area value from the platform
/// backend using [`update_window_size`]. This helper is a static approximation
/// suited for initial layout before backend data arrives.
///
/// # Example
/// ```rust
/// use rax_view::{column, text, layout::{safe_area_top, safe_area_bottom}};
///
/// let v = column((
///     rax_view::boxed(safe_area_top()),
///     rax_view::boxed(text("Content")),
///     rax_view::boxed(safe_area_bottom()),
/// ));
/// ```
pub fn safe_area_top() -> impl View {
    spacer().grow(0.0).height(44.0)
}

/// A fixed-height spacer for the iOS bottom safe area (home indicator).
/// Standard height is 34 pt on devices with a home indicator.
pub fn safe_area_bottom() -> impl View {
    spacer().grow(0.0).height(34.0)
}

/// Wraps `child` in a [`column`] that adds top and bottom safe-area spacers.
///
/// Equivalent to:
/// ```text
/// column((safe_area_top(), child, safe_area_bottom()))
/// ```
///
/// # Example
/// ```rust
/// use rax_view::{text, layout::safe_area_view};
///
/// let v = safe_area_view(text("Full-screen content"));
/// ```
pub fn safe_area_view<V: View + 'static>(child: V) -> impl View {
    use crate::view::boxed;
    column((
        boxed(safe_area_top()),
        boxed(child),
        boxed(safe_area_bottom()),
    ))
}

// ---------------------------------------------------------------------------
// Responsive: Orientation & SizeClass signals
// ---------------------------------------------------------------------------

/// Device orientation.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Orientation {
    /// Height is greater than width.
    #[default]
    Portrait,
    /// Width is greater than or equal to height.
    Landscape,
}

/// Horizontal size class — a coarse breakpoint for adaptive layouts.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SizeClass {
    /// Window width < 600 pt (phones in portrait, narrow split view).
    #[default]
    Compact,
    /// Window width ≥ 600 pt (tablets, phones in landscape, wide split view).
    Regular,
}

thread_local! {
    static ORIENTATION_SIGNAL: std::cell::Cell<Option<Signal<Orientation>>> =
        const { std::cell::Cell::new(None) };
    static WIDTH_SIGNAL: std::cell::Cell<Option<Signal<f32>>> =
        const { std::cell::Cell::new(None) };
}

/// Returns a reactive [`Signal<Orientation>`] updated by the platform backend
/// when the device rotates.
///
/// The signal starts as [`Orientation::Portrait`] and is updated via
/// [`update_window_size`]. The signal is lazily created on first use and shared
/// across all callers on the same thread.
///
/// # Example
/// ```rust
/// use rax_view::{dynamic, text, boxed, layout::{use_orientation, Orientation}};
///
/// let v = dynamic(move || {
///     let label = if use_orientation().get() == Orientation::Landscape {
///         "Landscape"
///     } else {
///         "Portrait"
///     };
///     boxed(text(label))
/// });
/// ```
pub fn use_orientation() -> Signal<Orientation> {
    if let Some(s) = ORIENTATION_SIGNAL.with(|c| c.get()) {
        return s;
    }
    let s = create_signal(Orientation::default());
    ORIENTATION_SIGNAL.with(|c| c.set(Some(s)));
    s
}

/// Returns a reactive [`Signal<f32>`] carrying the current window width in
/// logical points.
///
/// Starts at `390.0` (iPhone 14 / 15 logical width) and is updated via
/// [`update_window_size`]. The signal is lazily created on first use and shared
/// across all callers on the same thread.
///
/// # Example
/// ```rust
/// use rax_view::{dynamic, text, boxed, layout::use_window_width};
///
/// let v = dynamic(move || {
///     let w = use_window_width().get();
///     boxed(text(format!("Width: {w:.0}pt")))
/// });
/// ```
pub fn use_window_width() -> Signal<f32> {
    if let Some(s) = WIDTH_SIGNAL.with(|c| c.get()) {
        return s;
    }
    let s = create_signal(390.0f32);
    WIDTH_SIGNAL.with(|c| c.set(Some(s)));
    s
}

/// Returns a reactive [`Memo<SizeClass>`] derived from [`use_window_width`].
///
/// - `Compact` when width < 600 pt
/// - `Regular` when width ≥ 600 pt
///
/// Because this is a [`Memo`], it only re-notifies dependents when the class
/// actually crosses the 600 pt boundary, not on every pixel change.
///
/// # Example
/// ```rust
/// use rax_view::{dynamic, text, boxed, layout::{use_size_class, SizeClass}};
///
/// let v = dynamic(move || {
///     let label = if use_size_class().get() == SizeClass::Regular {
///         "Tablet layout"
///     } else {
///         "Phone layout"
///     };
///     boxed(text(label))
/// });
/// ```
pub fn use_size_class() -> Memo<SizeClass> {
    let w = use_window_width();
    create_memo(move || {
        if w.get() < 600.0 {
            SizeClass::Compact
        } else {
            SizeClass::Regular
        }
    })
}

/// Updates the window-size and orientation signals.
///
/// Call this from the platform backend whenever the window or display bounds
/// change (e.g. on device rotation, split-view resize, or window drag).
///
/// `width` and `height` are in logical points.
///
/// # Example (platform backend)
/// ```rust
/// use rax_view::layout::update_window_size;
///
/// // Called by UIKit / AppKit / the web runtime on every resize event.
/// fn on_window_resize(width: f32, height: f32) {
///     update_window_size(width, height);
/// }
/// ```
pub fn update_window_size(width: f32, height: f32) {
    if let Some(s) = WIDTH_SIGNAL.with(|c| c.get()) {
        s.set(width);
    }
    let orientation = if width >= height {
        Orientation::Landscape
    } else {
        Orientation::Portrait
    };
    if let Some(s) = ORIENTATION_SIGNAL.with(|c| c.get()) {
        s.set(orientation);
    }
}

// ---------------------------------------------------------------------------
// Responsive layout helper
// ---------------------------------------------------------------------------

/// Build a view that reactively adapts to the current [`SizeClass`] and
/// [`Orientation`].
///
/// `builder` is called once at startup and again whenever the size class or
/// orientation changes. The returned view replaces the previous one in the
/// tree via [`crate::dynamic::dynamic`].
///
/// # Example
/// ```rust
/// use rax_view::{text, boxed, layout::{responsive, SizeClass, Orientation}};
///
/// let v = responsive(|size_class, orientation| {
///     let label = match (size_class, orientation) {
///         (SizeClass::Regular, _) => "Tablet layout",
///         (_, Orientation::Landscape) => "Landscape phone",
///         _ => "Portrait phone",
///     };
///     boxed(text(label))
/// });
/// ```
pub fn responsive<V: crate::view::View + 'static>(
    builder: impl Fn(SizeClass, Orientation) -> V + 'static,
) -> impl crate::view::View {
    use crate::dynamic::dynamic;
    use crate::view::boxed;

    dynamic(move || {
        let sc = use_size_class().get();
        let ori = use_orientation().get();
        boxed(builder(sc, ori))
    })
}
