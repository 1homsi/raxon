//! Composable components built **entirely from the public view API**.
//!
//! UIKit has no native checkbox or radio button, so — rather than add engine
//! support — we compose them from [`icon`], [`text`], [`row`], [`dynamic`], and
//! [`ViewExt::on_tap`], exactly as a third-party author would. They double as a
//! worked example: everything here uses only `rax_view`'s public surface, so any
//! consumer can build their own reusable components the same way.
//!
//! Each takes a reactive `state` getter (a `Fn() -> bool`, e.g. a closure over a
//! signal) so the glyph re-renders when the underlying value changes, and an
//! `on_change`/`on_select` callback — the same value-in / event-out shape as the
//! native [`switch`](crate::switch) and [`slider`](crate::slider).

use crate::core::{AlignItems, Color, EdgeInsets, JustifyContent};
use crate::dom::{GesturePhase, Tree, WidgetId};
use crate::reactive::{create_effect, create_signal, Signal};

use super::container::{column, row};
use super::dynamic::dynamic;
use super::image::{icon, image};
use super::list::show;
use super::modifier::{PanInfo, ViewExt};
use super::scroll::scroll;
use super::text::text;
use super::text_input::text_input;
use super::view::{boxed, BoxedView, View, ViewSequence};

/// The default accent used for a checked/selected glyph (iOS system blue).
const DEFAULT_TINT: Color = Color::rgb(0, 122, 255);
const GLYPH: f32 = 24.0;

/// A labelled checkbox. Build via [`checkbox`].
pub struct Checkbox<S, F> {
    checked: S,
    label: Option<String>,
    on_change: F,
    tint: Color,
}

/// Creates a checkbox whose checked state is read from `checked` (re-read
/// reactively, so it updates when the underlying value changes) and that calls
/// `on_change` with the toggled value when tapped.
///
/// ```
/// use super::view::checkbox;
/// use crate::reactive::create_signal;
///
/// let agreed = create_signal(false);
/// let view = checkbox(move || agreed.get(), move |v| agreed.set(v))
///     .label("I agree to the terms");
/// ```
pub fn checkbox<S, F>(checked: S, on_change: F) -> Checkbox<S, F>
where
    S: Fn() -> bool + Clone + 'static,
    F: FnMut(bool) + 'static,
{
    Checkbox {
        checked,
        label: None,
        on_change,
        tint: DEFAULT_TINT,
    }
}

impl<S, F> Checkbox<S, F> {
    /// Adds a trailing text label (also part of the tap target).
    #[must_use]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Overrides the accent color of the checked glyph.
    #[must_use]
    pub fn tint(mut self, color: Color) -> Self {
        self.tint = color;
        self
    }
}

impl<S, F> View for Checkbox<S, F>
where
    S: Fn() -> bool + Clone + 'static,
    F: FnMut(bool) + 'static,
{
    fn build(self, tree: &mut Tree) -> WidgetId {
        let tint = self.tint;
        let checked_for_glyph = self.checked.clone();
        let glyph = dynamic(move || {
            let symbol = if checked_for_glyph() {
                "checkmark.square.fill"
            } else {
                "square"
            };
            boxed(icon(symbol).tint(tint).size(GLYPH, GLYPH))
        })
        .grow(0.0);

        let checked_for_tap = self.checked;
        let mut on_change = self.on_change;
        let toggle = move || on_change(!checked_for_tap());

        let content: BoxedView = match self.label {
            Some(label) => boxed(
                row((glyph, text(label).font_size(16.0)))
                    .gap(10.0)
                    .align(AlignItems::Center),
            ),
            None => boxed(glyph),
        };
        content.on_tap(toggle).build(tree)
    }
}

/// A labelled radio button (one option of a group). Build via [`radio`].
pub struct Radio<S, F> {
    selected: S,
    label: Option<String>,
    on_select: F,
    tint: Color,
}

/// Creates a radio button whose selected state is read from `selected` and that
/// calls `on_select` when tapped. Group several over a shared signal — each
/// `selected` closure compares the signal to its own value, and `on_select`
/// sets the signal — to get single-selection behaviour.
///
/// ```
/// use super::view::radio;
/// use crate::reactive::create_signal;
///
/// let choice = create_signal(0u32);
/// let first = radio(move || choice.get() == 0, move || choice.set(0)).label("One");
/// let second = radio(move || choice.get() == 1, move || choice.set(1)).label("Two");
/// ```
pub fn radio<S, F>(selected: S, on_select: F) -> Radio<S, F>
where
    S: Fn() -> bool + Clone + 'static,
    F: FnMut() + 'static,
{
    Radio {
        selected,
        label: None,
        on_select,
        tint: DEFAULT_TINT,
    }
}

impl<S, F> Radio<S, F> {
    /// Adds a trailing text label (also part of the tap target).
    #[must_use]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Overrides the accent color of the selected glyph.
    #[must_use]
    pub fn tint(mut self, color: Color) -> Self {
        self.tint = color;
        self
    }
}

impl<S, F> View for Radio<S, F>
where
    S: Fn() -> bool + Clone + 'static,
    F: FnMut() + 'static,
{
    fn build(self, tree: &mut Tree) -> WidgetId {
        let tint = self.tint;
        let selected_for_glyph = self.selected;
        let glyph = dynamic(move || {
            let symbol = if selected_for_glyph() {
                "largecircle.fill.circle"
            } else {
                "circle"
            };
            boxed(icon(symbol).tint(tint).size(GLYPH, GLYPH))
        })
        .grow(0.0);

        let mut on_select = self.on_select;
        let select = move || on_select();

        let content: BoxedView = match self.label {
            Some(label) => boxed(
                row((glyph, text(label).font_size(16.0)))
                    .gap(10.0)
                    .align(AlignItems::Center),
            ),
            None => boxed(glyph),
        };
        content.on_tap(select).build(tree)
    }
}

/// A surface that groups its children with padding, a background fill, and
/// rounded corners. Build via [`card`].
///
/// Like [`column`](crate::column)/[`row`](crate::row), it takes a tuple of
/// children; the styling defaults to a white, lightly-rounded panel and is
/// tunable with the builder methods.
pub struct Card<C> {
    children: C,
    padding: f32,
    gap: f32,
    background: Color,
    radius: f32,
}

/// Creates a card grouping `children`.
///
/// ```
/// use super::view::{card, text};
///
/// let view = card((
///     text("Title").font_size(18.0),
///     text("Body copy goes here.").font_size(14.0),
/// ))
/// .gap(6.0);
/// ```
pub fn card<C: ViewSequence>(children: C) -> Card<C> {
    Card {
        children,
        padding: 16.0,
        gap: 8.0,
        background: Color::rgb(255, 255, 255),
        radius: 14.0,
    }
}

impl<C> Card<C> {
    /// Sets the inner padding (default `16`).
    #[must_use]
    pub fn padding(mut self, value: f32) -> Self {
        self.padding = value;
        self
    }

    /// Sets the gap between children (default `8`).
    #[must_use]
    pub fn gap(mut self, value: f32) -> Self {
        self.gap = value;
        self
    }

    /// Sets the background fill (default white).
    #[must_use]
    pub fn background(mut self, color: Color) -> Self {
        self.background = color;
        self
    }

    /// Sets the corner radius (default `14`).
    #[must_use]
    pub fn corner_radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }
}

impl<C: ViewSequence> View for Card<C> {
    fn build(self, tree: &mut Tree) -> WidgetId {
        column(self.children)
            .padding(self.padding)
            .gap(self.gap)
            .background(self.background)
            .corner_radius(self.radius)
            .build(tree)
    }
}

/// A small rounded label for counts/status. Build via [`badge`].
pub struct Badge {
    label: String,
    background: Color,
    text_color: Color,
}

/// Creates a badge showing `label` (e.g. a count or short status).
///
/// ```
/// use super::view::badge;
///
/// let unread = badge("9+");
/// ```
pub fn badge(label: impl Into<String>) -> Badge {
    Badge {
        label: label.into(),
        background: DEFAULT_TINT,
        text_color: Color::rgb(255, 255, 255),
    }
}

impl Badge {
    /// Sets the pill background (default accent blue).
    #[must_use]
    pub fn background(mut self, color: Color) -> Self {
        self.background = color;
        self
    }

    /// Sets the text color (default white).
    #[must_use]
    pub fn text_color(mut self, color: Color) -> Self {
        self.text_color = color;
        self
    }
}

impl View for Badge {
    fn build(self, tree: &mut Tree) -> WidgetId {
        // A single-cell row gives us a padded, rounded pill that hugs its text.
        row((text(self.label).font_size(12.0).color(self.text_color),))
            .padding_insets(EdgeInsets {
                top: 3.0,
                right: 8.0,
                bottom: 3.0,
                left: 8.0,
            })
            .align(AlignItems::Center)
            .background(self.background)
            .corner_radius(10.0)
            .build(tree)
    }
}

/// A circular image, typically a profile picture. Build via [`avatar`].
pub struct Avatar {
    source: String,
    size: f32,
}

/// Creates a circular avatar from an asset/symbol `source`.
///
/// ```
/// use super::view::avatar;
///
/// let pic = avatar("person.crop.circle.fill").size(48.0);
/// ```
pub fn avatar(source: impl Into<String>) -> Avatar {
    Avatar {
        source: source.into(),
        size: 40.0,
    }
}

impl Avatar {
    /// Sets the diameter in points (default `40`).
    #[must_use]
    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }
}

