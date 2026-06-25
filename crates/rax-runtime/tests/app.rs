//! Full runtime pipeline, host-side: build → layout → tap → reactive update,
//! all observed through the recording backend (no platform needed).

use rax_core::Size;
use rax_dom::{Attribute, Event, Host, Mutation, RecordingBackend, WidgetId, WidgetKind};
use rax_reactive::{create_signal, Signal};
use rax_runtime::App;
use rax_view::{button, column, text, View};

fn counter(count: Signal<i32>) -> impl View {
    column((
        text(move || format!("Count: {}", count.get())),
        button("+1", move || count.update(|c| *c += 1)),
    ))
    .padding(16.0)
    .gap(8.0)
}

fn find_button(log: &[Mutation]) -> WidgetId {
    log.iter()
        .find_map(|m| match m {
            Mutation::Create {
                id,
                kind: WidgetKind::Button,
            } => Some(*id),
            _ => None,
        })
        .expect("counter has a button")
}

#[test]
fn app_builds_lays_out_and_reacts_to_taps() {
    let backend = RecordingBackend::new();
    let log = backend.log();
    let count = create_signal(0);

    let mut app = App::new(Host::new(backend), Size::new(320.0, 640.0), move || {
        counter(count)
    });

    // Initial build emitted Create + paint, and the initial layout emitted frames.
    {
        let muts = log.borrow();
        assert!(muts.iter().any(|m| matches!(
            m,
            Mutation::Create {
                kind: WidgetKind::View,
                ..
            }
        )));
        assert!(
            muts.iter().any(|m| matches!(m, Mutation::SetFrame { .. })),
            "initial layout emits frames"
        );
        // The root fills the viewport.
        assert!(muts.iter().any(|m| matches!(
            m,
            Mutation::SetFrame { id, rect } if *id == app.root() && rect.size == Size::new(320.0, 640.0)
        )));
    }

    let button_id = find_button(&log.borrow());
    log.borrow_mut().clear();

    // The platform delivers a tap; the next frame processes it.
    app.event_sink().dispatch(Event::Tap { target: button_id });
    app.tick();

    let muts = log.borrow();
    assert!(
        muts.iter().any(|m| matches!(m, Mutation::SetAttribute { attr: Attribute::Text(s), .. } if s == "Count: 1")),
        "tap incremented the counter and re-rendered the label"
    );
}

#[test]
fn relayout_emits_no_redundant_frames_when_nothing_changes() {
    let backend = RecordingBackend::new();
    let log = backend.log();
    let count = create_signal(0);
    let mut app = App::new(Host::new(backend), Size::new(320.0, 640.0), move || {
        counter(count)
    });

    log.borrow_mut().clear();
    app.tick(); // no events, no size change

    let frame_mutations = log
        .borrow()
        .iter()
        .filter(|m| matches!(m, Mutation::SetFrame { .. }))
        .count();
    assert_eq!(frame_mutations, 0, "stable layout emits no frames");
}

#[test]
fn safe_area_insets_offset_and_shrink_the_root() {
    use rax_core::{EdgeInsets, Rect};

    let backend = RecordingBackend::new();
    let log = backend.log();
    let count = create_signal(0);
    let mut app = App::new(Host::new(backend), Size::new(320.0, 640.0), move || {
        counter(count)
    });

    log.borrow_mut().clear();
    // A notch on top (47) and a home indicator at the bottom (34).
    app.set_safe_area(EdgeInsets {
        top: 47.0,
        right: 0.0,
        bottom: 34.0,
        left: 0.0,
    });

    let muts = log.borrow();
    // Root is offset by the top inset and shrunk by top+bottom.
    assert!(
        muts.iter().any(|m| matches!(
            m,
            Mutation::SetFrame { id, rect }
                if *id == app.root()
                    && *rect == Rect::new(0.0, 47.0, 320.0, 640.0 - 47.0 - 34.0)
        )),
        "root sits inside the safe area"
    );
}

#[test]
fn keyboard_inset_shrinks_content_from_the_bottom() {
    use rax_core::{EdgeInsets, Rect};

    let backend = RecordingBackend::new();
    let log = backend.log();
    let count = create_signal(0);
    let mut app = App::new(Host::new(backend), Size::new(320.0, 640.0), move || {
        counter(count)
    });
    app.set_safe_area(EdgeInsets {
        top: 47.0,
        right: 0.0,
        bottom: 34.0,
        left: 0.0,
    });

    log.borrow_mut().clear();
    app.set_keyboard_inset(300.0); // keyboard taller than the home indicator

    let muts = log.borrow();
    assert!(
        muts.iter().any(|m| matches!(
            m,
            Mutation::SetFrame { id, rect }
                if *id == app.root()
                    && *rect == Rect::new(0.0, 47.0, 320.0, 640.0 - 47.0 - 300.0)
        )),
        "keyboard inset (max with safe-area bottom) shrinks the root"
    );
}

#[test]
fn backdrop_resolves_with_color_scheme() {
    use rax_core::{Color, ColorScheme};
    use rax_runtime::{set_backdrop, Backdrop};

    let backend = RecordingBackend::new();
    let log = backend.log();
    let light = Color::rgb(250, 250, 250);
    let dark = Color::rgb(10, 10, 10);

    let mut app = App::new(Host::new(backend), Size::new(320.0, 640.0), move || {
        set_backdrop(Backdrop::System { light, dark });
        text("hi")
    });

    // Built in light mode → light backdrop emitted.
    assert!(
        log.borrow()
            .iter()
            .any(|m| matches!(m, Mutation::SetBackdrop { color } if *color == light)),
        "initial backdrop resolves to light"
    );

    log.borrow_mut().clear();
    app.set_color_scheme(ColorScheme::Dark);
    assert!(
        log.borrow()
            .iter()
            .any(|m| matches!(m, Mutation::SetBackdrop { color } if *color == dark)),
        "switching to dark re-resolves the backdrop"
    );
}
