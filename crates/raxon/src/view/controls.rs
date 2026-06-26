//! Value controls: `switch`, `slider`, `segmented`, `stepper`, and date pickers.

use std::cell::RefCell;
use std::rc::Rc;

use crate::dom::{Attribute, DatePickerMode, DatePickerStyle, Event, EventKind, Tree, WidgetId};
use crate::reactive::create_signal;

use super::container::row;
use super::view::{boxed, View};

/// An on/off switch. Build via [`switch`].
pub struct Switch<F> {
    checked: bool,
    on_change: F,
}

/// Creates a switch with initial state `checked` that calls `on_change` when
/// toggled.
pub fn switch<F: FnMut(bool) + 'static>(checked: bool, on_change: F) -> Switch<F> {
    Switch { checked, on_change }
}

impl<F: FnMut(bool) + 'static> View for Switch<F> {
    fn build(self, tree: &mut Tree) -> WidgetId {
        let id = tree.create_switch();
        tree.set(id, Attribute::BoolValue(self.checked));
        let mut on_change = self.on_change;
        tree.on(id, EventKind::ValueChanged, move |event| {
            if let Event::ValueChanged { value, .. } = event {
                on_change(*value != 0.0);
            }
        });
        id
    }
}

/// A value slider (`0.0..=1.0`). Build via [`slider`].
pub struct Slider<F> {
    value: f32,
    on_change: F,
}

/// Creates a slider at `value` (`0.0..=1.0`) that calls `on_change` as it moves.
pub fn slider<F: FnMut(f32) + 'static>(value: f32, on_change: F) -> Slider<F> {
    Slider { value, on_change }
}

impl<F: FnMut(f32) + 'static> View for Slider<F> {
    fn build(self, tree: &mut Tree) -> WidgetId {
        let id = tree.create_slider();
        tree.set(id, Attribute::FloatValue(self.value));
        let mut on_change = self.on_change;
        tree.on(id, EventKind::ValueChanged, move |event| {
            if let Event::ValueChanged { value, .. } = event {
                on_change(*value as f32);
            }
        });
        id
    }
}

/// A horizontal segmented control (pick one of N labelled options). Build via
/// [`segmented`].
pub struct Segmented<F> {
    items: Vec<String>,
    selected: usize,
    on_change: F,
}

/// Creates a segmented control over `items`, with `selected` initially active,
/// calling `on_change` with the newly selected index when the user picks a
/// segment.
pub fn segmented<F>(
    items: impl IntoIterator<Item = impl Into<String>>,
    selected: usize,
    on_change: F,
) -> Segmented<F>
where
    F: FnMut(usize) + 'static,
{
    Segmented {
        items: items.into_iter().map(Into::into).collect(),
        selected,
        on_change,
    }
}

impl<F: FnMut(usize) + 'static> View for Segmented<F> {
    fn build(self, tree: &mut Tree) -> WidgetId {
        let id = tree.create_segmented();
        tree.set(id, Attribute::Items(self.items));
        tree.set(id, Attribute::FloatValue(self.selected as f32));
        let mut on_change = self.on_change;
        tree.on(id, EventKind::ValueChanged, move |event| {
            if let Event::ValueChanged { value, .. } = event {
                on_change(value.max(0.0) as usize);
            }
        });
        id
    }
}

/// A -/+ stepper over a bounded numeric range. Build via [`stepper`].
pub struct Stepper<F> {
    value: f32,
    min: f32,
    max: f32,
    step: f32,
    on_change: F,
}

/// Creates a stepper at `value`, reporting the new value via `on_change` when
/// the user taps -/+. Defaults to a `0..=100` range with a step of `1`; tune
/// with [`Stepper::range`] and [`Stepper::step`].
pub fn stepper<F: FnMut(f32) + 'static>(value: f32, on_change: F) -> Stepper<F> {
    Stepper {
        value,
        min: 0.0,
        max: 100.0,
        step: 1.0,
        on_change,
    }
}

impl<F> Stepper<F> {
    /// Sets the inclusive `min..=max` bounds.
    #[must_use]
    pub fn range(mut self, min: f32, max: f32) -> Self {
        self.min = min;
        self.max = max;
        self
    }

    /// Sets the increment applied per -/+ tap.
    #[must_use]
    pub fn step(mut self, step: f32) -> Self {
        self.step = step;
        self
    }
}

