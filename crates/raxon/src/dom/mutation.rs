//! The widget/mutation model: the data crossing the render seam.
//!
//! The engine never touches a platform view directly. Instead it produces a
//! stream of [`Mutation`]s — a flat, `Clone`-able, comparable command list — and
//! a [`Backend`](crate::Backend) applies them to real `UIView`s /
//! `android.view.View`s. Keeping this an inspectable value type is what makes
//! the whole framework testable with zero platform code (assert on the stream)
//! and is the seam that later allows diffing off the main thread.

use std::sync::Arc;

use crate::core::{Color, Index, Rect, Size};

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
    /// A native date/time picker (maps to `UIDatePicker`); current value is
    /// epoch seconds via `DateValue`.
    DatePicker,
    /// A multi-line editable text area (maps to `UITextView`).
    TextArea,
    /// An absolute-position container — children layer on top of each other (ZStack).
    Stack,
    /// A camera preview view that optionally decodes QR codes (maps to an
    /// `AVCaptureSession`-backed `UIView`).
    Camera,
    /// An embedded web view (WKWebView). Receives a URL or HTML string via Attribute::Url.
    WebView,
    /// A virtualized, recycling list widget (maps to UITableView on iOS).
    LazyList,
    /// A map view (MKMapView).
    MapView,
    /// A vector drawing surface rendered from a [`DrawList`](Attribute::DrawList)
    /// of [`DrawCmd`]s (maps to a `CALayer`-backed `UIView` on iOS). The chart
    /// builders in `crate::view` produce one of these.
    Canvas,
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

/// A platform permission Raxon can check or request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionKind {
    /// Foreground location access.
    Location,
    /// Camera capture / preview access.
    Camera,
    /// Microphone capture access.
    Microphone,
    /// Photo-library read access.
    Photos,
    /// User notification authorization.
    Notifications,
    /// Motion sensor access.
    Motion,
}

/// Current platform authorization state for a [`PermissionKind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionStatus {
    /// The backend has not reported a value yet.
    Unknown,
    /// The permission is not supported on this platform.
    Unsupported,
    /// The OS can still prompt the user.
    NotDetermined,
    /// The OS or device policy restricts the permission.
    Restricted,
    /// The user denied the permission.
    Denied,
    /// The permission is granted.
    Granted,
    /// The permission is granted for a limited subset, such as selected photos.
    Limited,
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
    /// Swipe in a cardinal direction (discrete).
    Swipe,
}

/// Cardinal direction for a swipe gesture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwipeDirection {
    /// Swipe toward the left edge.
    Left,
    /// Swipe toward the right edge.
    Right,
    /// Swipe toward the top edge.
    Up,
    /// Swipe toward the bottom edge.
    Down,
}

/// Mouse / pointer cursor style. On mobile this is a no-op; on desktop / iPad
/// pointer-device contexts it controls the system cursor shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorStyle {
    /// The default system cursor (arrow).
    Default,
    /// A pointing-hand cursor, e.g. for links and buttons.
    Pointer,
    /// An I-beam cursor, e.g. for text fields.
    Text,
    /// An open-hand (grab) cursor, e.g. for draggable content.
    Grab,
}

/// Scroll position and velocity reported by `OnScrollChange`.
#[derive(Debug, Clone, PartialEq)]
pub struct ScrollInfo {
    /// Horizontal content offset in points.
    pub offset_x: f32,
    /// Vertical content offset in points.
    pub offset_y: f32,
    /// Horizontal scroll velocity in points per second.
    pub velocity_x: f32,
    /// Vertical scroll velocity in points per second.
    pub velocity_y: f32,
}

/// Controls how the keyboard is dismissed when the user drags a scroll view.
///
/// Maps to `UIScrollView.keyboardDismissMode` on iOS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardDismissMode {
    /// The keyboard is not dismissed automatically (default).
    None,
    /// The keyboard is dismissed when a drag begins.
    OnDrag,
    /// The keyboard follows the drag gesture interactively.
    Interactive,
}

