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

use rax_core::{AlignItems, Color, EdgeInsets};
use rax_dom::{Tree, WidgetId};

use crate::container::{column, row};
use crate::dynamic::dynamic;
use crate::image::{icon, image};
use crate::modifier::ViewExt;
use crate::text::text;
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
