//! The widget/mutation model: the data crossing the render seam.
//!
//! The engine never touches a platform view directly. Instead it produces a
//! stream of [`Mutation`]s — a flat, `Clone`-able, comparable command list — and
//! a [`Backend`](crate::Backend) applies them to real `UIView`s /
//! `android.view.View`s. Keeping this an inspectable value type is what makes
//! the whole framework testable with zero platform code (assert on the stream)
//! and is the seam that later allows diffing off the main thread.

use rax_core::{Color, Index, Rect};

/// A stable handle to a node in the retained element tree (and, 1:1, to a native
/// view created by the backend).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WidgetId(pub(crate) Index);

impl WidgetId {
    /// The raw slot, for debugging / inspector tooling.
    pub fn raw(self) -> u32 {
        self.0.slot()
    }
}

/// The kind of native view to materialize. Intentionally tiny for now; new kinds
/// are added here and matched in each backend (open/closed: the engine is closed
/// to modification, backends extend by handling new variants).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetKind {
    /// A layout container (maps to a plain `UIView` / `ViewGroup`).
    View,
    /// A text label (maps to `UILabel` / `TextView`).
    Text,
    /// A tappable button (maps to `UIButton` / `Button`).
    Button,
}

/// A single settable **paint** property on a widget.
///
/// These are forwarded to the backend and not retained. Layout *inputs*
/// (direction, padding, gap, size) are not here — they live in
/// [`LayoutStyle`](rax_core::LayoutStyle), retained on the node, and produce
/// [`Mutation::SetFrame`] via the layout pass.
#[derive(Debug, Clone, PartialEq)]
pub enum Attribute {
    /// Text content (valid on [`WidgetKind::Text`]/[`WidgetKind::Button`]).
    Text(String),
    /// Font size in logical pixels.
    FontSize(f32),
    /// Foreground / text color.
    TextColor(Color),
    /// Background fill.
    BackgroundColor(Color),
}

/// One atomic change to the native view tree.
#[derive(Debug, Clone, PartialEq)]
pub enum Mutation {
    /// Allocate a native view of `kind` for `id`.
    Create {
        /// The new widget's id.
        id: WidgetId,
        /// What to create.
        kind: WidgetKind,
    },
    /// Set or update a paint property on an existing widget.
    SetAttribute {
        /// Target widget.
        id: WidgetId,
        /// Property to apply.
        attr: Attribute,
    },
    /// Position and size a widget's native view (output of the layout pass), in
    /// the coordinate space of its parent.
    SetFrame {
        /// Target widget.
        id: WidgetId,
        /// New frame.
        rect: Rect,
    },
    /// Insert `child` into `parent`'s child list at `index`.
    InsertChild {
        /// Container.
        parent: WidgetId,
        /// Position among siblings.
        index: usize,
        /// Child to insert.
        child: WidgetId,
    },
    /// Detach `child` from `parent` (the child may still be re-inserted).
    RemoveChild {
        /// Container.
        parent: WidgetId,
        /// Child to detach.
        child: WidgetId,
    },
    /// Free the native view backing `id`.
    Destroy {
        /// Widget to destroy.
        id: WidgetId,
    },
}
