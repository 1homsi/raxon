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

use rax_core::{AlignItems, Color, EdgeInsets, JustifyContent};
use rax_dom::{GesturePhase, Tree, WidgetId};
use rax_reactive::{create_effect, create_signal, Signal};

use crate::container::{column, row};
use crate::dynamic::dynamic;
use crate::image::{icon, image};
use crate::modifier::{PanInfo, ViewExt};
use crate::text::text;
use crate::text_input::text_input;
use crate::view::{boxed, BoxedView, View, ViewSequence};

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
/// use rax_view::checkbox;
/// use rax_reactive::create_signal;
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
/// use rax_view::radio;
/// use rax_reactive::create_signal;
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
/// use rax_view::{card, text};
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
/// use rax_view::badge;
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
/// use rax_view::avatar;
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
/// use rax_view::chip;
/// use rax_reactive::create_signal;
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
    use crate::button::button;
    use rax_dom::TextAlign;

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
/// use rax_view::modal;
/// use rax_reactive::create_signal;
///
/// let open = create_signal(false);
/// let v = modal(open, move || open.set(false), || rax_view::text("Hello"));
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
/// use rax_view::{fade_transition, text};
/// use rax_reactive::create_signal;
///
/// let visible = create_signal(true);
/// let v = fade_transition(visible, || text("Hello, world!"));
/// ```
pub fn fade_transition<V: View + 'static>(
    show: Signal<bool>,
    content: impl Fn() -> V + 'static,
) -> impl View {
    use rax_anim::{animate, Easing};

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
/// use rax_view::bottom_sheet;
/// use rax_reactive::create_signal;
///
/// let open = create_signal(false);
/// let v = bottom_sheet(open, move || open.set(false), || rax_view::text("Sheet body"));
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
/// use rax_view::toast;
/// use rax_reactive::create_signal;
///
/// let msg: rax_reactive::Signal<Option<String>> = create_signal(None);
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
/// use rax_view::item_separator;
/// use rax_core::Color;
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
/// use rax_view::picker;
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
/// use rax_view::{grid, text, boxed};
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
/// use rax_view::network_image;
///
/// let img = network_image("https://example.com/photo.jpg", "photo");
/// ```
pub fn network_image(url: impl Into<String>, placeholder: impl Into<String>) -> impl View {
    let url = url.into();
    let placeholder = placeholder.into();
    let bytes = create_signal::<Option<std::sync::Arc<Vec<u8>>>>(None);

    // Kick off the fetch.
    let res = rax_net::get(url);
    create_effect(move || {
        use rax_async::ResourceState;
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
/// use rax_view::{app_bar, text};
/// use rax_core::Color;
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
    use crate::button::button;

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
            crate::text::text(title)
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
/// use rax_view::action_sheet;
/// use rax_reactive::create_signal;
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
    use crate::button::button;
    use rax_dom::TextAlign;

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
    use crate::container::stack;
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
/// [`rax_runtime::last_panic`] to surface panics as a visible overlay in
/// debug builds rather than silently freezing the app.
///
/// Place at the **top** of your root view tree (e.g. in a `stack`) so it
/// always renders above your other content:
///
/// ```no_run
/// use rax_view::{error_overlay, stack};
/// use rax_reactive::create_signal;
///
/// // let msg = create_signal(rax_runtime::last_panic());
/// // let view = stack((your_app_view, error_overlay(msg)));
/// ```
pub fn error_overlay(message: rax_reactive::Signal<Option<String>>) -> impl View {
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
/// use rax_view::swipe_actions;
/// use rax_core::Color;
/// use std::sync::Arc;
///
/// let view = swipe_actions(
///     || rax_view::text("My item"),
///     vec![
///         ("Delete".to_string(), Color::rgb(255, 51, 51), Arc::new(|| println!("deleted")) as Arc<dyn Fn() + Send + Sync>),
///     ],
/// );
/// ```
pub fn swipe_actions(
    content: impl Fn() -> BoxedView + 'static,
    actions: Vec<(String, Color, std::sync::Arc<dyn Fn() + Send + Sync>)>,
) -> impl View {
    use rax_anim::{animate, Easing};
    use crate::modifier::ViewExt;
    use rax_dom::Transform;

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
                    crate::button::button(label, move || {
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
/// use rax_view::{dev_tools, stack, text};
///
/// let v = stack((text("App content"), dev_tools()));
/// ```
pub fn dev_tools() -> BoxedView {
    if cfg!(debug_assertions) {
        boxed(
            row((
                boxed(
                    text("rax [debug]")
                        .font_size(10.0)
                        .color(Color::rgb(255, 255, 255)),
                ),
            ))
            .padding(4.0)
            .background(Color::rgba(0, 0, 0, 153))
            .corner_radius(4.0),
        )
    } else {
        boxed(column(()).size(0.0, 0.0))
    }
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
            let spring_x = rax_anim::spring(sx.get(), 0.0, rax_anim::Spring::default());
            create_effect(move || sx.set(spring_x.get()));
            let sy = y;
            let spring_y = rax_anim::spring(sy.get(), 0.0, rax_anim::Spring::default());
            create_effect(move || sy.set(spring_y.get()));
        }
    };
    (x, y, handler)
}