impl View for Avatar {
    fn build(self, tree: &mut Tree) -> WidgetId {
        // A square image clipped to a full corner radius is a circle.
        image(self.source)
            .size(self.size, self.size)
            .corner_radius(self.size / 2.0)
            .build(tree)
    }
}

/// A compact, tappable, selectable pill — filter chips, tags, choices. Build
/// via [`chip`].
pub struct Chip<F> {
    label: String,
    selected: bool,
    accent: Color,
    on_tap: F,
}

/// Creates a chip showing `label`, filled when `selected`, calling `on_tap`
/// when pressed.
///
/// ```
/// use super::view::chip;
/// use crate::reactive::create_signal;
///
/// let on = create_signal(false);
/// let view = chip("Spicy", on.get(), move || on.update(|v| *v = !*v));
/// ```
pub fn chip<F: FnMut() + 'static>(
    label: impl Into<String>,
    selected: bool,
    on_tap: F,
) -> Chip<F> {
    Chip {
        label: label.into(),
        selected,
        accent: DEFAULT_TINT,
        on_tap,
    }
}

impl<F> Chip<F> {
    /// Overrides the accent color (fill when selected, outline otherwise).
    #[must_use]
    pub fn accent(mut self, color: Color) -> Self {
        self.accent = color;
        self
    }
}

impl<F: FnMut() + 'static> View for Chip<F> {
    fn build(self, tree: &mut Tree) -> WidgetId {
        let (bg, fg) = if self.selected {
            (self.accent, Color::rgb(255, 255, 255))
        } else {
            (Color::TRANSPARENT, self.accent)
        };
        row((text(self.label).font_size(14.0).color(fg),))
            .padding_insets(EdgeInsets {
                top: 6.0,
                right: 14.0,
                bottom: 6.0,
                left: 14.0,
            })
            .align(AlignItems::Center)
            .background(bg)
            .corner_radius(16.0)
            .border(1.0, self.accent)
            .on_tap(self.on_tap)
            .build(tree)
    }
}

// ---------------------------------------------------------------------------
// SearchBar — styled text input row with a search icon
// ---------------------------------------------------------------------------

/// A search bar composed from a styled text input row.
/// `query` is the controlled signal; `on_change` fires on each keystroke;
/// `placeholder` is the hint text.
pub fn search_bar(
    query: Signal<String>,
    on_change: impl Fn(String) + Clone + 'static,
    placeholder: impl Into<String>,
) -> impl View {
    let placeholder = placeholder.into();
    dynamic(move || {
        let current = query.get();
        let on_change2 = on_change.clone();
        let placeholder2 = placeholder.clone();
        boxed(
            row((
                boxed(text("🔍").font_size(14.0).color(Color::rgba(0, 0, 0, 128))),
                boxed(
                    text_input(current, move |s| on_change2(s))
                        .placeholder(placeholder2)
                        .grow(1.0),
                ),
            ))
            .gap(8.0)
            .padding(8.0)
            .background(Color::rgba(128, 128, 128, 26))
            .corner_radius(10.0),
        )
    })
}

// ---------------------------------------------------------------------------
// Alert — centered dialog overlay
// ---------------------------------------------------------------------------

/// An alert overlay (composed from Modal + Card). Shows when `show` is `true`;
/// tapping the button sets `show` to `false`.
pub fn alert(
    show: Signal<bool>,
    title: impl Into<String>,
    message: impl Into<String>,
    button_label: impl Into<String>,
) -> impl View {
    use super::button::button;
    use crate::dom::TextAlign;

    let title = title.into();
    let message = message.into();
    let button_label = button_label.into();
    modal(show, move || show.set(false), move || {
        let title2 = title.clone();
        let message2 = message.clone();
        let button_label2 = button_label.clone();
        column((
            boxed(
                text(title2)
                    .font_size(17.0)
                    .color(Color::rgb(0, 0, 0))
                    .align(TextAlign::Center),
            ),
            boxed(
                text(message2)
                    .font_size(13.0)
                    .color(Color::rgba(0, 0, 0, 153))
                    .align(TextAlign::Center),
            ),
            boxed(button(button_label2, move || show.set(false))),
        ))
        .gap(12.0)
        .padding(20.0)
        .background(Color::rgb(255, 255, 255))
        .corner_radius(14.0)
    })
}

// ---------------------------------------------------------------------------
// Modal — full-screen dimmed overlay
// ---------------------------------------------------------------------------

/// Shows `content` as a centered modal overlay when `show` is `true`.
///
/// Tapping the dim background calls `on_dismiss`. The overlay sits in the
/// normal layout flow at zero size when hidden, so it does not affect siblings;
/// when shown it expands (flex-grow 1) and covers the parent container.
///
/// ```
/// use super::view::modal;
/// use crate::reactive::create_signal;
///
/// let open = create_signal(false);
/// let v = modal(open, move || open.set(false), || super::view::text("Hello"));
/// ```
pub fn modal<V: View + 'static>(
    show: Signal<bool>,
    on_dismiss: impl Fn() + Clone + 'static,
    content: impl Fn() -> V + 'static,
) -> impl View {
    dynamic(move || {
        if !show.get() {
            return boxed(column(()).size(0.0, 0.0));
        }
        let dismiss = on_dismiss.clone();
        boxed(
            column((boxed(
                column((boxed(content()),))
                    .align(AlignItems::Center)
                    .justify(JustifyContent::Center),
            ),))
            .grow()
            .align(AlignItems::Center)
            .justify(JustifyContent::Center)
            .background(Color::rgba(0, 0, 0, 180))
            .on_tap(move || dismiss()),
        )
    })
    .grow(0.0)
}

// ---------------------------------------------------------------------------
// Fade transition — animated opacity wrapper
// ---------------------------------------------------------------------------

/// Wraps `content` (produced by a closure) in a fade that animates opacity
/// between `0.0` (hidden) and `1.0` (visible) as `show` changes.
///
/// The view **occupies space even when invisible** — use [`dynamic`] / [`show`]
/// if you want to unmount the content when it hides.
///
/// # Example
/// ```rust
/// use super::view::{fade_transition, text};
/// use crate::reactive::create_signal;
///
/// let visible = create_signal(true);
/// let v = fade_transition(visible, || text("Hello, world!"));
/// ```
pub fn fade_transition<V: View + 'static>(
    show: Signal<bool>,
    content: impl Fn() -> V + 'static,
) -> impl View {
    use crate::anim::{animate, Easing};

    // Start at the correct opacity for the current state.
    let initial = if show.get() { 1.0f32 } else { 0.0f32 };
    let opacity = create_signal(initial);

    // When `show` flips, kick off a new 300 ms tween and relay its value
    // into `opacity` on every tick (via a nested effect).
    create_effect(move || {
        let target = if show.get() { 1.0f32 } else { 0.0f32 };
        let anim_sig = animate(opacity.get(), target, 0.3, Easing::EaseInOut);
        create_effect(move || opacity.set(anim_sig.get()));
    });

    dynamic(move || {
        boxed(column((boxed(content()),)).opacity(opacity.get()))
    })
}

// ---------------------------------------------------------------------------
// Bottom sheet — slide-up panel
// ---------------------------------------------------------------------------

/// Shows `content` in a bottom sheet when `show` is `true`.
///
/// Tapping the translucent dim area above the panel calls `on_dismiss`.
///
/// ```
/// use super::view::bottom_sheet;
/// use crate::reactive::create_signal;
///
/// let open = create_signal(false);
/// let v = bottom_sheet(open, move || open.set(false), || super::view::text("Sheet body"));
/// ```
pub fn bottom_sheet<V: View + 'static>(
    show: Signal<bool>,
    on_dismiss: impl Fn() + Clone + 'static,
    content: impl Fn() -> V + 'static,
) -> impl View {
    dynamic(move || {
        if !show.get() {
            return boxed(column(()).size(0.0, 0.0));
        }
        let dismiss = on_dismiss.clone();
        boxed(
            column((
                boxed(column(()).grow().on_tap(move || dismiss())),
                boxed(
                    column((boxed(content()),))
                        .background(Color::WHITE)
                        .corner_radius(16.0)
                        .padding(20.0),
                ),
            ))
            .grow()
            .background(Color::rgba(0, 0, 0, 150)),
        )
    })
    .grow(0.0)
}

// ---------------------------------------------------------------------------
// Toast / Snackbar
// ---------------------------------------------------------------------------

/// Renders a toast bar when `message` contains `Some(text)`, nothing when `None`.
///
/// Position and animation are the caller's responsibility (e.g. wrap in a
/// `stack` overlay pinned to the bottom with margin).
///
/// ```
/// use super::view::toast;
/// use crate::reactive::create_signal;
///
/// let msg: crate::reactive::Signal<Option<String>> = create_signal(None);
/// let v = toast(msg);
/// ```
pub fn toast(message: Signal<Option<String>>) -> impl View {
    dynamic(move || match message.get() {
        None => boxed(column(()).size(0.0, 0.0)),
        Some(msg) => boxed(
            row((boxed(text(msg).font_size(14.0).color(Color::WHITE).grow(1.0)),))
                .padding(14.0)
                .background(Color::rgb(30, 30, 40))
                .corner_radius(10.0),
        ),
    })
    .grow(0.0)
}

// ---------------------------------------------------------------------------
// Item separator — horizontal rule for list items
// ---------------------------------------------------------------------------

