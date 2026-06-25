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

    let text_h = (16.0_f32 * 1.4).ceil(); // default font size -> measured height

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