/// Date picker display mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatePickerMode {
    /// Calendar date only.
    Date,
    /// Time of day only.
    Time,
    /// Calendar date and time of day.
    DateTime,
}

/// Native date picker presentation style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatePickerStyle {
    /// Let the platform choose.
    Automatic,
    /// Wheel picker.
    Wheels,
    /// Compact field that opens the picker in an overlay.
    Compact,
    /// Inline calendar/time controls.
    Inline,
}

/// A reference-counted, heap-allocated callback (`Arc<dyn Fn()>`).
///
/// Implements `Clone` by cloning the `Arc` (cheap reference count bump) and
/// `PartialEq` by pointer identity (two distinct `Arc`s are never considered
/// equal, so any re-setting of a callback attribute always triggers a
/// re-render). `Debug` formats as `"<callback>"`.
#[derive(Clone)]
pub struct Callback(pub std::sync::Arc<dyn Fn() + Send + Sync>);

impl Callback {
    /// Call the wrapped function.
    pub fn call(&self) {
        (self.0)();
    }
}

impl std::fmt::Debug for Callback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<callback>")
    }
}

impl PartialEq for Callback {
    fn eq(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &other.0)
    }
}

/// A reference-counted callback that receives a [`ScrollInfo`] value.
///
/// `Clone` bumps the `Arc` reference count; `PartialEq` compares by pointer
/// identity (so any re-setting always triggers a re-render). `Debug` formats
/// as `"<scroll_callback>"`.
#[derive(Clone)]
pub struct ScrollCallback(pub Arc<dyn Fn(ScrollInfo) + Send + Sync>);

impl ScrollCallback {
    /// Call the wrapped function with the given scroll info.
    pub fn call(&self, info: ScrollInfo) {
        (self.0)(info);
    }
}

impl std::fmt::Debug for ScrollCallback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<scroll_callback>")
    }
}

impl PartialEq for ScrollCallback {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// A reference-counted callback fired when an image loads successfully.
///
/// `Clone` bumps the `Arc` reference count; `PartialEq` compares by pointer
/// identity.
#[derive(Clone)]
pub struct ImageLoadCallback(pub Arc<dyn Fn() + Send + Sync>);

impl ImageLoadCallback {
    /// Call the wrapped function.
    pub fn call(&self) {
        (self.0)();
    }
}

impl std::fmt::Debug for ImageLoadCallback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<image_load_callback>")
    }
}

impl PartialEq for ImageLoadCallback {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// A reference-counted callback fired when an image fails to load.
///
/// The argument is a short error description. `Clone` bumps the `Arc`
/// reference count; `PartialEq` compares by pointer identity.
#[derive(Clone)]
pub struct ImageErrorCallback(pub Arc<dyn Fn(String) + Send + Sync>);

impl ImageErrorCallback {
    /// Call the wrapped function with the given error message.
    pub fn call(&self, error: String) {
        (self.0)(error);
    }
}

impl std::fmt::Debug for ImageErrorCallback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<image_error_callback>")
    }
}

