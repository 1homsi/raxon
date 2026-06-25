//! Switch / slider / image widgets via the recording backend.

use std::cell::Cell;
use std::rc::Rc;

use rax_dom::{Attribute, Event, Host, Mutation, RecordingBackend, Tree};
use rax_view::{image, slider, switch, View};

fn harness() -> (Tree, Rc<std::cell::RefCell<Vec<Mutation>>>) {
    let backend = RecordingBackend::new();
    let log = backend.log();
    (Tree::new(Host::new(backend)), log)
}

#[test]
fn switch_emits_initial_value_and_reports_toggles() {
    let (mut tree, log) = harness();
    let toggled = Rc::new(Cell::new(false));
    let t2 = toggled.clone();
    let id = switch(false, move |on| t2.set(on)).build(&mut tree);

    assert!(log.borrow().contains(&Mutation::SetAttribute {
        id,
        attr: Attribute::BoolValue(false)
    }));

    tree.dispatch(&Event::ValueChanged {
        target: id,
        value: 1.0,
    });
    assert!(toggled.get(), "switch reported on");
}

#[test]
fn slider_reports_value() {
    let (mut tree, log) = harness();
    let last = Rc::new(Cell::new(0.0_f32));
    let l2 = last.clone();
    let id = slider(0.25, move |v| l2.set(v)).build(&mut tree);

    assert!(log.borrow().contains(&Mutation::SetAttribute {
        id,
        attr: Attribute::FloatValue(0.25)
    }));

    tree.dispatch(&Event::ValueChanged {
        target: id,
        value: 0.8,
    });
    assert!((last.get() - 0.8).abs() < 1e-6, "slider reported new value");
}

#[test]
fn image_sets_source() {
    let (mut tree, log) = harness();
    let id = image("star.fill").build(&mut tree);
    assert!(log.borrow().contains(&Mutation::SetAttribute {
        id,
        attr: Attribute::ImageSource("star.fill".into())
    }));
}
