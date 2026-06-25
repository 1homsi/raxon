//! The widget/mutation model: the data crossing the render seam.
//!
//! The engine never touches a platform view directly. Instead it produces a
//! stream of [`Mutation`]s — a flat, `Clone`-able, comparable command list — and
//! a [`Backend`](crate::Backend) applies them to real `UIView`s /
//! `android.view.View`s. Keeping this an inspectable value type is what makes
//! the whole framework testable with zero platform code (assert on the stream)
//! and is the seam that later allows diffing off the main thread.

use rax_core::{Color, Index, Rect, Size};

/// A stable handle to a node in the retained element tree (and, 1:1, to a native
/// view created by the backend).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WidgetId(pub(crate) Index);

impl WidgetId {
    /// The raw slot, for debugging / inspector tooling.
    pub fn raw(self) -> u32 {
        self.0.slot()
    }

    /// Packs this id into a `u64` (slot in the high 32 bits, generation in the
    /// low 32). Used to stash a widget id in an opaque native field such as a
    /// `UIView.tag`, and recover it on a callback.
    pub fn to_u64(self) -> u64 {
        ((self.0.slot() as u64) << 32) | (self.0.generation() as u64)
    }

    /// Inverse of [`to_u64`](WidgetId::to_u64).
    pub fn from_u64(bits: u64) -> WidgetId {
        WidgetId(Index::from_raw((bits >> 32) as u32, bits as u32))
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
    /// An image view (maps to `UIImageView` / `ImageView`).
    Image,
    /// An on/off switch (maps to `UISwitch` / `Switch`).
    Switch,
    /// A value slider (maps to `UISlider` / `SeekBar`).
    Slider,
    /// A single-line editable text field (maps to `UITextField` / `EditText`).
    TextInput,
    /// A scroll container (maps to `UIScrollView` / `ScrollView`).
    Scroll,
}

/// Accessibility role, mapped to platform a11y traits (VoiceOver/TalkBack).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Role {
    /// No special role.
    #[default]
    None,
    /// Activatable button.
    Button,
    /// Section header.
    Header,
    /// Image/graphic.
    Image,
    /// Link.
    Link,
    /// Adjustable (slider-like).
    Adjustable,
    /// Search field.
    Search,
}

/// A gesture a widget should recognize (the backend attaches a recognizer).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GestureKind {
    /// Single tap.
    Tap,
    /// Double tap.
    DoubleTap,
    /// Long press.
    LongPress,
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
    /// Corner radius in logical points (rounds the view's layer).
    CornerRadius(f32),
    /// Opacity, `0.0` transparent .. `1.0` opaque.
    Opacity(f32),
    /// Border width in logical points.
    BorderWidth(f32),
    /// Border color.
    BorderColor(Color),
    /// Drop shadow.
    Shadow(Shadow),
    /// Image source: an asset name or system-symbol name.
    ImageSource(String),
    /// Boolean value (e.g. a switch's on/off state).
    BoolValue(bool),
    /// Floating-point value (e.g. a slider's position, `0.0..=1.0`).
    FloatValue(f32),
    /// Tint color (e.g. image tint, control accent).
    TintColor(Color),
    /// Placeholder text (e.g. for a text field).
    Placeholder(String),
    /// Accessibility label read by screen readers.
    AccessibilityLabel(String),
    /// Accessibility role / traits.
    AccessibilityRole(Role),
}

/// A drop shadow specification.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shadow {
    /// Shadow color (alpha is used as shadow opacity).
    pub color: Color,
    /// Blur radius in points.
    pub radius: f32,
    /// Horizontal offset in points.
    pub dx: f32,
    /// Vertical offset in points.
    pub dy: f32,
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
    /// Designate `id` as the root view, to be attached to the platform's content
    /// view (window / view-controller view).
    SetRoot {
        /// The root widget.
        id: WidgetId,
    },
    /// Attach a gesture recognizer to `id` so it emits the corresponding event.
    AddGesture {
        /// Target widget.
        id: WidgetId,
        /// Which gesture to recognize.
        gesture: GestureKind,
    },
    /// Set a scroll container's scrollable content size.
    SetContentSize {
        /// Scroll widget.
        id: WidgetId,
        /// Total content size.
        size: Size,
    },
}
