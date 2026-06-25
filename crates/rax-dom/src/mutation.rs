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
    /// A spinning activity indicator (maps to `UIActivityIndicatorView`).
    ActivityIndicator,
    /// A determinate progress bar (maps to `UIProgressView`); uses `FloatValue`.
    Progress,
    /// A horizontal segmented control (maps to `UISegmentedControl` /
    /// `MaterialButtonToggleGroup`); segment titles come from `Items` and the
    /// selected index from `FloatValue`.
    Segmented,
    /// A -/+ stepper for a bounded numeric value (maps to `UIStepper`); the
    /// current value comes from `FloatValue` and the bounds from `Range`.
    Stepper,
    /// A multi-line editable text area (maps to `UITextView`).
    TextArea,
    /// An absolute-position container — children layer on top of each other (ZStack).
    Stack,
    /// A camera preview view that optionally decodes QR codes (maps to an
    /// `AVCaptureSession`-backed `UIView`).
    Camera,
    /// An embedded web view (WKWebView). Receives a URL or HTML string via Attribute::Url.
    WebView,
}

/// A local notification to schedule via the UserNotifications framework.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalNotification {
    /// Unique identifier for this notification (used to cancel it later).
    pub id: String,
    /// The notification title.
    pub title: String,
    /// The notification body text.
    pub body: String,
    /// Delay in seconds from now before the notification fires. 0 = 1 second (minimum).
    pub delay_seconds: u32,
}

/// iOS `UIKeyboardType` — the style of keyboard shown for a text input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardType {
    /// The default keyboard for the current input method.
    Default,
    /// A keyboard that displays standard ASCII characters.
    Ascii,
    /// A keyboard that displays numbers and punctuation.
    NumbersAndPunctuation,
    /// A keyboard optimized for URL entry.
    Url,
    /// A numeric keypad (0–9 only).
    NumberPad,
    /// A keypad for entering phone numbers.
    PhonePad,
    /// A keypad for name or phone number entry.
    NamePhonePad,
    /// A keyboard optimized for email address entry.
    Email,
    /// A numeric keypad that can also display a decimal point.
    DecimalPad,
}

/// The label on the keyboard's return key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturnKeyType {
    /// Default label.
    Default,
    /// "Done" label.
    Done,
    /// "Go" label.
    Go,
    /// "Search" label.
    Search,
    /// "Send" label.
    Send,
    /// "Next" label.
    Next,
}

/// Horizontal text alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    /// Leading edge (left in LTR).
    #[default]
    Start,
    /// Centered.
    Center,
    /// Trailing edge (right in LTR).
    End,
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

/// Layout direction for a view and all of its descendants.
///
/// Maps to `UISemanticContentAttribute` on iOS so that `UIView` subviews,
/// text, and gesture directions are automatically mirrored for RTL locales.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutDirection {
    /// Left-to-right layout (the default on most locales).
    Ltr,
    /// Right-to-left layout (Arabic, Hebrew, Persian, …).
    Rtl,
}