/// A horizontal separator suitable for use between list items.
///
/// `inset` adds left padding so the line starts at the same x-position as
/// list content — matching the default iOS separator inset.
///
/// # Example
/// ```rust
/// use super::view::item_separator;
/// use crate::core::Color;
///
/// let sep = item_separator(Color::rgba(0, 0, 0, 51), 16.0);
/// ```
pub fn item_separator(color: Color, inset: f32) -> impl View {
    row((
        boxed(column(()).size(inset, 1.0)),
        boxed(column(()).background(color).height(1.0).grow(1.0)),
    ))
}

// ---------------------------------------------------------------------------
// Picker
// ---------------------------------------------------------------------------

/// A vertical list of labelled rows where the currently-selected item is
/// highlighted with a checkmark.
///
/// ```
/// use super::view::picker;
///
/// let view = picker(
///     vec!["Apple".to_string(), "Banana".to_string()],
///     0,
///     |i| println!("picked {i}"),
/// );
/// ```
pub fn picker(
    options: Vec<String>,
    selected: usize,
    on_select: impl Fn(usize) + Clone + 'static,
) -> impl View {
    let rows: Vec<BoxedView> = options
        .into_iter()
        .enumerate()
        .map(|(i, label)| {
            let on_select = on_select.clone();
            let is_selected = i == selected;
            let checkmark: BoxedView = if is_selected {
                boxed(icon("checkmark").tint(DEFAULT_TINT).size(16.0, 16.0))
            } else {
                boxed(column(()).size(16.0, 16.0))
            };
            boxed(
                row((
                    boxed(text(label).font_size(16.0).grow(1.0)),
                    checkmark,
                ))
                .padding(14.0)
                .align(AlignItems::Center)
                .on_tap(move || on_select(i)),
            )
        })
        .collect();
    column(rows).corner_radius(10.0).background(Color::WHITE)
}

// ---------------------------------------------------------------------------
// Grid layout
// ---------------------------------------------------------------------------

/// Arranges `items` in a `columns`-wide grid with uniform `gap` spacing.
///
/// Items fill left-to-right; a new row is started every `columns` items.
/// An incomplete final row is left-aligned (not stretched).
///
/// # Example
/// ```rust
/// use super::view::{grid, text, boxed};
///
/// let cells: Vec<_> = (0..9).map(|i| boxed(text(format!("Cell {i}")))).collect();
/// let view = grid(3, 8.0, cells);
/// ```
pub fn grid(columns: usize, gap: f32, items: Vec<BoxedView>) -> impl View {
    let mut rows: Vec<BoxedView> = Vec::new();
    let mut current_row: Vec<BoxedView> = Vec::new();

    for item in items {
        current_row.push(item);
        if current_row.len() == columns {
            let row_items = std::mem::take(&mut current_row);
            rows.push(boxed(row(row_items).gap(gap)));
        }
    }
    if !current_row.is_empty() {
        rows.push(boxed(row(current_row).gap(gap)));
    }

    column(rows).gap(gap)
}

// ---------------------------------------------------------------------------
// Network Image
// ---------------------------------------------------------------------------

/// An image that loads from a URL via the HTTP client. Shows `placeholder` (an
/// SF Symbol name or asset name) while loading, then displays the fetched image.
///
/// Requires `rax-net` to be configured (automatically done on iOS via `rax::run`).
///
/// ```
/// use super::view::network_image;
///
/// let img = network_image("https://example.com/photo.jpg", "photo");
/// ```
pub fn network_image(url: impl Into<String>, placeholder: impl Into<String>) -> impl View {
    let url = url.into();
    let placeholder = placeholder.into();
    let bytes = create_signal::<Option<std::sync::Arc<Vec<u8>>>>(None);

    // Kick off the fetch.
    let res = crate::net::get(url);
    create_effect(move || {
        use crate::async_rt::ResourceState;
        if let ResourceState::Ready(resp) = res.get() {
            if !resp.body_bytes.is_empty() {
                bytes.set(Some(std::sync::Arc::new(resp.body_bytes.clone())));
            }
        }
    });

    dynamic(move || match bytes.get() {
        Some(data) => boxed(image("").data(data)),
        None => boxed(image(placeholder.clone())),
    })
}

// ---------------------------------------------------------------------------
// AppBar / Toolbar
// ---------------------------------------------------------------------------

/// A navigation bar with a `title`, an optional back button, and trailing
/// action buttons.
///
/// # Example
/// ```rust
/// use super::view::{app_bar, text};
/// use crate::core::Color;
///
/// let bar = app_bar(
///     "Settings",
///     None::<(&str, fn())>,
///     vec![],
///     Color::BLACK,
///     Color::rgb(245, 245, 245),
/// );
/// ```
pub fn app_bar(
    title: impl Into<String>,
    back: Option<(impl Into<String> + 'static, impl Fn() + 'static)>,
    actions: Vec<(String, Box<dyn Fn()>)>,
    text_color: Color,
    bg_color: Color,
) -> impl View {
    use super::button::button;

    let title = title.into();

    let back_view: BoxedView = if let Some((label, action)) = back {
        boxed(button(label.into(), action))
    } else {
        boxed(column(()).size(0.0, 0.0))
    };

    let action_views: Vec<BoxedView> = actions
        .into_iter()
        .map(|(label, action)| {
            let action = Box::new(action);
            boxed(button(label, move || action()))
        })
        .collect();

    row((
        back_view,
        boxed(
            super::text::text(title)
                .font_size(17.0)
                .color(text_color)
                .grow(1.0),
        ),
        boxed(column(action_views).gap(8.0)),
    ))
    .gap(8.0)
    .align(AlignItems::Center)
    .padding(12.0)
    .background(bg_color)
}

// ---------------------------------------------------------------------------
// ActionSheet — bottom sheet with a list of labeled action buttons
// ---------------------------------------------------------------------------

/// An action sheet showing a list of action buttons and a cancel button.
///
/// `show` controls visibility; each action is a `(label, callback)` pair using
/// `Arc<dyn Fn()>` so the closures can be cloned into the rendered children.
/// The cancel button always hides the sheet. Each action callback is called
/// before closing the sheet.
///
/// # Example
/// ```rust
/// use super::view::action_sheet;
/// use crate::reactive::create_signal;
/// use std::sync::Arc;
///
/// let open = create_signal(false);
/// let v = action_sheet(
///     open,
///     Some("Choose an action".to_string()),
///     vec![
///         ("Delete".to_string(), Arc::new(|| println!("Deleted")) as Arc<dyn Fn() + Send + Sync>),
///     ],
/// );
/// ```
pub fn action_sheet(
    show: Signal<bool>,
    title: Option<String>,
    actions: Vec<(String, std::sync::Arc<dyn Fn() + Send + Sync>)>,
) -> impl View {
    use super::button::button;
    use crate::dom::TextAlign;

    bottom_sheet(show, move || show.set(false), move || {
        let mut children: Vec<BoxedView> = vec![];

        if let Some(ref t) = title {
            children.push(boxed(
                text(t.clone())
                    .font_size(13.0)
                    .color(Color::rgba(0, 0, 0, 128))
                    .align(TextAlign::Center),
            ));
        }

        for (label, action) in &actions {
            let action = action.clone();
            let show2 = show;
            children.push(boxed(item_separator(Color::rgba(0, 0, 0, 26), 0.0)));
            children.push(boxed(
                column((boxed(button(label.clone(), move || {
                    action();
                    show2.set(false);
                })),))
                .padding(16.0),
            ));
        }

        // Cancel button
        children.push(boxed(item_separator(Color::rgba(0, 0, 0, 51), 0.0)));
        children.push(boxed(
            column((boxed(button("Cancel", move || show.set(false))),)).padding(16.0),
        ));

        column(children).padding(8.0)
    })
}

// ---------------------------------------------------------------------------
// Drawer / SideMenu — slides in from the left
// ---------------------------------------------------------------------------

/// A side drawer that slides in from the left. `show` controls visibility;
/// `on_dismiss` is called when the scrim is tapped.
/// `content` renders the drawer body; `width` is the drawer width in points.
pub fn drawer<V: View + 'static>(
    show: Signal<bool>,
    on_dismiss: impl Fn() + Clone + 'static,
    width: f32,
    content: impl Fn() -> V + 'static,
) -> impl View {
    use super::container::stack;
    dynamic(move || {
        if !show.get() {
            return boxed(column(()).size(0.0, 0.0));
        }
        let on_dismiss2 = on_dismiss.clone();
        boxed(
            stack((
                // Scrim — full-screen dim that closes the drawer on tap
                boxed(
                    column(())
                        .grow()
                        .background(Color::rgba(0, 0, 0, 102))
                        .on_tap(move || on_dismiss2()),
                ),
                // Drawer panel — flush left, full height
                boxed(
                    column((boxed(content()),))
                        .size(width, 0.0)
                        .grow(1.0)
                        .background(Color::rgb(255, 255, 255)),
                ),
            ))
            .grow(),
        )
    })
    .grow(0.0)
}

// ---------------------------------------------------------------------------
// Error overlay — dev-mode panic display
// ---------------------------------------------------------------------------

