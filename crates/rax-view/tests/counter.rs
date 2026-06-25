//! The end-to-end proof that the declarative builder lowers correctly and that
//! fine-grained reactivity flows through it: build a counter, tap it, observe
//! exactly one targeted mutation.

use rax_dom::{Attribute, Event, Host, Mutation, RecordingBackend, Tree, WidgetId, WidgetKind};
use rax_reactive::{create_signal, Signal};
use rax_view::*;

fn counter(count: Signal<i32>) -> impl View {
    column((
        text(move || format!("Count: {}", count.get())).font_size(24.0),
        button("+1", move || count.update(|c| *c += 1)),
    ))
    .padding(16.0)
    .gap(8.0)
}

fn find_text(log: &[Mutation], wanted: &str) -> bool {
    log.iter().any(
        |m| matches!(m, Mutation::SetAttribute { attr: Attribute::Text(s), .. } if s == wanted),
    )
}

fn first_button(log: &[Mutation]) -> WidgetId {
    log.iter()
        .find_map(|m| match m {
            Mutation::Create {
                id,
                kind: WidgetKind::Button,
            } => Some(*id),
            _ => None,
        })
        .expect("counter should create a button")
}

#[test]
fn counter_builds_initial_tree() {
    let backend = RecordingBackend::new();
    let log = backend.log();
    let mut tree = Tree::new(Host::new(backend));
    let count = create_signal(0);

    let root = mount(&mut tree, counter(count));

    // Layout style is retained on the node (not a paint mutation).
    let style = tree.style_of(root).unwrap();
    assert_eq!(style.direction, rax_core::FlexDirection::Column);
    assert_eq!(style.gap, 8.0);
    assert_eq!(style.padding, rax_core::EdgeInsets::all(16.0));

    let muts = log.borrow();
    assert_eq!(
        muts[0],
        Mutation::Create {
            id: root,
            kind: WidgetKind::View
        }
    );
    // It contains a text label and a button.
    assert!(find_text(&muts, "Count: 0"));
    assert_eq!(tree.children_of(root).len(), 2);
}

#[test]
fn signal_change_emits_one_targeted_mutation() {
    let backend = RecordingBackend::new();
    let log = backend.log();
    let mut tree = Tree::new(Host::new(backend));
    let count = create_signal(0);
    mount(&mut tree, counter(count));

    log.borrow_mut().clear();
    count.set(5);

    let muts = log.borrow();
    assert_eq!(muts.len(), 1, "fine-grained: one mutation, no tree diff");
    assert!(
        matches!(&muts[0], Mutation::SetAttribute { attr: Attribute::Text(s), .. } if s == "Count: 5")
    );
}

#[test]
fn tapping_the_button_updates_the_label() {
    let backend = RecordingBackend::new();
    let log = backend.log();
    let mut tree = Tree::new(Host::new(backend));
    let count = create_signal(0);
    mount(&mut tree, counter(count));

    let button_id = first_button(&log.borrow());
    log.borrow_mut().clear();

    // Simulate the platform delivering a tap on the button.
    tree.dispatch(&Event::Tap { target: button_id });

    assert!(
        find_text(&log.borrow(), "Count: 1"),
        "tap handler incremented and re-rendered the label"
    );
}

#[test]
fn empty_container_builds_with_no_children() {
    let backend = RecordingBackend::new();
    let log = backend.log();
    let mut tree = Tree::new(Host::new(backend));

    let root = mount(&mut tree, column(()));
    assert_eq!(tree.children_of(root).len(), 0);
    assert_eq!(
        log.borrow()[0],
        Mutation::Create {
            id: root,
            kind: WidgetKind::View
        }
    );
}