/// The style of haptic feedback to generate, mapping to iOS feedback generators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HapticStyle {
    /// Light impact (`UIImpactFeedbackGenerator` style light).
    Light,
    /// Medium impact (`UIImpactFeedbackGenerator` style medium).
    Medium,
    /// Heavy impact (`UIImpactFeedbackGenerator` style heavy).
    Heavy,
    /// Selection changed feedback (`UISelectionFeedbackGenerator`).
    Selection,
    /// Success notification (`UINotificationFeedbackGenerator`).
    Success,
    /// Warning notification (`UINotificationFeedbackGenerator`).
    Warning,
    /// Error notification (`UINotificationFeedbackGenerator`).
    Error,
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
    /// Pan / drag (continuous; reports translation + velocity).
    Pan,
    /// Pinch / scale (continuous; reports scale factor + velocity).
    Pinch,
    /// Rotation gesture (continuous; reports rotation in radians + velocity).
    Rotate,
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
    /// An ordered list of string items (e.g. segmented-control titles).
    Items(Vec<String>),
    /// A bounded numeric range with a step increment (e.g. for a stepper or a
    /// ranged slider): `(min, max, step)`.
    Range {
        /// Minimum value.
        min: f32,
        /// Maximum value.
        max: f32,
        /// Increment per step.
        step: f32,
    },
    /// Accessibility label read by screen readers.
    AccessibilityLabel(String),
    /// Accessibility hint — describes the result of activating this element
    /// (e.g. "Opens the settings screen"). Read by VoiceOver after the label.
    AccessibilityHint(String),
    /// Accessibility role / traits.
    AccessibilityRole(Role),
    /// Whether this element is hidden from assistive technologies. Use `true`
    /// for purely decorative elements that add no semantic value.
    AccessibilityHidden(bool),
    /// Layout direction for this view and its subtree.
    Direction(LayoutDirection),
    /// Font weight (100–900, where 400 is regular and 700 is bold).
    FontWeight(f32),
    /// Italic style.
    Italic(bool),
    /// Horizontal text alignment.
    TextAlign(TextAlign),
    /// A 2D affine transform (scale/rotate/translate) on the rendered view.
    Transform(Transform),
    /// A linear gradient background fill.
    Gradient(LinearGradient),
    /// Number of lines for a text label (0 = unlimited).
    NumberOfLines(u32),
    /// Raw image bytes (PNG/JPEG) to display in an image view.
    ImageData(std::sync::Arc<Vec<u8>>),
    /// Whether the scroll view scrolls horizontally instead of vertically.
    Horizontal(bool),
    /// Enables pull-to-refresh on a scroll view; the value is whether it's currently refreshing.
    Refreshing(bool),
    /// Label on the keyboard's return key.
    ReturnKey(ReturnKeyType),
    /// Whether the text field is secure (password).
    Secure(bool),
    /// Enables QR code scanning on a Camera widget.
    QrScanning(bool),
    /// Custom font family by PostScript name (e.g. `"Georgia-Bold"`). Falls
    /// back to the system font if the name is not found on device.
    FontFamily(String),
    /// Keyboard type for text inputs (maps to `UIKeyboardType` on iOS).
    KeyboardType(KeyboardType),
    /// Rich text with inline spans (overrides `Text` if present).
    RichText(Vec<TextSpan>),
    /// URL to load in a WebView widget.
    Url(String),
    /// Raw HTML to load in a WebView widget.
    Html(String),
    /// Use a dynamic-type text style instead of a fixed font size.
    TextStyle(TextStyle),
}

/// A semantic text style that scales with the user's preferred reading size
/// (maps to `UIFontTextStyle` on iOS).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextStyle {
    /// Large title.
    LargeTitle,
    /// Title 1.
    Title1,
    /// Title 2.
    Title2,
    /// Title 3.
    Title3,
    /// Headline.
    Headline,
    /// Subheadline.
    Subheadline,
    /// Body text.
    Body,
    /// Callout.
    Callout,
    /// Footnote.
    Footnote,
    /// Caption 1.
    Caption1,
    /// Caption 2.
    Caption2,
}

/// A text span with inline styling.
#[derive(Debug, Clone, PartialEq)]
pub struct TextSpan {
    /// The text content of this span.
    pub text: String,
    /// Optional text color.
    pub color: Option<rax_core::Color>,
    /// Optional font size in points.
    pub font_size: Option<f32>,
    /// Bold weight.
    pub bold: bool,
    /// Italic style.
    pub italic: bool,
    /// Underline decoration.
    pub underline: bool,
}

impl TextSpan {
    /// Create a new span with the given text and all styling defaults.
    pub fn new(text: impl Into<String>) -> Self {
        TextSpan {
            text: text.into(),
            color: None,
            font_size: None,
            bold: false,
            italic: false,
            underline: false,
        }
    }

    /// Set the span color.
    pub fn color(mut self, c: rax_core::Color) -> Self {
        self.color = Some(c);
        self
    }

    /// Set the span font size.
    pub fn font_size(mut self, s: f32) -> Self {
        self.font_size = Some(s);
        self
    }

    /// Make this span bold.
    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    /// Make this span italic.
    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    /// Underline this span.
    pub fn underline(mut self) -> Self {
        self.underline = true;
        self
    }
}