/// Shows a red error overlay when `message` is `Some`. Typically used with
/// [`crate::runtime::last_panic`] to surface panics as a visible overlay in
/// debug builds rather than silently freezing the app.
///
/// Place at the **top** of your root view tree (e.g. in a `stack`) so it
/// always renders above your other content:
///
/// ```no_run
/// use super::view::{error_overlay, stack};
/// use crate::reactive::create_signal;
///
/// // let msg = create_signal(crate::runtime::last_panic());
/// // let view = stack((your_app_view, error_overlay(msg)));
/// ```
pub fn error_overlay(message: crate::reactive::Signal<Option<String>>) -> impl View {
    dynamic(move || match message.get() {
        Some(msg) => boxed(
            column((
                boxed(
                    text("Panic")
                        .font_size(18.0)
                        .color(Color::rgb(255, 255, 255)),
                ),
                boxed(
                    text(msg)
                        .font_size(12.0)
                        .color(Color::rgba(255, 204, 204, 255)),
                ),
            ))
            .padding(16.0)
            .background(Color::rgba(204, 0, 0, 242))
            .grow(),
        ),
        None => boxed(column(()).size(0.0, 0.0)),
    })
}

// ---------------------------------------------------------------------------
// SwipeActions — swipe-to-reveal trailing action buttons
// ---------------------------------------------------------------------------

/// A list row with trailing swipe-to-reveal action buttons.
///
/// Pan left to reveal the `actions` buttons. Release past the halfway point to
/// snap open; release before halfway (or let go near closed) to snap back.
/// Each action is a `(label, color, callback)` triple; tapping an action button
/// calls the callback and closes the row. The content is produced by a closure
/// so it can be rebuilt inside a reactive `dynamic`.
///
/// # Example
/// ```rust
/// use super::view::swipe_actions;
/// use crate::core::Color;
/// use std::sync::Arc;
///
/// let view = swipe_actions(
///     || super::view::text("My item"),
///     vec![
///         ("Delete".to_string(), Color::rgb(255, 51, 51), Arc::new(|| println!("deleted")) as Arc<dyn Fn() + Send + Sync>),
///     ],
/// );
/// ```
pub fn swipe_actions(
    content: impl Fn() -> BoxedView + 'static,
    actions: Vec<(String, Color, std::sync::Arc<dyn Fn() + Send + Sync>)>,
) -> impl View {
    use crate::anim::{animate, Easing};
    use super::modifier::ViewExt;
    use crate::dom::Transform;

    let offset_x = create_signal(0.0f32);
    let action_width = 80.0f32 * actions.len() as f32;

    // Build the action buttons (right-side panel, initially hidden by the
    // content sitting over them).
    let action_buttons: Vec<BoxedView> = actions
        .iter()
        .map(|(label, color, action)| {
            let action = action.clone();
            let label = label.clone();
            let bg = *color;
            let offset = offset_x;
            boxed(
                column((boxed(
                    super::button::button(label, move || {
                        action();
                        offset.set(0.0);
                    }),
                ),))
                .background(bg)
                .grow_by(1.0),
            )
        })
        .collect();

    // Layout: a row whose right panel is exactly `action_width` wide and whose
    // content panel fills the remaining space. We slide the *content* panel
    // left using `transform_fn` — which does not disturb flex layout — so the
    // action panel becomes visible behind it as the user swipes.
    row((
        boxed(
            dynamic(move || {
                boxed(
                    column((boxed(content()),))
                        .grow()
                        .transform_fn(move || {
                            Transform::IDENTITY.translate(offset_x.get(), 0.0)
                        }),
                )
            })
            .grow(1.0),
        ),
        boxed(
            row(action_buttons)
                .width(action_width),
        ),
    ))
    .on_pan(move |info: PanInfo| {
        // Accumulate delta onto the current offset; clamp to [−action_width, 0].
        let current = offset_x.get();
        let new_val = (current + info.translation.x * 0.5).clamp(-action_width, 0.0);
        offset_x.set(new_val);

        if info.phase == GesturePhase::Ended {
            // Snap: past half-way → open; otherwise → closed.
            let target = if new_val < -action_width / 2.0 {
                -action_width
            } else {
                0.0
            };
            let anim = animate(new_val, target, 0.2, Easing::EaseOut);
            create_effect(move || offset_x.set(anim.get()));
        }
    })
}

// ---------------------------------------------------------------------------
// DevTools overlay
// ---------------------------------------------------------------------------

/// A lightweight developer badge overlay. In debug builds renders a small
/// translucent "rax [debug]" pill in the bottom-right corner of the parent.
/// In release builds this is a zero-sized, zero-cost view.
///
/// Place inside a `stack()` over your app content:
///
/// ```rust
/// use super::view::{dev_tools, stack, text};
///
/// let v = stack((text("App content"), dev_tools()));
/// ```
pub fn dev_tools() -> BoxedView {
    if cfg!(debug_assertions) {
        let fps = super::use_fps();
        boxed(
            dynamic(move || {
                let fps_val = fps.get();
                boxed(
                    row((
                        boxed(
                            text(move || format!("rax [debug] {:.0}fps", fps_val))
                                .font_size(10.0)
                                .color(Color::rgb(255, 255, 255)),
                        ),
                    ))
                    .padding(4.0)
                    .background(Color::rgba(0, 0, 0, 153))
                    .corner_radius(4.0),
                )
            })
        )
    } else {
        boxed(column(()).size(0.0, 0.0))
    }
}

// ---------------------------------------------------------------------------
// SectionList — scrollable list with section headers
// ---------------------------------------------------------------------------

/// A section descriptor for [`section_list`].
pub struct Section {
    /// The header label text for this section.
    pub header: String,
    /// The items belonging to this section.
    pub items: Vec<BoxedView>,
}

impl Section {
    /// Constructs a new section with the given header and items.
    pub fn new(header: impl Into<String>, items: Vec<BoxedView>) -> Self {
        Section {
            header: header.into(),
            items,
        }
    }
}

/// A scrollable list with section headers. Each section header is a styled
/// label; items follow below it. On iOS the headers visually separate sections
/// (true sticky header pinning requires UICollectionView — planned).
///
/// # Example
/// ```rust
/// use super::view::{section_list, Section, text, boxed, row, spacer};
/// use crate::core::Color;
///
/// section_list(
///     vec![
///         Section::new("Fruits", vec![boxed(row((text("Apple"), spacer())))]),
///         Section::new("Vegetables", vec![boxed(row((text("Carrot"), spacer())))]),
///     ],
///     Color::rgba(0, 0, 0, 128),
///     Color::rgba(0, 0, 0, 13),
/// )
/// # ;
/// ```
pub fn section_list(
    sections: Vec<Section>,
    header_text_color: Color,
    header_bg_color: Color,
) -> impl View {
    use crate::core::EdgeInsets;
    let mut all_rows: Vec<BoxedView> = Vec::new();
    for section in sections {
        // Section header
        all_rows.push(boxed(
            row((boxed(
                text(section.header)
                    .font_size(13.0)
                    .color(header_text_color),
            ),))
            .padding_insets(EdgeInsets {
                top: 4.0,
                bottom: 4.0,
                left: 16.0,
                right: 16.0,
            })
            .background(header_bg_color),
        ));
        // Section items
        for item in section.items {
            all_rows.push(item);
        }
    }
    scroll(column(all_rows))
}

// ---------------------------------------------------------------------------
// LazyColumn / LazyRow — scrolling list with fine-grained reactivity
// ---------------------------------------------------------------------------

/// A vertically scrolling list that renders `count` items using `item_builder`.
///
/// Each item is built once on first render. This is **not** a fully virtualized
/// (recycling) list — all items are in the DOM — but it uses fine-grained
/// reactivity to avoid re-renders when unrelated signals change.
///
/// For true UITableView-backed recycling, a native `LazyList` widget is planned
/// as future work.
///
/// # Example
/// ```rust
/// use super::view::{lazy_column, text, boxed};
///
/// let v = lazy_column(100, |i| boxed(text(format!("Item {}", i))));
/// ```
pub fn lazy_column(count: usize, item_builder: impl Fn(usize) -> BoxedView + 'static) -> impl View {
    let items: Vec<BoxedView> = (0..count).map(|i| item_builder(i)).collect();
    scroll(column(items).gap(0.0))
}

/// Same as [`lazy_column`] but scrolls horizontally.
pub fn lazy_row(count: usize, item_builder: impl Fn(usize) -> BoxedView + 'static) -> impl View {
    let items: Vec<BoxedView> = (0..count).map(|i| item_builder(i)).collect();
    scroll(column(items).gap(0.0)).horizontal()
}

/// A reactive list that rebuilds efficiently when `items` changes.
///
/// Uses [`dynamic`] so the entire list re-renders only when the signal changes.
/// Individual item builders are called on every rebuild; wrap item content in
/// further signals for sub-item fine-grained updates.
///
/// # Example
/// ```rust
/// use super::view::{reactive_list, text, boxed};
/// use crate::reactive::create_signal;
///
/// let items = create_signal(vec!["Alice".to_string(), "Bob".to_string()]);
/// let v = reactive_list(items, |_i, name| boxed(text(name)));
/// ```
pub fn reactive_list<T: Clone + 'static>(
    items: Signal<Vec<T>>,
    item_builder: impl Fn(usize, T) -> BoxedView + 'static,
) -> impl View {
    dynamic(move || {
        let current = items.get();
        let views: Vec<BoxedView> = current
            .into_iter()
            .enumerate()
            .map(|(i, item)| item_builder(i, item))
            .collect();
        boxed(scroll(column(views)))
    })
}

