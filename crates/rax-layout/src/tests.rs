use rax_core::{EdgeInsets, FlexDirection, LayoutStyle, Rect, Size};
use rax_dom::{Host, RecordingBackend, Tree};

use super::compute;

fn frame_of(frames: &[(rax_dom::WidgetId, Rect)], id: rax_dom::WidgetId) -> Rect {
    frames
        .iter()
        .find(|(i, _)| *i == id)
        .expect("frame present")
        .1
}

#[test]
fn column_stacks_children_with_padding_and_gap() {
    let mut tree = Tree::new(Host::new(RecordingBackend::new()));
    let root = tree.create_view();
    tree.set_style(
        root,
        LayoutStyle {
            direction: FlexDirection::Column,
            padding: EdgeInsets::all(10.0),
            gap: 8.0,
            ..LayoutStyle::default()
        },
    );
    let a = tree.create_text();
    let b = tree.create_text();
    tree.append(root, a);
    tree.append(root, b);

    let frames = compute(&tree, root, Size::new(300.0, 600.0));

    // Root fills the available space.
    assert_eq!(frame_of(&frames, root), Rect::new(0.0, 0.0, 300.0, 600.0));

    let text_h = (16.0_f32 * 1.35).ceil(); // default font size -> measured height

    // First child: offset by padding, stretched across the inner width.
    let fa = frame_of(&frames, a);
    assert_eq!(fa.origin.x, 10.0);
    assert_eq!(fa.origin.y, 10.0);
    assert!(
        (fa.size.width - 280.0).abs() < 0.5,
        "stretched to 300 - 2*10"
    );
    assert!((fa.size.height - text_h).abs() < 0.5);

    // Second child sits a gap below the first.
    let fb = frame_of(&frames, b);
    assert!(
        (fb.origin.y - (10.0 + text_h + 8.0)).abs() < 0.5,
        "padding + first height + gap"
    );
}

#[test]
fn justify_space_between_pushes_children_to_edges() {
    let mut tree = Tree::new(Host::new(RecordingBackend::new()));
    let root = tree.create_view();
    tree.set_style(
        root,
        LayoutStyle {
            direction: FlexDirection::Row,
            justify_content: rax_core::JustifyContent::SpaceBetween,
            ..LayoutStyle::default()
        },
    );
    let a = tree.create_button();
    let b = tree.create_button();
    tree.append(root, a);
    tree.append(root, b);

    let frames = compute(&tree, root, Size::new(300.0, 80.0));
    let fa = frame_of(&frames, a);
    let fb = frame_of(&frames, b);
    assert_eq!(fa.origin.x, 0.0, "first child flush left");
    assert!(
        fb.max_x() >= 299.0,
        "last child flush right: {}",
        fb.max_x()
    );
}

#[test]
fn max_width_constrains_size() {
    let mut tree = Tree::new(Host::new(RecordingBackend::new()));
    let root = tree.create_view();
    tree.set_style(
        root,
        LayoutStyle {
            direction: FlexDirection::Column,
            ..LayoutStyle::default()
        },
    );
    let child = tree.create_view();
    tree.set_style(
        child,
        LayoutStyle {
            width: rax_core::Dimension::Points(1000.0),
            max_width: rax_core::Dimension::Points(120.0),
            height: rax_core::Dimension::Points(10.0),
            ..LayoutStyle::default()
        },
    );
    tree.append(root, child);

    let frames = compute(&tree, root, Size::new(300.0, 300.0));
    assert!(
        (frame_of(&frames, child).size.width - 120.0).abs() < 0.5,
        "clamped to max_width"
    );
}

#[test]
fn absolute_position_uses_inset() {
    let mut tree = Tree::new(Host::new(RecordingBackend::new()));
    let root = tree.create_view();
    tree.set_style(root, LayoutStyle::default());
    let child = tree.create_view();
    tree.set_style(
        child,
        LayoutStyle {
            position: rax_core::Position::Absolute,
            inset: EdgeInsets {
                top: 25.0,
                left: 40.0,
                right: 0.0,
                bottom: 0.0,
            },
            width: rax_core::Dimension::Points(50.0),
            height: rax_core::Dimension::Points(50.0),
            ..LayoutStyle::default()
        },
    );
    tree.append(root, child);

    let frames = compute(&tree, root, Size::new(300.0, 300.0));
    assert_eq!(
        frame_of(&frames, child).origin,
        rax_core::Point::new(40.0, 25.0),
        "positioned by inset"
    );
}

#[test]
fn row_lays_children_horizontally() {
    let mut tree = Tree::new(Host::new(RecordingBackend::new()));
    let root = tree.create_view();
    tree.set_style(
        root,
        LayoutStyle {
            direction: FlexDirection::Row,
            gap: 4.0,
            ..LayoutStyle::default()
        },
    );
    let a = tree.create_button();
    let b = tree.create_button();
    tree.append(root, a);
    tree.append(root, b);

    let frames = compute(&tree, root, Size::new(200.0, 100.0));
    let fa = frame_of(&frames, a);
    let fb = frame_of(&frames, b);

    assert_eq!(fa.origin.x, 0.0);
    assert!(
        fb.origin.x >= fa.max_x(),
        "second button is to the right of the first"
    );
}
