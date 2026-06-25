//! Value controls: `switch` and `slider`.

use rax_dom::{Attribute, Event, EventKind, Tree, WidgetId};

use crate::view::View;

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