/// Returns a `(x_signal, y_signal, handler)` triple for gesture-driven animation.
///
/// Pass `handler` to `.on_pan()` on a view; use `x_signal.get()` and
/// `y_signal.get()` for transforms or offsets. When `spring_back` is true, both
/// signals animate back to `0.0` (via a spring) when the gesture ends.
///
/// # Example
/// ```rust
/// let (offset_x, offset_y, pan_handler) = pan_animation(true);
/// column(content())
///     .on_pan(pan_handler)
///     .translate(move || offset_x.get(), move || offset_y.get())
/// ```
pub fn pan_animation(spring_back: bool) -> (Signal<f32>, Signal<f32>, impl FnMut(PanInfo)) {
    let x = create_signal(0.0f32);
    let y = create_signal(0.0f32);
    let handler = move |info: PanInfo| {
        x.set(info.translation.x);
        y.set(info.translation.y);
        if spring_back && info.phase == GesturePhase::Ended {
            // Animate back to 0 via spring physics. Create a spring signal
            // from the current offset to 0 and relay each frame into x/y.
            let sx = x;
            let spring_x = crate::anim::spring(sx.get(), 0.0, crate::anim::Spring::default());
            create_effect(move || sx.set(spring_x.get()));
            let sy = y;
            let spring_y = crate::anim::spring(sy.get(), 0.0, crate::anim::Spring::default());
            create_effect(move || sy.set(spring_y.get()));
        }
    };
    (x, y, handler)
}

// ---------------------------------------------------------------------------
// Wrap — flow layout (row with flex wrap)
// ---------------------------------------------------------------------------

/// A flowing, wrapping row of items with uniform `gap` between them.
///
/// Items are laid out left-to-right; when a row is full they wrap onto the
/// next line — equivalent to CSS `flex-wrap: wrap`.
///
/// # Example
/// ```rust
/// use super::view::{wrap, chip};
/// use crate::reactive::create_signal;
///
/// let selected = create_signal(0usize);
/// let tags: Vec<_> = (0..8)
///     .map(|i| super::view::boxed(chip(format!("Tag {i}"), selected.get() == i, move || selected.set(i))))
///     .collect();
/// let v = wrap(8.0, tags);
/// ```
pub fn wrap(gap: f32, items: Vec<BoxedView>) -> impl View {
    row(items).gap(gap).wrap()
}

// ---------------------------------------------------------------------------
// Pressable — tappable wrapper with reactive opacity feedback
// ---------------------------------------------------------------------------

/// A wrapper that dims its content to `0.4` opacity while the user's finger is
/// down, then restores full opacity on release, before calling `on_press`.
///
/// Unlike a raw `.on_tap()`, `pressable` gives tactile visual feedback for any
/// arbitrary content.
///
/// # Example
/// ```rust
/// use super::view::{pressable, text};
///
/// let v = pressable(text("Tap me"), || println!("pressed"));
/// ```
pub fn pressable<V: View + 'static>(content: V, on_press: impl Fn() + 'static) -> impl View {
    let pressed = create_signal(false);

    // `opacity_fn` re-reads `pressed` reactively on every frame that the
    // signal changes, giving us a zero-cost pressed-state without rebuild.
    column((boxed(
        column((boxed(content),))
            .opacity_fn(move || if pressed.get() { 0.4 } else { 1.0 })
            .on_tap(move || {
                on_press();
            }),
    ),))
    .on_pan(move |info: PanInfo| {
        // Track finger-down / finger-up via the pan gesture began/ended phases.
        // A pure tap has no pan events, so we also handle the tap above.
        // The pan handler dims on Began and restores on Ended.
        match info.phase {
            GesturePhase::Began => pressed.set(true),
            GesturePhase::Ended => pressed.set(false),
            GesturePhase::Changed => {}
        }
    })
}

// ---------------------------------------------------------------------------
// Skeleton — shimmer loading placeholder
// ---------------------------------------------------------------------------

/// An animated shimmer box used as a content placeholder while data loads.
///
/// The opacity oscillates between `0.4` and `1.0` on a 1-second ease-in-out
/// cycle to mimic the standard shimmer effect. Supply `width` and `height` in
/// points; `corner_radius` defaults to `8`.
///
/// # Example
/// ```rust
/// use super::view::skeleton;
///
/// let placeholder = skeleton(200.0, 20.0);
/// ```
pub fn skeleton(width: f32, height: f32) -> Skeleton {
    Skeleton {
        width,
        height,
        color: Color::rgb(224, 224, 224),
        radius: 8.0,
    }
}

/// A shimmer loading placeholder. Build via [`skeleton`].
pub struct Skeleton {
    width: f32,
    height: f32,
    color: Color,
    radius: f32,
}

impl Skeleton {
    /// Overrides the placeholder fill color (default `#E0E0E0`).
    #[must_use]
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Overrides the corner radius (default `8`).
    #[must_use]
    pub fn corner_radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }
}

impl View for Skeleton {
    fn build(self, tree: &mut Tree) -> WidgetId {
        use crate::anim::{animate, Easing};

        // Two chained 1-second tweens give an infinite oscillation: bright →
        // dim → bright. We drive them by a local signal and chain effects.
        let opacity = create_signal(1.0f32);
        let color = self.color;
        let radius = self.radius;

        // Kick off the first leg of the oscillation (bright → dim).
        let dim = animate(1.0f32, 0.4, 1.0, Easing::EaseInOut);
        create_effect(move || {
            let v = dim.get();
            opacity.set(v);
            // When the dim leg completes, start the brighten leg.
            if (v - 0.4).abs() < 0.01 {
                let brighten = animate(0.4f32, 1.0, 1.0, Easing::EaseInOut);
                create_effect(move || opacity.set(brighten.get()));
            }
        });

        column(())
            .size(self.width, self.height)
            .background(color)
            .corner_radius(radius)
            .opacity_fn(move || opacity.get())
            .build(tree)
    }
}

// ---------------------------------------------------------------------------
// Banner — inline alert strip (info / success / warning / error)
// ---------------------------------------------------------------------------

/// The semantic kind of a [`banner`], controlling its color scheme.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BannerKind {
    /// Neutral informational message (blue).
    Info,
    /// Positive confirmation (green).
    Success,
    /// Non-critical warning (amber).
    Warning,
    /// Error or destructive action (red).
    Error,
}

/// Shows a styled inline alert strip when `visible` is `true`.
///
/// The strip is zero-height when hidden so it does not displace siblings.
///
/// # Example
/// ```rust
/// use super::view::{banner, BannerKind};
/// use crate::reactive::create_signal;
///
/// let visible = create_signal(true);
/// let v = banner(visible, "Your changes were saved.", BannerKind::Success);
/// ```
pub fn banner(
    visible: Signal<bool>,
    message: impl Into<String>,
    kind: BannerKind,
) -> impl View {
    let message = message.into();

    let (bg, icon_sym, fg) = match kind {
        BannerKind::Info    => (Color::rgba(0, 122, 255, 26),  "info.circle.fill",            Color::rgb(0, 122, 255)),
        BannerKind::Success => (Color::rgba(52, 199, 89, 26),  "checkmark.circle.fill",       Color::rgb(52, 199, 89)),
        BannerKind::Warning => (Color::rgba(255, 149, 0, 26),  "exclamationmark.triangle.fill", Color::rgb(255, 149, 0)),
        BannerKind::Error   => (Color::rgba(255, 59, 48, 26),  "xmark.circle.fill",           Color::rgb(255, 59, 48)),
    };

    dynamic(move || {
        if !visible.get() {
            return boxed(column(()).size(0.0, 0.0));
        }
        let msg2 = message.clone();
        boxed(
            row((
                boxed(icon(icon_sym).tint(fg).size(18.0, 18.0)),
                boxed(text(msg2).font_size(14.0).color(fg).grow(1.0)),
            ))
            .gap(8.0)
            .padding(12.0)
            .align(AlignItems::Center)
            .background(bg)
            .corner_radius(10.0),
        )
    })
    .grow(0.0)
}

// ---------------------------------------------------------------------------
// Collapsible — disclosure / accordion widget
// ---------------------------------------------------------------------------

/// A disclosure widget: tapping `header` toggles the `content` section.
///
/// `expanded` is an externally-owned signal so callers can control or observe
/// the open/closed state (e.g. to implement an exclusive accordion).
///
/// # Example
/// ```rust
/// use super::view::{collapsible, text};
/// use crate::reactive::create_signal;
///
/// let open = create_signal(false);
/// let v = collapsible(
///     text("Section title"),
///     open,
///     || text("Hidden body content"),
/// );
/// ```
pub fn collapsible<H, C, V>(
    header: H,
    expanded: Signal<bool>,
    content: C,
) -> impl View
where
    H: View + 'static,
    C: Fn() -> V + 'static,
    V: View + 'static,
{
    column((
        // Header row with a trailing chevron that rotates 90° when open.
        boxed(
            row((
                boxed(boxed(header).grow(1.0)),
                boxed(dynamic(move || {
                    let sym = if expanded.get() {
                        "chevron.up"
                    } else {
                        "chevron.down"
                    };
                    boxed(icon(sym).tint(Color::rgba(0, 0, 0, 128)).size(12.0, 12.0))
                })),
            ))
            .gap(8.0)
            .align(AlignItems::Center)
            .padding(12.0)
            .on_tap(move || expanded.update(|e| *e = !*e)),
        ),
        // Body — zero-height when collapsed, built lazily each open.
        boxed(show(move || expanded.get(), content)),
    ))
}

// ---------------------------------------------------------------------------
// Carousel — horizontal paged scroller
// ---------------------------------------------------------------------------

