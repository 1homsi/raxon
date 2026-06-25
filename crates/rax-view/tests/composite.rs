//! `checkbox` / `radio` — composite components built from the public API.

use rax_dom::{Attribute, Event, Host, Mutation, RecordingBackend, Tree};
use rax_reactive::create_signal;
use rax_view::{checkbox, radio, View};

fn harness() -> (Tree, std::rc::Rc<std::cell::RefCell<Vec<Mutation>>>) {
    let backend = RecordingBackend::new();
    let log = backend.log();
    (Tree::new(Host::new(backend)), log)
}

fn has_image(log: &[Mutation], wanted: &str) -> bool {
    log.iter().any(
        |m| matches!(m, Mutation::SetAttribute { attr: Attribute::ImageSource(s), .. } if s == wanted),
    )
}

#[test]
fn checkbox_reflects_state_and_toggles_on_tap() {
    let (mut tree, log) = harness();
    let agreed = create_signal(false);
    let id = checkbox(move || agreed.get(), move |v| agreed.set(v))
        .label("I agree")
        .build(&mut tree);
    tree.run_dynamic();

    assert!(has_image(&log.borrow(), "square"), "starts unchecked");
    assert!(!has_image(&log.borrow(), "checkmark.square.fill"));

    log.borrow_mut().clear();
    tree.dispatch(&Event::Tap { target: id });
    tree.run_dynamic();

    assert!(agreed.get(), "tap toggled the bound signal");
    assert!(
        has_image(&log.borrow(), "checkmark.square.fill"),
        "glyph updated to checked"
    );
}

#[test]
fn radio_group_single_selection() {
    let (mut tree, log) = harness();
    let choice = create_signal(0u32);

    let _first = radio(move || choice.get() == 0, move || choice.set(0))
        .label("One")
        .build(&mut tree);
    let second = radio(move || choice.get() == 1, move || choice.set(1))
        .label("Two")
        .build(&mut tree);
    tree.run_dynamic();

    // Option 0 selected initially.
    assert!(has_image(&log.borrow(), "largecircle.fill.circle"));

    log.borrow_mut().clear();
    tree.dispatch(&Event::Tap { target: second });
    tree.run_dynamic();

    assert_eq!(choice.get(), 1, "selecting option two updates the signal");
    // The previously-selected option falls back to the empty glyph; the newly
    // selected one fills — both glyph kinds appear in the rebuild.
    assert!(has_image(&log.borrow(), "largecircle.fill.circle"));
    assert!(has_image(&log.borrow(), "circle"));
}

#[test]
fn card_groups_children_and_badge_shows_label() {
    use rax_view::{badge, card, text};
    let (mut tree, log) = harness();
    card((text("Title"), text("Body"))).build(&mut tree);
    badge("9+").build(&mut tree);

    let has_text = |s: &str| {
        log.borrow().iter().any(
            |m| matches!(m, Mutation::SetAttribute { attr: Attribute::Text(t), .. } if t == s),
        )
    };
    assert!(has_text("Title") && has_text("Body"), "card rendered children");
    assert!(has_text("9+"), "badge rendered its label");
}

#[test]
fn avatar_is_circular_and_chip_toggles() {
    use rax_view::{avatar, chip, View};
    use std::cell::Cell;
    use std::rc::Rc;

    let (mut tree, log) = harness();
    avatar("person.crop.circle.fill").size(48.0).build(&mut tree);

    // Circular = corner radius is half the size.
    assert!(
        log.borrow().iter().any(|m| matches!(
            m,
            Mutation::SetAttribute { attr: Attribute::CornerRadius(r), .. } if (*r - 24.0).abs() < 1e-3
        )),
        "avatar rounds to a circle"
    );

    let tapped = Rc::new(Cell::new(0));
    let t2 = tapped.clone();
    let id = chip("Spicy", true, move || t2.set(t2.get() + 1)).build(&mut tree);
    tree.dispatch(&Event::Tap { target: id });
    assert_eq!(tapped.get(), 1, "chip reports taps");
}