impl PartialEq for ImageErrorCallback {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// A single settable **paint** property on a widget.
///
/// These are forwarded to the backend and not retained. Layout *inputs*
/// (direction, padding, gap, size) are not here — they live in
/// [`LayoutStyle`](crate::core::LayoutStyle), retained on the node, and produce
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
    /// Native date picker mode.
    DatePickerMode(DatePickerMode),
    /// Native date picker presentation style.
    DatePickerStyle(DatePickerStyle),
    /// Native date picker current value as seconds since the Unix epoch.
    DateValue(f64),
    /// Minimum selectable date as seconds since the Unix epoch.
    DateMin(f64),
    /// Maximum selectable date as seconds since the Unix epoch.
    DateMax(f64),
    /// For LazyList: total number of items in the data set.
    ItemCount(usize),
    /// For LazyList: estimated height per item in points.
    EstimatedItemHeight(f32),
    /// When true, animate any frame changes to this view with a spring.
    AnimateLayout(bool),
    /// For MapView: set the center coordinate.
    MapCenter { latitude: f64, longitude: f64 },
    /// For MapView: set the zoom level (degrees of span; smaller = more zoomed in).
    MapSpan { lat_span: f64, lon_span: f64 },
    /// For MapView: add or update an annotation at a coordinate.
    MapAnnotation {
        /// Unique ID for this annotation.
        annotation_id: String,
        latitude: f64,
        longitude: f64,
        title: String,
    },
    /// Whether this element is currently selected (e.g. a list row, tab, or
    /// toggle). Maps to `accessibilityTraits.selected` on iOS.
    AccessibilitySelected(bool),
    /// Whether this element is disabled. Maps to `accessibilityTraits.notEnabled`
    /// on iOS / `ViewCompat.setEnabled(false)` on Android.
    AccessibilityDisabled(bool),
    /// Whether this element represents an expanded disclosure region.
    /// Maps to `UIAccessibilityTraitNone` + custom state on iOS.
    AccessibilityExpanded(bool),
    /// Whether this element is currently loading or updating content.
    /// Maps to `accessibilityTraits.updatesFrequently` on iOS.
    AccessibilityBusy(bool),
    /// Extra touch area beyond the view's bounds, in points.
    /// Maps to `UIView` hit-test override on iOS / `TouchDelegate` on Android.
    HitSlop {
        /// Points added above the view.
        top: f32,
        /// Points added to the right of the view.
        right: f32,
        /// Points added below the view.
        bottom: f32,
        /// Points added to the left of the view.
        left: f32,
    },
    /// Letter spacing (tracking) in points. Maps to `NSKernAttributeName` on iOS.
    LetterSpacing(f32),
    /// Line height multiplier. Maps to `NSParagraphStyle.lineSpacing` on iOS.
    LineHeight(f32),
    /// Text decoration (underline / strikethrough). Maps to NSUnderline /
    /// NSStrikethrough attribute keys on iOS.
    TextDecoration(TextDecoration),
    /// Text shadow. Maps to `NSShadowAttributeName` on iOS.
    TextShadow {
        /// Shadow color (alpha controls opacity).
        color: Color,
        /// Horizontal offset in points.
        offset_x: f32,
        /// Vertical offset in points.
        offset_y: f32,
        /// Blur radius in points.
        blur: f32,
    },
    /// Enable or disable scrolling on a scroll view (`UIScrollView.isScrollEnabled`).
    ScrollEnabled(bool),
    /// Show or hide both horizontal and vertical scroll indicators
    /// (`UIScrollView.showsHorizontalScrollIndicator` /
    /// `UIScrollView.showsVerticalScrollIndicator`).
    ShowsScrollIndicator(bool),
    /// Enable paged scrolling so the scroll view snaps to page boundaries
    /// (`UIScrollView.isPagingEnabled`). Used by carousel-style layouts.
    PagingEnabled(bool),
    /// Inset the scrollable content area from each edge, in points
    /// (`UIScrollView.contentInset`).
    ContentInset {
        /// Inset from the top edge.
        top: f32,
        /// Inset from the right edge.
        right: f32,
        /// Inset from the bottom edge.
        bottom: f32,
        /// Inset from the left edge.
        left: f32,
    },
    /// Fires when a touch begins on this view (touch-down / press-in).
    OnPressIn(Callback),
    /// Fires when a touch ends (or is cancelled) on this view (press-out).
    OnPressOut(Callback),
    /// Pointer cursor style shown over this view on pointer-device contexts.
    /// No-op on touch-only platforms.
    Cursor(CursorStyle),
    /// Fires when the user performs a swipe gesture in `direction`.
    OnSwipe {
        /// The required swipe direction.
        direction: SwipeDirection,
        /// The handler to invoke.
        handler: Callback,
    },
    /// Color applied to the placeholder text of a text field.
    PlaceholderColor(Color),
    /// A short label shown to the left of the text field's content (prefix).
    InputPrefix(String),
    /// A short label shown to the right of the text field's content (suffix).
    InputSuffix(String),
    /// Show or hide the built-in clear (×) button on a text field.
    ClearButton(bool),
    /// If `true`, the text field / area is non-editable (read-only).
    ReadOnly(bool),
    /// Maximum number of characters the text field will accept.
    MaxLength(usize),
    /// Blur / backdrop-filter effect radius in points. iOS maps to
    /// `UIBlurEffect` + `UIVisualEffectView` as a subview.
    BlurRadius(f32),
    /// Clip subviews to the view's bounds (`clipsToBounds` / `masksToBounds`).
    ClipToBounds(bool),
    /// Z-order on the CALayer (higher values render on top of lower ones).
    ZIndex(i32),
    /// The visual style of the iOS status bar (dark / light / auto).
    /// Maps to `UIStatusBarStyle` on iOS.
    StatusBarStyle(StatusBarStyle),
    /// Enforce a fixed width-to-height aspect ratio on this view.
    /// Maps to an `NSLayoutConstraint` aspect-ratio constraint on iOS.
    AspectRatio(f32),
    /// Flex order (CSS `order`). Lower values render/lay-out first.
    /// Maps to `UIStackView` ordering or manual sort in the backend.
    FlexOrder(i32),
    /// Whether the text in this label is user-selectable (copy/select).
    /// On iOS, enabling requires a `UITextView` swap — set as a hint for now.
    UserSelectText(bool),
    /// Paragraph spacing in points added after each paragraph break.
    /// Maps to `NSParagraphStyle.paragraphSpacing` on iOS.
    ParagraphSpacing(f32),
    /// Italic / oblique style for a text label.
    /// Maps to `UIFont.italicSystemFont` or a slanted variant on iOS.
    FontStyle(FontStyle),
    /// Group this element's children into a single accessibility container.
    /// When `true`, the view itself is not an accessibility element but its
    /// children are grouped. Maps to `isAccessibilityElement = false` on iOS.
    AccessibilityGroup(bool),
    /// Heading level for this element (1–6). When `level > 0`, adds the
    /// `UIAccessibilityTraitHeader` bitmask (0x10000) to the view's traits.
    /// Level 0 means no heading.
    AccessibilityHeadingLevel(u8),
    /// Named custom accessibility actions to expose on this element.
    /// Each string becomes a `UIAccessibilityCustomAction` on iOS and emits
    /// [`Event::AccessibilityAction`](crate::dom::Event::AccessibilityAction)
    /// when selected.
    AccessibilityActions(Vec<String>),
    /// When `true`, the view's font adjusts for the user's preferred content
    /// size category (Dynamic Type). Maps to
    /// `adjustsFontForContentSizeCategory = true` on iOS.
    DynamicType(bool),
    /// A string read by VoiceOver as the element's current value (e.g. "50%"
    /// for a progress indicator). Maps to `accessibilityValue` on iOS.
    AccessibilityValueString(String),
    /// Fires continuously while the user scrolls a scroll view, reporting
    /// the current offset and estimated velocity.
    OnScrollChange(ScrollCallback),
    /// Fires when the user begins dragging a scroll view.
    OnScrollBegin(Callback),
    /// Fires when the scroll view comes to rest after scrolling.
    OnScrollEnd(Callback),
    /// How the keyboard is dismissed when the user drags a scroll view.
    /// Maps to `UIScrollView.keyboardDismissMode` on iOS.
    KeyboardDismissMode(KeyboardDismissMode),
    /// How the image view scales/positions its content to fit its bounds.
    /// Maps to `UIView.contentMode` on iOS.
    ImageResizeMode(ImageResizeMode),
    /// Fired when the image view successfully loads its image.
    /// iOS: fired for native asset/system-symbol and decoded raw-data loads.
    ImageOnLoad(ImageLoadCallback),
    /// Fired when the image view fails to load its image.
    /// The `String` argument carries a short error description.
    /// iOS: fired for missing native sources and undecodable raw image data.
    ImageOnError(ImageErrorCallback),
    /// The list of vector commands to render on a [`WidgetKind::Canvas`].
    /// Re-applying replaces the previous drawing.
    DrawList(Vec<DrawCmd>),
    /// A long-press context menu for any view: an ordered list of menu items.
    /// On iOS this installs a `UIContextMenuInteraction`.
    ContextMenu(Vec<MenuItem>),
}