/// A horizontally-scrolling carousel of items built from a reactive `Vec`.
///
/// Each item in the signal is rendered by `item_fn`. The carousel rebuilds
/// whenever the signal changes. Items are spaced by `gap` points.
///
/// # Example
/// ```rust
/// use super::view::{carousel, text, boxed};
/// use crate::reactive::create_signal;
///
/// let pages = create_signal(vec!["Page 1".to_string(), "Page 2".to_string()]);
/// let v = carousel(pages, 12.0, |page| boxed(text(page)));
/// ```
pub fn carousel<T, F, V>(
    items: Signal<Vec<T>>,
    gap: f32,
    item_fn: F,
) -> impl View
where
    T: Clone + 'static,
    F: Fn(T) -> V + 'static,
    V: View + 'static,
{
    dynamic(move || {
        let current = items.get();
        let views: Vec<BoxedView> = current.into_iter().map(|item| boxed(item_fn(item))).collect();
        boxed(scroll(row(views).gap(gap)).horizontal().paging())
    })
    .grow(0.0)
}

// ---------------------------------------------------------------------------
// KeyboardAvoidingView — bottom-inset wrapper for soft keyboard
// ---------------------------------------------------------------------------

/// Wraps `content` in a vertically-scrolling container so that the soft
/// keyboard does not occlude input fields.
///
/// On iOS the underlying `UIScrollView` automatically adjusts its
/// `contentInset` when the keyboard appears (controlled by the platform
/// backend's `keyboardDismissMode` / `contentInsetAdjustmentBehavior`).
/// Wrapping content in a `scroll` is the minimal hook that lets the backend
/// apply that inset.
///
/// # Example
/// ```rust
/// use super::view::{keyboard_avoiding_view, text_input};
/// use crate::reactive::create_signal;
///
/// let query = create_signal(String::new());
/// let v = keyboard_avoiding_view(
///     text_input(query.get(), move |s| query.set(s))
/// );
/// ```
pub fn keyboard_avoiding_view<V: View>(content: V) -> impl View {
    scroll(content)
}

// ---------------------------------------------------------------------------
// InfiniteScroll — pull-to-load-more wrapper
// ---------------------------------------------------------------------------

/// Wraps `content` (produced by a closure) in a refreshable scroll view.
/// When the user pulls past the top edge `on_load_more` is called (e.g. to
/// fetch the next page). While `loading` is `true` the built-in spinner is
/// shown.
///
/// `content` is a closure so it can be rebuilt alongside the refreshing
/// state inside a [`dynamic`] context. If your content is static you can
/// pass `|| boxed(your_view)`.
///
/// # Example
/// ```rust
/// use super::view::{infinite_scroll, text, boxed};
/// use crate::reactive::create_signal;
///
/// let loading = create_signal(false);
/// let v = infinite_scroll(
///     || boxed(text("Content here")),
///     loading,
///     move || {
///         loading.set(true);
///         // … fetch next page, then loading.set(false)
///     },
/// );
/// ```
pub fn infinite_scroll<C>(
    content: C,
    loading: Signal<bool>,
    on_load_more: impl FnMut() + 'static,
) -> impl View
where
    C: Fn() -> BoxedView + 'static,
{
    // Wrap `on_load_more` in a clone-friendly Arc so it can be shared across
    // the `dynamic` rebuild closure without requiring `Clone` on `impl FnMut`.
    use std::sync::{Arc, Mutex};
    let cb = Arc::new(Mutex::new(on_load_more));

    dynamic(move || {
        let cb2 = cb.clone();
        let is_refreshing = loading.get();
        boxed(scroll(content()).refreshable(is_refreshing, move || {
            if let Ok(mut f) = cb2.lock() {
                f();
            }
        }))
    })
    .grow(1.0)
}

// ---------------------------------------------------------------------------
// StatusBarSpacer — top safe-area filler
// ---------------------------------------------------------------------------

/// A fixed-height spacer that fills the iOS status-bar safe area (44 pt on
/// notched devices, 20 pt on non-notched devices). This is a static
/// approximation; for a pixel-perfect inset wire up the actual safe-area
/// inset from the platform backend.
///
/// # Example
/// ```rust
/// use super::view::{status_bar_spacer, column, text};
///
/// let v = column((
///     super::view::boxed(status_bar_spacer()),
///     super::view::boxed(text("Content below status bar")),
/// ));
/// ```
pub fn status_bar_spacer() -> impl View {
    // 44 pt is the standard notch/Dynamic-Island safe-area top inset.
    column(()).height(44.0)
}

// ---------------------------------------------------------------------------
// list_with_header — header / footer / empty-state wrapper
// ---------------------------------------------------------------------------

/// A list with a header, footer, and empty-state view.
///
/// When `items_empty()` returns `true`, `empty_fn` is rendered in place of
/// `content_fn`. The `header_fn` and `footer_fn` are always rendered.
/// All four layout factories are closures so the views can be rebuilt
/// reactively inside a [`dynamic`] context without needing `Clone` on the
/// views themselves.
///
/// # Example
/// ```rust
/// use super::view::{list_with_header, text, boxed};
/// use crate::reactive::create_signal;
///
/// let items: crate::reactive::Signal<Vec<String>> = create_signal(vec![]);
/// let v = list_with_header(
///     || boxed(text("My List")),
///     || boxed(text("End of list")),
///     move || items.get().is_empty(),
///     || boxed(text("No items yet")),
///     || boxed(text("Item list goes here")),
/// );
/// ```
pub fn list_with_header(
    header_fn: impl Fn() -> BoxedView + 'static,
    footer_fn: impl Fn() -> BoxedView + 'static,
    items_empty: impl Fn() -> bool + 'static + Clone,
    empty_fn: impl Fn() -> BoxedView + 'static,
    content_fn: impl Fn() -> BoxedView + 'static,
) -> impl View {
    let items_empty2 = items_empty.clone();
    column((
        boxed(dynamic(move || header_fn())),
        boxed(show(move || !items_empty(), content_fn)),
        boxed(show(move || items_empty2(), empty_fn)),
        boxed(dynamic(move || footer_fn())),
    ))
}

// ---------------------------------------------------------------------------
// empty_state — placeholder for an empty list or section
// ---------------------------------------------------------------------------

/// A simple empty-state placeholder that displays `message` centered in its
/// container with muted styling.
///
/// # Example
/// ```rust
/// use super::view::empty_state;
///
/// let v = empty_state("No results found");
/// ```
pub fn empty_state(message: &'static str) -> impl View {
    use crate::dom::TextAlign;
    column((
        text(message)
            .color(Color::hex(0x9E9E9Eff))
            .align(TextAlign::Center),
    ))
    .padding(32.0)
    .align(AlignItems::Center)
    .justify(JustifyContent::Center)
}

// ---------------------------------------------------------------------------
// sticky_header — section-header wrapper
// ---------------------------------------------------------------------------

/// A sticky section-header wrapper. Visually equivalent to a UITableView
/// section header: light-grey background with standard vertical padding and
/// horizontal insets.
///
/// # Example
/// ```rust
/// use super::view::{sticky_header, text};
///
/// let v = sticky_header(text("SECTION A"));
/// ```
pub fn sticky_header<V: View + 'static>(content: V) -> impl View {
    column((boxed(content),))
        .padding_insets(EdgeInsets {
            top: 8.0,
            bottom: 8.0,
            left: 16.0,
            right: 16.0,
        })
        .background(Color::hex(0xF5F5F5ff))
}

// ---------------------------------------------------------------------------
// Tooltip
// ---------------------------------------------------------------------------

/// Wraps `content` in a tooltip that shows `message` as a dark bubble
/// on tap.
pub fn tooltip<V: View + 'static>(content: V, message: &'static str) -> impl View {
    let show = create_signal(false);
    column((
        boxed(
            boxed(content)
                .on_tap(move || show.update(|s| *s = !*s)),
        ),
        boxed(dynamic(move || {
            if show.get() {
                boxed(
                    column((boxed(text(message)
                        .color(Color::hex(0xFFFFFFff))
                        .font_size(12.0)),))
                        .padding(8.0)
                        .background(Color::hex(0x333333CC))
                        .corner_radius(6.0)
                        .on_tap(move || show.set(false)),
                )
            } else {
                boxed(column(()))
            }
        })),
    ))
}

// ---------------------------------------------------------------------------
// ColorPicker — palette of common color swatches
// ---------------------------------------------------------------------------

/// A simple color picker that displays a row of colour swatches.
///
/// Tapping a swatch writes the corresponding [`Color`] into `color`. `label`
/// is rendered above the palette row.
///
/// # Example
/// ```rust
/// use super::view::color_picker;
/// use crate::reactive::create_signal;
/// use crate::core::Color;
///
/// let color = create_signal(Color::hex(0xFF0000ff));
/// let v = color_picker(color, "Pick a color");
/// ```
pub fn color_picker(color: Signal<Color>, label: &'static str) -> impl View {
    let palette: &[u32] = &[
        0xFF0000ff, 0xFF6600ff, 0xFFCC00ff, 0x66CC00ff, 0x00CC66ff,
        0x0066FFff, 0x6600FFff, 0xFF00CCff, 0xFFFFFFff, 0x888888ff, 0x000000ff,
    ];
    let swatches: Vec<BoxedView> = palette
        .iter()
        .copied()
        .map(|c| {
            let col = Color::hex(c);
            boxed(
                column(())
                    .width(32.0)
                    .height(32.0)
                    .background(col)
                    .corner_radius(16.0)
                    .border(1.0, Color::hex(0xCCCCCCff))
                    .on_tap(move || color.set(col)),
            )
        })
        .collect();

    column((
        boxed(text(label).font_size(14.0)),
        boxed(row(swatches).gap(8.0).wrap()),
    ))
    .gap(8.0)
}

