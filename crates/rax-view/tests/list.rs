//! `each` (reactive collection) and `show` (conditional) helpers.

use rax_dom::{Attribute, Host, Mutation, RecordingBackend, Tree};
use rax_reactive::create_signal;
use rax_view::{boxed, each, show, text, View};

fn harness() -> (Tree, std::rc::Rc<std::cell::RefCell<Vec<Mutation>>>) {
    let backend = RecordingBackend::new();
    let log = backend.log();
    (Tree::new(Host::new(backend)), log)
}

fn has_text(log: &[Mutation], wanted: &str) -> bool {
    log.iter().any(
        |m| matches!(m, Mutation::SetAttribute { attr: Attribute::Text(s), .. } if s == wanted),
    )
}

#[test]
fn each_renders_items_and_updates_on_change() {
    let (mut tree, log) = harness();
    let items = create_signal(vec!["a".to_string(), "b".to_string()]);
    each(move || items.get(), |s| boxed(text(s))).build(&mut tree);
    tree.run_dynamic();

    assert!(has_text(&log.borrow(), "a"));
    assert!(has_text(&log.borrow(), "b"));
    assert!(!has_text(&log.borrow(), "c"));

    log.borrow_mut().clear();
    items.update(|v| v.push("c".to_string()));
    tree.run_dynamic();
    assert!(
        has_text(&log.borrow(), "c"),
        "new item rendered after update"
    );
}

#[test]
fn show_renders_only_when_condition_holds() {
    let (mut tree, log) = harness();
    let visible = create_signal(true);
    show(move || visible.get(), || text("shown")).build(&mut tree);
    tree.run_dynamic();
    assert!(has_text(&log.borrow(), "shown"));

    log.borrow_mut().clear();
    visible.set(false);
    tree.run_dynamic();
    // The branch is torn down; the rebuilt subtree shows nothing.
    assert!(
        !has_text(&log.borrow(), "shown"),
        "hidden when condition is false"
    );
}