/// One entry in a [`ContextMenu`](Attribute::ContextMenu).
///
/// Selecting the item invokes its `action`. `Clone` bumps the `Arc`; equality
/// compares title, icon, and action pointer identity.
#[derive(Clone)]
pub struct MenuItem {
    /// The menu row's title.
    pub title: String,
    /// Optional leading SF Symbol name (e.g. `"trash"`); `None` for no icon.
    pub icon: Option<String>,
    /// Whether the item is rendered in a destructive (red) style.
    pub destructive: bool,
    /// Invoked when the user selects this item.
    pub action: std::sync::Arc<dyn Fn() + Send + Sync>,
}

impl MenuItem {
    /// A menu item titled `title` that runs `action` when selected.
    pub fn new(title: impl Into<String>, action: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            title: title.into(),
            icon: None,
            destructive: false,
            action: std::sync::Arc::new(action),
        }
    }

    /// Adds a leading SF Symbol icon (e.g. `"trash"`).
    #[must_use]
    pub fn icon(mut self, name: impl Into<String>) -> Self {
        self.icon = Some(name.into());
        self
    }

    /// Renders the item in a destructive (red) style.
    #[must_use]
    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }

    /// Invoke this item's action.
    pub fn call(&self) {
        (self.action)();
    }
}

impl std::fmt::Debug for MenuItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MenuItem")
            .field("title", &self.title)
            .field("icon", &self.icon)
            .field("destructive", &self.destructive)
            .finish_non_exhaustive()
    }
}