// ---------------------------------------------------------------------------
// RatingBar — star-based value input
// ---------------------------------------------------------------------------

/// A star-rating bar that allows the user to pick a value between `1` and `max`.
///
/// `value` holds the current rating (as a `f32` so fractional ratings are
/// representable). Tapping star `i` calls `on_change(i as f32)`.
///
/// # Example
/// ```rust
/// use super::view::rating_bar;
/// use crate::reactive::create_signal;
///
/// let rating = create_signal(3.0f32);
/// let v = rating_bar(rating, 5, move |v| rating.set(v));
/// ```
pub fn rating_bar(
    value: Signal<f32>,
    max: u32,
    on_change: impl Fn(f32) + 'static + Clone,
) -> impl View {
    let stars: Vec<BoxedView> = (1..=max)
        .map(|i| {
            let on_change = on_change.clone();
            let val = value;
            let fi = i as f32;
            boxed(
                boxed(dynamic(move || {
                    let v = val.get();
                    let star = if v >= fi { "★" } else { "☆" };
                    boxed(text(star).font_size(28.0).color(Color::hex(0xFFCC00ff)))
                }))
                .on_tap(move || on_change(fi)),
            )
        })
        .collect();
    row(stars).gap(4.0)
}

// ---------------------------------------------------------------------------
// StatusBar
// ---------------------------------------------------------------------------

use crate::dom::StatusBarStyle;

/// A zero-sized view that sets the status bar style. Place near the root.
pub fn status_bar(style: StatusBarStyle) -> impl View {
    use crate::dom::Attribute;
    column(()).decorate(move |tree, id| {
        tree.bind(id, move || Attribute::StatusBarStyle(style.clone()));
    })
}

// ---------------------------------------------------------------------------
// TabBar / BottomNavigation
// ---------------------------------------------------------------------------

/// A single tab in a [`tab_bar`].
pub struct TabItem {
    /// Short label shown under the icon.
    pub label: String,
    /// Optional icon name (from the icon set).
    pub icon: Option<String>,
    /// Content view for this tab.
    pub content: BoxedView,
}

impl TabItem {
    /// Create a tab with label and content.
    pub fn new(label: impl Into<String>, content: impl View + 'static) -> Self {
        Self { label: label.into(), icon: None, content: boxed(content) }
    }
    /// Add an icon name.
    pub fn icon(mut self, icon_name: impl Into<String>) -> Self {
        self.icon = Some(icon_name.into());
        self
    }
}

/// A bottom tab bar. Renders the selected tab's content above a row of tab buttons.
pub fn tab_bar(tabs: Vec<TabItem>, selected: Signal<usize>) -> impl View {
    let mut content_views: Vec<BoxedView> = Vec::new();
    let mut button_views: Vec<BoxedView> = Vec::new();

    for (i, tab) in tabs.into_iter().enumerate() {
        // Content pane — visible when active (opacity 0 = invisible but keeps layout)
        let sel_for_content = selected;
        content_views.push(boxed(
            boxed(tab.content)
                .opacity_fn(move || if sel_for_content.get() == i { 1.0 } else { 0.0 })
                .grow(1.0),
        ));

        // Tab button
        let label = tab.label.clone();
        let icon_name = tab.icon.clone();
        let sel_for_btn = selected;
        let btn_content: BoxedView = if let Some(ico) = icon_name {
            boxed(
                column((
                    boxed(text(ico.clone()).font_size(20.0)),
                    boxed(text(label).font_size(11.0)),
                ))
                .align(AlignItems::Center),
            )
        } else {
            boxed(text(label).font_size(13.0))
        };
        let btn = column((boxed(btn_content),))
            .grow_by(1.0)
            .padding(8.0)
            .on_tap(move || selected.set(i));
        button_views.push(boxed(
            boxed(btn)
                .background_fn(move || {
                    if sel_for_btn.get() == i {
                        Color::hex(0xE8F4FFff)
                    } else {
                        Color::hex(0xF9F9F9ff)
                    }
                }),
        ));
    }

    column((
        boxed(column(content_views).grow()),
        boxed(
            row(button_views)
                .background(Color::hex(0xF9F9F9ff))
                .border(1.0, Color::hex(0xE0E0E0ff)),
        ),
    ))
}

// ---------------------------------------------------------------------------
// SegmentedControl
// ---------------------------------------------------------------------------

/// A horizontal segmented control. `selected` is a reactive index signal.
pub fn segmented_control(options: Vec<&'static str>, selected: Signal<usize>) -> impl View {
    let btns: Vec<BoxedView> = options
        .into_iter()
        .enumerate()
        .map(|(i, label)| {
            let sel = selected;
            let seg = column((boxed(text(label).font_size(13.0)),))
                .grow_by(1.0)
                .padding(8.0)
                .on_tap(move || selected.set(i));
            boxed(
                boxed(seg)
                    .background_fn(move || {
                        if sel.get() == i { Color::hex(0x007AFFff) } else { Color::hex(0xE5E5EAff) }
                    }),
            )
        })
        .collect();
    row(btns)
        .corner_radius(8.0)
        .border(1.0, Color::hex(0xC8C8C8ff))
}

// ---------------------------------------------------------------------------
// Breadcrumbs
// ---------------------------------------------------------------------------

/// A horizontal breadcrumb trail. Items are tappable; the last is non-interactive.
pub fn breadcrumbs(items: Vec<&'static str>, on_tap: impl Fn(usize) + 'static + Clone) -> impl View {
    let count = items.len();
    let mut views: Vec<BoxedView> = Vec::new();
    for (i, label) in items.into_iter().enumerate() {
        let is_last = i == count - 1;
        let on_tap = on_tap.clone();
        let btn = boxed(text(label)
            .font_size(14.0)
            .color(if is_last { Color::hex(0x1C1C1Eff) } else { Color::hex(0x007AFFff) }));
        if is_last {
            views.push(btn);
        } else {
            views.push(boxed(
                boxed(btn).on_tap(move || on_tap(i)),
            ));
            views.push(boxed(
                text(" / ").font_size(14.0).color(Color::hex(0x8E8E93ff)),
            ));
        }
    }
    row(views).gap(0.0)
}

// ---------------------------------------------------------------------------
// Backdrop / Scrim
// ---------------------------------------------------------------------------

/// A semi-transparent full-area scrim overlay. Tapping it calls `on_tap`.
pub fn backdrop(opacity: f32, on_tap: impl Fn() + 'static) -> impl View {
    let alpha = (opacity.clamp(0.0, 1.0) * 255.0) as u32;
    // RRGGBBAA: 0x000000{alpha}
    let rgba = (0x000000u32 << 8) | alpha;
    column(())
        .grow()
        .background(Color::hex(rgba))
        .on_tap(on_tap)
}

// ---------------------------------------------------------------------------
// Accordion
// ---------------------------------------------------------------------------

/// A single section in an [`accordion`].
pub struct AccordionSection {
    /// Section header label.
    pub title: String,
    /// Body content shown when open.
    pub content: BoxedView,
}

impl AccordionSection {
    /// Create a section with title and content view.
    pub fn new(title: impl Into<String>, content: impl View + 'static) -> Self {
        Self { title: title.into(), content: boxed(content) }
    }
}

/// A single-open accordion. Only one section can be expanded at a time.
pub fn accordion(sections: Vec<AccordionSection>) -> impl View {
    let open: Signal<Option<usize>> = create_signal(None);

    let section_views: Vec<BoxedView> = sections
        .into_iter()
        .enumerate()
        .map(|(i, section)| {
            let title = section.title.clone();
            let content = section.content;
            let open_sig = open;

            // Header row
            let header = boxed(
                row((
                    boxed(text(title).font_size(15.0).grow(1.0)),
                    boxed(dynamic(move || {
                        let is_open = open_sig.get() == Some(i);
                        boxed(text(if is_open { "▲" } else { "▼" }).font_size(12.0))
                    })),
                ))
                .padding(16.0)
                .background(Color::hex(0xF2F2F7ff))
                .on_tap(move || {
                    open_sig.update(|o| {
                        *o = if *o == Some(i) { None } else { Some(i) };
                    });
                }),
            );

            // Body — always in tree, opacity gates visibility
            let body = boxed(
                column((content,))
                    .padding(16.0)
                    .opacity_fn(move || if open_sig.get() == Some(i) { 1.0 } else { 0.0 }),
            );

            boxed(column((header, body)))
        })
        .collect();

    column(section_views).gap(1.0)
}

// ---------------------------------------------------------------------------
// Multi-select list
// ---------------------------------------------------------------------------