impl<F: FnMut(f32) + 'static> View for Stepper<F> {
    fn build(self, tree: &mut Tree) -> WidgetId {
        let id = tree.create_stepper();
        tree.set(
            id,
            Attribute::Range {
                min: self.min,
                max: self.max,
                step: self.step,
            },
        );
        tree.set(id, Attribute::FloatValue(self.value));
        let mut on_change = self.on_change;
        tree.on(id, EventKind::ValueChanged, move |event| {
            if let Event::ValueChanged { value, .. } = event {
                on_change(*value as f32);
            }
        });
        id
    }
}

/// A native date/time picker backed by platform controls. Build via
/// [`date_picker`].
pub struct DatePicker<F> {
    value: f64,
    mode: DatePickerMode,
    style: DatePickerStyle,
    min: Option<f64>,
    max: Option<f64>,
    on_change: F,
}

/// Creates a native date picker at `value` (seconds since the Unix epoch) that
/// calls `on_change` with the updated epoch seconds.
pub fn date_picker<F: FnMut(f64) + 'static>(value: f64, on_change: F) -> DatePicker<F> {
    DatePicker {
        value,
        mode: DatePickerMode::Date,
        style: DatePickerStyle::Compact,
        min: None,
        max: None,
        on_change,
    }
}

impl<F> DatePicker<F> {
    /// Sets whether the picker edits a date, a time, or both.
    #[must_use]
    pub fn mode(mut self, mode: DatePickerMode) -> Self {
        self.mode = mode;
        self
    }

    /// Sets the native picker presentation style.
    #[must_use]
    pub fn style(mut self, style: DatePickerStyle) -> Self {
        self.style = style;
        self
    }

    /// Sets the minimum selectable date as seconds since the Unix epoch.
    #[must_use]
    pub fn min(mut self, epoch_seconds: f64) -> Self {
        self.min = Some(epoch_seconds);
        self
    }

    /// Sets the maximum selectable date as seconds since the Unix epoch.
    #[must_use]
    pub fn max(mut self, epoch_seconds: f64) -> Self {
        self.max = Some(epoch_seconds);
        self
    }
}

impl<F: FnMut(f64) + 'static> View for DatePicker<F> {
    fn build(self, tree: &mut Tree) -> WidgetId {
        let id = tree.create_date_picker();
        tree.set(id, Attribute::DatePickerMode(self.mode));
        tree.set(id, Attribute::DatePickerStyle(self.style));
        if let Some(min) = self.min {
            tree.set(id, Attribute::DateMin(min));
        }
        if let Some(max) = self.max {
            tree.set(id, Attribute::DateMax(max));
        }
        tree.set(id, Attribute::DateValue(self.value));
        let mut on_change = self.on_change;
        tree.on(id, EventKind::ValueChanged, move |event| {
            if let Event::ValueChanged { value, .. } = event {
                on_change(*value);
            }
        });
        id
    }
}

/// Composes two native date pickers into a date range editor. The callback
/// receives `(start_epoch_seconds, end_epoch_seconds)` whenever either side
/// changes.
pub fn date_range_picker<F: FnMut(f64, f64) + 'static>(
    start: f64,
    end: f64,
    on_change: F,
) -> impl View {
    let start_signal = create_signal(start);
    let end_signal = create_signal(end);
    let on_change = Rc::new(RefCell::new(on_change));

    let on_start = on_change.clone();
    let start_picker = date_picker(start, move |value| {
        start_signal.set(value);
        (on_start.borrow_mut())(value, end_signal.get());
    });

    let on_end = on_change;
    let end_picker = date_picker(end, move |value| {
        end_signal.set(value);
        (on_end.borrow_mut())(start_signal.get(), value);
    });

    row((boxed(start_picker), boxed(end_picker))).gap(8.0)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::dom::{Event, Host, RecordingBackend, Tree};
    use crate::reactive::create_root;
    use crate::view::{date_picker, View};

    #[test]
    fn date_picker_reports_epoch_seconds_on_value_change() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let seen_for_handler = seen.clone();
        let ((mut tree, id), scope) = create_root(|| {
            let mut tree = Tree::new(Host::new(RecordingBackend::new()));
            let id = date_picker(1_780_000_000.0, move |value| {
                seen_for_handler.borrow_mut().push(value);
            })
            .build(&mut tree);
            (tree, id)
        });

        tree.dispatch(&Event::ValueChanged {
            target: id,
            value: 1_780_086_400.0,
        });

        assert_eq!(&*seen.borrow(), &[1_780_086_400.0]);
        scope.dispose();
    }
}