impl PartialEq for MenuItem {
    fn eq(&self, other: &Self) -> bool {
        self.title == other.title
            && self.icon == other.icon
            && self.destructive == other.destructive
            && std::sync::Arc::ptr_eq(&self.action, &other.action)
    }
}

/// How an image view scales/positions its content to fit its bounds.
///
/// Maps to `UIView.contentMode` on iOS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageResizeMode {
    /// Scale to fill, clipping excess (UIViewContentModeScaleAspectFill = 1).
    Cover,
    /// Scale to fit, letter-boxing if needed (UIViewContentModeScaleAspectFit = 2).
    Contain,
    /// Scale to fill the bounds exactly, ignoring aspect ratio
    /// (UIViewContentModeScaleToFill = 0).
    Stretch,
    /// Center without scaling (UIViewContentModeCenter = 4).
    Center,
    /// Tile the image (stub; actual tiling requires a CALayer pattern fill).
    Repeat,
}

/// The visual style of the iOS status bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusBarStyle {
    /// Dark content (use on light backgrounds).
    Dark,
    /// Light content (use on dark backgrounds).
    Light,
    /// Follows the system / device appearance setting.
    Auto,
}

/// Text decoration applied to a label or span.
#[derive(Clone, Debug, PartialEq)]
pub enum TextDecoration {
    /// No decoration.
    None,
    /// Single underline.
    Underline,
    /// Horizontal strikethrough.
    Strikethrough,
    /// Double underline.
    UnderlineDouble,
}