/// A multi-select list: like [`picker`] but lets the user toggle any number of
/// options on or off. `is_selected(i)` is re-read reactively per row (so checks
/// update live), and `on_toggle(i)` is called when a row is tapped.
///
/// # Example
/// ```rust
/// use super::view::multi_select;
/// use crate::reactive::create_signal;
///
/// let chosen = create_signal(std::collections::HashSet::<usize>::new());
/// let view = multi_select(
///     vec!["Email".into(), "SMS".into(), "Push".into()],
///     move |i| chosen.get().contains(&i),
///     move |i| chosen.update(|s| { if !s.insert(i) { s.remove(&i); } }),
/// );
/// ```
pub fn multi_select(
    options: Vec<String>,
    is_selected: impl Fn(usize) -> bool + Clone + 'static,
    on_toggle: impl Fn(usize) + Clone + 'static,
) -> impl View {
    let rows: Vec<BoxedView> = options
        .into_iter()
        .enumerate()
        .map(|(i, label)| {
            let on_toggle = on_toggle.clone();
            let is_selected = is_selected.clone();
            boxed(
                row((
                    boxed(text(label).font_size(16.0).grow(1.0)),
                    boxed(dynamic(move || {
                        if is_selected(i) {
                            boxed(
                                icon("checkmark.circle.fill")
                                    .tint(DEFAULT_TINT)
                                    .size(22.0, 22.0),
                            )
                        } else {
                            boxed(
                                icon("circle")
                                    .tint(Color::hex(0xC7C7CCff))
                                    .size(22.0, 22.0),
                            )
                        }
                    })),
                ))
                .padding(14.0)
                .align(AlignItems::Center)
                .on_tap(move || on_toggle(i)),
            )
        })
        .collect();
    column(rows).corner_radius(10.0).background(Color::WHITE)
}

// ---------------------------------------------------------------------------
// Drag-to-reorder list
// ---------------------------------------------------------------------------

/// A vertically-stacked list whose rows can be dragged to reorder.
///
/// Each row is built by `render(i)`. While a row is dragged it follows the
/// finger; on release `on_reorder(from, to)` is called with the original and
/// target indices (a no-op if unchanged). `row_height` translates the drag
/// distance into a number of slots moved.
///
/// # Example
/// ```rust
/// use super::view::{reorderable_list, boxed, text};
///
/// let view = reorderable_list(
///     3,
///     48.0,
///     |i| boxed(text(format!("Item {i}"))),
///     |from, to| println!("moved {from} -> {to}"),
/// );
/// ```
pub fn reorderable_list(
    count: usize,
    row_height: f32,
    render: impl Fn(usize) -> BoxedView + 'static,
    on_reorder: impl Fn(usize, usize) + Clone + 'static,
) -> impl View {
    use crate::dom::Transform;

    let dragging = create_signal::<Option<usize>>(None);
    let drag_dy = create_signal(0.0f32);

    let rows: Vec<BoxedView> = (0..count)
        .map(|i| {
            let on_reorder = on_reorder.clone();
            let content = render(i);
            boxed(
                column((content,))
                    .grow_by(0.0)
                    .transform_fn(move || {
                        if dragging.get() == Some(i) {
                            Transform::IDENTITY.translate(0.0, drag_dy.get())
                        } else {
                            Transform::IDENTITY
                        }
                    })
                    .z_index(if dragging.get() == Some(i) { 1 } else { 0 })
                    .on_pan(move |info: PanInfo| match info.phase {
                        GesturePhase::Began => {
                            dragging.set(Some(i));
                            drag_dy.set(0.0);
                        }
                        GesturePhase::Changed => {
                            drag_dy.set(info.translation.y);
                        }
                        GesturePhase::Ended => {
                            let slots = (info.translation.y / row_height).round() as i64;
                            let from = i as i64;
                            let to = (from + slots).clamp(0, count as i64 - 1);
                            dragging.set(None);
                            drag_dy.set(0.0);
                            if to != from {
                                on_reorder(from as usize, to as usize);
                            }
                        }
                    }),
            )
        })
        .collect();
    column(rows)
}

// ---------------------------------------------------------------------------
// Error boundary
// ---------------------------------------------------------------------------

/// Wraps `content` and shows `fallback(message)` instead whenever `error`
/// holds `Some(message)`.
///
/// A controllable error boundary: set the signal from a failed
/// [`Resource`](crate::async_rt::Resource), a `catch_unwind` site, or any error
/// path to swap the subtree for a recovery UI. Clearing it back to `None`
/// restores `content`.
///
/// # Example
/// ```rust
/// use super::view::{error_boundary, boxed, text, button};
/// use crate::reactive::create_signal;
///
/// let err = create_signal::<Option<String>>(None);
/// let view = error_boundary(
///     err,
///     || boxed(text("All good")),
///     move |msg| boxed(text(format!("Something went wrong: {msg}"))),
/// );
/// ```
pub fn error_boundary(
    error: Signal<Option<String>>,
    content: impl Fn() -> BoxedView + 'static,
    fallback: impl Fn(String) -> BoxedView + 'static,
) -> impl View {
    dynamic(move || match error.get() {
        Some(msg) => fallback(msg),
        None => content(),
    })
}

// ---------------------------------------------------------------------------
// PDF viewer
// ---------------------------------------------------------------------------

/// Displays the PDF (or any document the platform web view can render) at `url`
/// inside an embedded web view. On iOS this is a `WKWebView`, which renders
/// PDFs natively with pinch-zoom and scrolling.
///
/// # Example
/// ```rust
/// use super::view::pdf_view;
///
/// let view = pdf_view("https://example.com/invoice.pdf");
/// ```
pub fn pdf_view(url: impl Into<String>) -> impl View {
    super::web_view::web_view(url)
}

// ---------------------------------------------------------------------------
// Calendar (month grid)
// ---------------------------------------------------------------------------

/// Number of days in `month` (1–12) of `year`, accounting for leap years.
fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

/// Day of week for a date, via Sakamoto's algorithm. `0` = Sunday … `6` = Saturday.
fn weekday(year: i32, month: u32, day: u32) -> u32 {
    let t = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let y = if month < 3 { year - 1 } else { year };
    let m = month as usize;
    ((((y + y / 4 - y / 100 + y / 400 + t[m - 1] + day as i32) % 7) + 7) % 7) as u32
}

/// A month-grid calendar for `year`/`month` (month is 1–12). The currently
/// selected day is read reactively from `selected`; tapping a day calls
/// `on_select(day)`. The selected day is highlighted with a filled circle.
///
/// This is composed entirely from views — no native calendar widget — so it
/// renders identically on every backend.
///
/// # Example
/// ```rust
/// use super::view::calendar;
/// use crate::reactive::create_signal;
///
/// let day = create_signal::<Option<u32>>(Some(15));
/// let view = calendar(2026, 6, day, move |d| day.set(Some(d)));
/// ```
pub fn calendar(
    year: i32,
    month: u32,
    selected: Signal<Option<u32>>,
    on_select: impl Fn(u32) + Clone + 'static,
) -> impl View {
    const WD: [&str; 7] = ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"];

    let header = row(WD
        .iter()
        .map(|d| {
            boxed(
                text(*d)
                    .font_size(12.0)
                    .color(Color::hex(0x8E8E93ff))
                    .align(crate::dom::TextAlign::Center)
                    .grow(1.0),
            )
        })
        .collect::<Vec<_>>())
    .gap(2.0);

    let first_wd = weekday(year, month, 1) as usize;
    let dim = days_in_month(year, month) as usize;

    let mut cells: Vec<Option<u32>> = Vec::new();
    for _ in 0..first_wd {
        cells.push(None);
    }
    for d in 1..=dim {
        cells.push(Some(d as u32));
    }
    while cells.len() % 7 != 0 {
        cells.push(None);
    }

    let mut weeks: Vec<BoxedView> = vec![boxed(header)];
    for chunk in cells.chunks(7) {
        let row_cells: Vec<BoxedView> = chunk
            .iter()
            .map(|c| match c {
                Some(d) => {
                    let d = *d;
                    let on_select = on_select.clone();
                    boxed(
                        column((boxed(
                            text(format!("{d}"))
                                .font_size(15.0)
                                .align(crate::dom::TextAlign::Center)
                                .text_color_fn(move || {
                                    if selected.get() == Some(d) {
                                        Color::WHITE
                                    } else {
                                        Color::hex(0x1C1C1Eff)
                                    }
                                }),
                        ),))
                        .align(AlignItems::Center)
                        .justify(JustifyContent::Center)
                        .padding(6.0)
                        .corner_radius(18.0)
                        .background_fn(move || {
                            if selected.get() == Some(d) {
                                DEFAULT_TINT
                            } else {
                                Color::rgba(0, 0, 0, 0)
                            }
                        })
                        .grow(1.0)
                        .on_tap(move || on_select(d)),
                    )
                }
                None => boxed(column(()).grow_by(1.0)),
            })
            .collect();
        weeks.push(boxed(row(row_cells).gap(2.0)));
    }
    column(weeks).gap(6.0)
}

#[cfg(test)]
mod date_math_tests {
    use super::{days_in_month, weekday};

    #[test]
    fn days_in_month_handles_leap_years() {
        assert_eq!(days_in_month(2026, 1), 31);
        assert_eq!(days_in_month(2026, 2), 28); // not a leap year
        assert_eq!(days_in_month(2024, 2), 29); // leap year
        assert_eq!(days_in_month(2000, 2), 29); // divisible by 400
        assert_eq!(days_in_month(1900, 2), 28); // divisible by 100 not 400
        assert_eq!(days_in_month(2026, 4), 30);
    }

    #[test]
    fn weekday_matches_known_dates() {
        // 2026-06-01 is a Monday (1).
        assert_eq!(weekday(2026, 6, 1), 1);
        // 2000-01-01 was a Saturday (6).
        assert_eq!(weekday(2000, 1, 1), 6);
        // 2024-02-29 was a Thursday (4).
        assert_eq!(weekday(2024, 2, 29), 4);
    }
}