/// A linear color gradient used as a background fill. `start` and `end` are in
/// unit coordinates (`0,0` = top-left, `1,1` = bottom-right).
#[derive(Debug, Clone, PartialEq)]
pub struct LinearGradient {
    /// Two or more color stops, evenly distributed.
    pub colors: Vec<Color>,
    /// Gradient start point in unit coordinates.
    pub start: (f32, f32),
    /// Gradient end point in unit coordinates.
    pub end: (f32, f32),
}

impl LinearGradient {
    /// A vertical (top-to-bottom) gradient through `colors`.
    pub fn vertical(colors: impl IntoIterator<Item = Color>) -> Self {
        LinearGradient {
            colors: colors.into_iter().collect(),
            start: (0.5, 0.0),
            end: (0.5, 1.0),
        }
    }

    /// A horizontal (leading-to-trailing) gradient through `colors`.
    pub fn horizontal(colors: impl IntoIterator<Item = Color>) -> Self {
        LinearGradient {
            colors: colors.into_iter().collect(),
            start: (0.0, 0.5),
            end: (1.0, 0.5),
        }
    }
}

/// A 2D affine transform applied to a widget's rendering: scale, then rotate,
/// then translate (composed in that order). Build from [`Transform::IDENTITY`]
/// with the chainable setters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    /// Horizontal translation in points.
    pub translate_x: f32,
    /// Vertical translation in points.
    pub translate_y: f32,
    /// Horizontal scale factor.
    pub scale_x: f32,
    /// Vertical scale factor.
    pub scale_y: f32,
    /// Rotation in radians (clockwise).
    pub rotate: f32,
}

impl Transform {
    /// The identity transform (no change).
    pub const IDENTITY: Transform = Transform {
        translate_x: 0.0,
        translate_y: 0.0,
        scale_x: 1.0,
        scale_y: 1.0,
        rotate: 0.0,
    };

    /// Sets uniform scale.
    #[must_use]
    pub const fn scale(mut self, factor: f32) -> Self {
        self.scale_x = factor;
        self.scale_y = factor;
        self
    }

    /// Sets non-uniform scale.
    #[must_use]
    pub const fn scale_xy(mut self, x: f32, y: f32) -> Self {
        self.scale_x = x;
        self.scale_y = y;
        self
    }

    /// Sets rotation in radians.
    #[must_use]
    pub const fn rotate(mut self, radians: f32) -> Self {
        self.rotate = radians;
        self
    }

    /// Sets translation in points.
    #[must_use]
    pub const fn translate(mut self, x: f32, y: f32) -> Self {
        self.translate_x = x;
        self.translate_y = y;
        self
    }
}

impl Default for Transform {
    fn default() -> Self {
        Transform::IDENTITY
    }
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
    /// Set the backdrop fill shown behind the root — i.e. the safe-area region
    /// (notch/status-bar/home-indicator) not covered by app content. Applied to
    /// the platform window, not any widget.
    SetBackdrop {
        /// The backdrop color.
        color: Color,
    },
    /// Trigger a one-shot haptic feedback pulse. Not retained state — the backend
    /// fires the generator once and discards it.
    Haptic {
        /// The intensity and generator style.
        style: HapticStyle,
    },
    /// Schedule a local notification via the UserNotifications framework.
    ScheduleNotification(LocalNotification),
    /// Cancel a pending local notification by its id.
    CancelNotification {
        /// The notification identifier to cancel.
        id: String,
    },
    /// Trigger a biometric authentication prompt (Face ID / Touch ID).
    /// The result is delivered as a global [`Event::BiometricResult`].
    AuthenticateBiometric {
        /// The localized reason string shown to the user.
        reason: String,
    },
    /// Start receiving location updates. Results arrive via `Event::LocationUpdated`.
    StartLocation,
    /// Stop location updates.
    StopLocation,
    /// Start motion sensor updates. Delivers `Event::MotionUpdated` at ~60 Hz.
    StartMotion {
        /// true = enable accelerometer updates.
        accelerometer: bool,
        /// true = enable gyroscope updates.
        gyroscope: bool,
    },
    /// Stop motion sensor updates.
    StopMotion,
}