/// The italic/oblique style of a font.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FontStyle {
    /// Upright (roman) text — the default.
    #[default]
    Normal,
    /// Italic text (UIFont italic system font or a named italic variant).
    Italic,
    /// Oblique text (slanted without a true italic variant; falls back to italic on iOS).
    Oblique,
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
    pub color: Option<crate::core::Color>,
    /// Optional font size in points.
    pub font_size: Option<f32>,
    /// Bold weight.
    pub bold: bool,
    /// Italic style.
    pub italic: bool,
    /// Underline decoration.
    pub underline: bool,
    /// Strikethrough decoration.
    pub strikethrough: bool,
    /// Letter spacing (tracking) in points.
    pub letter_spacing: Option<f32>,
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
            strikethrough: false,
            letter_spacing: None,
        }
    }

    /// Set the span color.
    pub fn color(mut self, c: crate::core::Color) -> Self {
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

    /// Strikethrough this span.
    pub fn strikethrough(mut self) -> Self {
        self.strikethrough = true;
        self
    }

    /// Set letter spacing (tracking) in points.
    pub fn letter_spacing(mut self, kern: f32) -> Self {
        self.letter_spacing = Some(kern);
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

/// A stroke (outline) style for a [`DrawCmd`]: a line width and color.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stroke {
    /// Line width in logical points.
    pub width: f32,
    /// Stroke color.
    pub color: Color,
}

impl Stroke {
    /// A stroke of `width` points in `color`.
    pub const fn new(width: f32, color: Color) -> Self {
        Self { width, color }
    }
}

/// A single vector drawing command for a [`WidgetKind::Canvas`].
///
/// Coordinates are in the canvas's local space — logical points, origin at the
/// top-left with `y` growing downward (matching screen space). The chart
/// builders in `crate::view` emit a `Vec<DrawCmd>` for a fixed size; the iOS
/// backend renders each command with CoreGraphics into a `CALayer`.
#[derive(Debug, Clone, PartialEq)]
pub enum DrawCmd {
    /// A straight line segment from `(x1, y1)` to `(x2, y2)`.
    Line {
        /// Start x.
        x1: f32,
        /// Start y.
        y1: f32,
        /// End x.
        x2: f32,
        /// End y.
        y2: f32,
        /// Line width in points.
        width: f32,
        /// Line color.
        color: Color,
    },
    /// An axis-aligned (optionally rounded) rectangle, optionally filled and/or
    /// stroked.
    Rect {
        /// Left edge.
        x: f32,
        /// Top edge.
        y: f32,
        /// Width.
        w: f32,
        /// Height.
        h: f32,
        /// Corner radius (0 = square corners).
        radius: f32,
        /// Fill color, if any.
        fill: Option<Color>,
        /// Outline stroke, if any.
        stroke: Option<Stroke>,
    },
    /// A circle centered at `(cx, cy)` with radius `r`.
    Circle {
        /// Center x.
        cx: f32,
        /// Center y.
        cy: f32,
        /// Radius.
        r: f32,
        /// Fill color, if any.
        fill: Option<Color>,
        /// Outline stroke, if any.
        stroke: Option<Stroke>,
    },
    /// A polyline (or polygon when `closed`) through `points`. A `fill` paints
    /// the enclosed area (closed shapes); a `stroke` outlines the path.
    Path {
        /// Vertices, in order, as `(x, y)`.
        points: Vec<(f32, f32)>,
        /// Whether the last point connects back to the first.
        closed: bool,
        /// Fill color for the enclosed area, if any.
        fill: Option<Color>,
        /// Outline stroke, if any.
        stroke: Option<Stroke>,
    },
    /// A run of text with its top-left anchored at `(x, y)`.
    Text {
        /// Anchor x.
        x: f32,
        /// Anchor y.
        y: f32,
        /// The string to draw.
        text: String,
        /// Font size in points.
        size: f32,
        /// Text color.
        color: Color,
        /// Horizontal alignment within the text's natural box.
        align: TextAlign,
    },
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
    /// Query the current authorization state for a platform permission.
    /// The result is delivered as a global [`Event::PermissionChanged`].
    CheckPermission {
        /// The permission to inspect.
        permission: PermissionKind,
    },
    /// Request a platform permission from the user.
    /// The result is delivered as a global [`Event::PermissionChanged`].
    RequestPermission {
        /// The permission to request.
        permission: PermissionKind,
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
    /// Present a media picker (PHPickerViewController). Results arrive via `Event::MediaPicked`.
    PresentMediaPicker {
        /// Maximum number of items to select (1 for single-select).
        max_selection: usize,
    },
    /// Present a document picker (UIDocumentPickerViewController). Results arrive
    /// via `Event::DocumentPicked`.
    PresentDocumentPicker {
        /// Allowed UTType identifiers (e.g. `"public.pdf"`, `"public.plain-text"`).
        /// Empty means any file (`public.item`).
        types: Vec<String>,
    },
    /// Register a background task identifier with BGTaskScheduler.
    /// Must be called during app launch before the first background task fires.
    /// The identifier must also appear in `BGTaskSchedulerPermittedIdentifiers` in Info.plist.
    RegisterBackgroundTask {
        /// The task identifier string.
        identifier: String,
    },
    /// Schedule the next execution of a registered background task.
    ScheduleBackgroundTask {
        /// The task identifier string.
        identifier: String,
        /// Earliest begin date offset in seconds from now.
        /// The system may run the task later than this; it is a minimum, not an exact time.
        earliest_seconds: f64,
    },
    /// Copy text to the system clipboard (UIPasteboard on iOS).
    SetClipboard {
        /// The text to write to the clipboard.
        text: String,
    },
    /// Present the system share sheet (UIActivityViewController on iOS) with the given text.
    ShareText {
        /// The text to share.
        text: String,
    },
    /// Open a URL with the platform's default external handler.
    OpenExternalUrl {
        /// The absolute URL to open.
        url: String,
    },
    /// Post an accessibility announcement via `UIAccessibilityPostNotification`.
    /// VoiceOver reads the message immediately, interrupting any in-progress speech.
    AnnounceAccessibility {
        /// The string to announce.
        message: String,
    },
    /// Move VoiceOver focus to the native view backing `id`.
    /// Maps to `UIAccessibilityPostNotification(UIAccessibilityScreenChangedNotification, view)`.
    RequestFocus {
        /// The widget to focus.
        id: WidgetId,
    },
    /// Request GPS location updates. Results arrive via `Event::LocationUpdated`.
    /// iOS: calls `[CLLocationManager startUpdatingLocation]`.
    RequestLocation,
    /// Stop GPS location updates.
    StopLocationUpdates,
    /// Enable or disable the device torch (flashlight).
    /// iOS: sets `AVCaptureDevice` torch mode via `AVCaptureDevice.torchMode`.
    SetTorch {
        /// `true` to turn the torch on, `false` to turn it off.
        on: bool,
    },
    /// Register for Apple Push Notification Service (APNS) remote notifications.
    /// iOS: calls `[UIApplication registerForRemoteNotifications]`.
    /// The device token is published to `runtime::use_push_token()`.
    RegisterForPushNotifications,
    /// Set the app badge count shown on the home screen icon.
    /// iOS: calls `[[UIApplication sharedApplication] setApplicationIconBadgeNumber: count]`.
    SetAppBadge {
        /// The badge count to display (0 clears the badge).
        count: u32,
    },
    /// Programmatically scroll a scroll view to the given content offset.
    /// iOS: `[UIScrollView setContentOffset:animated:]`.
    ScrollTo {
        /// The scroll view widget to scroll.
        id: WidgetId,
        /// Horizontal content offset in points.
        offset_x: f32,
        /// Vertical content offset in points.
        offset_y: f32,
        /// Whether to animate the scroll.
        animated: bool,
    },
    /// Programmatically scroll a scroll view back to the top (offset 0, 0).
    /// iOS: `[UIScrollView setContentOffset:{0,0} animated:]`.
    ScrollToTop {
        /// The scroll view widget to scroll.
        id: WidgetId,
        /// Whether to animate the scroll.
        animated: bool,
    },
}
