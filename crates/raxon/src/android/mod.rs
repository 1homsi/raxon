//! Android backend foundation.
//!
//! This module provides the pure-Rust half of the Android backend: a command
//! queue that translates raxon [`Mutation`](crate::dom::Mutation)s into Android
//! view operations. JNI glue can drain these commands from an Activity and
//! apply them to real `android.view.View` instances while sending native events
//! back through [`EventSink`](crate::dom::EventSink).

#![forbid(unsafe_code)]

use std::cell::RefCell;
use std::rc::Rc;

use crate::core::{Color, Rect, Size};
use crate::dom::{
    Attribute, Backend, Event, EventSink, GestureKind, HapticStyle, Host, LocalNotification,
    Mutation, WidgetId, WidgetKind,
};
use crate::runtime::App;
use crate::view::View;

/// A shared queue of Android commands produced by [`AndroidBackend`].
pub type AndroidCommandQueue = Rc<RefCell<Vec<AndroidCommand>>>;

/// Android host session used by generated JNI glue.
pub type AndroidHostSession = crate::host::HostSession<AndroidDriver>;

/// Android host-session registry keyed by opaque JNI-safe handles.
pub type AndroidHostSessionRegistry = crate::host::HostSessionRegistry<AndroidDriver>;

/// Host-originated Android event payload for JNI adapters.
pub type AndroidWireEvent = crate::wire::WireEvent;

/// Batch of host-originated Android events for JNI adapters.
pub type AndroidWireEventBatch = crate::wire::WireEventBatch;

/// Android view classes used by the first native backend pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AndroidViewClass {
    /// A generic `FrameLayout` container.
    FrameLayout,
    /// A `TextView`.
    TextView,
    /// A `Button`.
    Button,
    /// An `ImageView`.
    ImageView,
    /// A `Switch`.
    Switch,
    /// A `SeekBar`.
    SeekBar,
    /// An `EditText`.
    EditText,
    /// A `ScrollView` / `HorizontalScrollView`.
    ScrollView,
    /// A `ProgressBar` in indeterminate mode.
    ActivityIndicator,
    /// A determinate `ProgressBar`.
    ProgressBar,
    /// A segmented-control host, typically backed by Material buttons.
    SegmentedControl,
    /// A numeric stepper host.
    Stepper,
    /// A `DatePicker` or `TimePicker` host.
    DatePicker,
    /// A camera preview host.
    CameraPreview,
    /// A `WebView`.
    WebView,
    /// A `RecyclerView`.
    RecyclerView,
    /// A map view host.
    MapView,
    /// A custom canvas view.
    CanvasView,
}

impl AndroidViewClass {
    /// Maps a framework widget kind to the Android view class that should back it.
    pub const fn from_widget_kind(kind: WidgetKind) -> Self {
        match kind {
            WidgetKind::View | WidgetKind::Stack => AndroidViewClass::FrameLayout,
            WidgetKind::Text => AndroidViewClass::TextView,
            WidgetKind::Button => AndroidViewClass::Button,
            WidgetKind::Image => AndroidViewClass::ImageView,
            WidgetKind::Switch => AndroidViewClass::Switch,
            WidgetKind::Slider => AndroidViewClass::SeekBar,
            WidgetKind::TextInput | WidgetKind::TextArea => AndroidViewClass::EditText,
            WidgetKind::Scroll => AndroidViewClass::ScrollView,
            WidgetKind::ActivityIndicator => AndroidViewClass::ActivityIndicator,
            WidgetKind::Progress => AndroidViewClass::ProgressBar,
            WidgetKind::Segmented => AndroidViewClass::SegmentedControl,
            WidgetKind::Stepper => AndroidViewClass::Stepper,
            WidgetKind::DatePicker => AndroidViewClass::DatePicker,
            WidgetKind::Camera => AndroidViewClass::CameraPreview,
            WidgetKind::WebView => AndroidViewClass::WebView,
            WidgetKind::LazyList => AndroidViewClass::RecyclerView,
            WidgetKind::MapView => AndroidViewClass::MapView,
            WidgetKind::Canvas => AndroidViewClass::CanvasView,
        }
    }

    /// Fully-qualified Android class name for the default host implementation.
    pub const fn class_name(self) -> &'static str {
        match self {
            AndroidViewClass::FrameLayout => "android.widget.FrameLayout",
            AndroidViewClass::TextView => "android.widget.TextView",
            AndroidViewClass::Button => "android.widget.Button",
            AndroidViewClass::ImageView => "android.widget.ImageView",
            AndroidViewClass::Switch => "android.widget.Switch",
            AndroidViewClass::SeekBar => "android.widget.SeekBar",
            AndroidViewClass::EditText => "android.widget.EditText",
            AndroidViewClass::ScrollView => "android.widget.ScrollView",
            AndroidViewClass::ActivityIndicator => "android.widget.ProgressBar",
            AndroidViewClass::ProgressBar => "android.widget.ProgressBar",
            AndroidViewClass::SegmentedControl => {
                "com.google.android.material.button.MaterialButtonToggleGroup"
            }
            AndroidViewClass::Stepper => "android.widget.NumberPicker",
            AndroidViewClass::DatePicker => "android.widget.DatePicker",
            AndroidViewClass::CameraPreview => "android.view.TextureView",
            AndroidViewClass::WebView => "android.webkit.WebView",
            AndroidViewClass::RecyclerView => "androidx.recyclerview.widget.RecyclerView",
            AndroidViewClass::MapView => "com.google.android.gms.maps.MapView",
            AndroidViewClass::CanvasView => "android.view.View",
        }
    }
}

/// A command for the Android host layer to apply to real views.
#[derive(Debug, Clone, PartialEq)]
pub enum AndroidCommand {
    /// Create a native view.
    Create {
        /// Stable widget id.
        id: u64,
        /// Android view class to instantiate.
        class: AndroidViewClass,
    },
    /// Set a platform-neutral attribute on a view.
    SetAttribute {
        /// Stable widget id.
        id: u64,
        /// Attribute payload.
        attr: Attribute,
    },
    /// Set a view frame in logical pixels.
    SetFrame {
        /// Stable widget id.
        id: u64,
        /// Left coordinate.
        x: f32,
        /// Top coordinate.
        y: f32,
        /// Width.
        width: f32,
        /// Height.
        height: f32,
    },
    /// Insert a child into a parent container.
    InsertChild {
        /// Parent widget id.
        parent: u64,
        /// Child widget id.
        child: u64,
        /// Child index.
        index: usize,
    },
    /// Remove a child from a parent container.
    RemoveChild {
        /// Parent widget id.
        parent: u64,
        /// Child widget id.
        child: u64,
    },
    /// Destroy a native view.
    Destroy {
        /// Stable widget id.
        id: u64,
    },
    /// Attach the root view to the Activity content view.
    SetRoot {
        /// Root widget id.
        id: u64,
    },
    /// Register a gesture recognizer/listener.
    AddGesture {
        /// Stable widget id.
        id: u64,
        /// Gesture kind.
        gesture: GestureKind,
    },
    /// Set a scrollable content size.
    SetContentSize {
        /// Scroll widget id.
        id: u64,
        /// Content width.
        width: f32,
        /// Content height.
        height: f32,
    },
    /// Set the Activity/window backdrop color as Android `0xAARRGGBB`.
    SetBackdrop {
        /// Packed Android color.
        argb: u32,
    },
    /// Trigger Android haptic feedback.
    Haptic {
        /// Haptic style.
        style: HapticStyle,
    },
    /// Invoke a platform service.
    Request(AndroidPlatformRequest),
    /// Scroll a scroll view to an explicit offset.
    ScrollTo {
        /// Scroll widget id.
        id: u64,
        /// Horizontal offset.
        offset_x: f32,
        /// Vertical offset.
        offset_y: f32,
        /// Whether the scroll should animate.
        animated: bool,
    },
    /// Scroll a scroll view to the top-left origin.
    ScrollToTop {
        /// Scroll widget id.
        id: u64,
        /// Whether the scroll should animate.
        animated: bool,
    },
}

impl AndroidCommand {
    /// Translates a framework mutation into an Android host command.
    pub fn from_mutation(mutation: Mutation) -> Self {
        match mutation {
            Mutation::Create { id, kind } => AndroidCommand::Create {
                id: widget_key(id),
                class: AndroidViewClass::from_widget_kind(kind),
            },
            Mutation::SetAttribute { id, attr } => AndroidCommand::SetAttribute {
                id: widget_key(id),
                attr,
            },
            Mutation::SetFrame { id, rect } => AndroidCommand::SetFrame {
                id: widget_key(id),
                x: rect.origin.x,
                y: rect.origin.y,
                width: rect.size.width,
                height: rect.size.height,
            },
            Mutation::InsertChild {
                parent,
                index,
                child,
            } => AndroidCommand::InsertChild {
                parent: widget_key(parent),
                child: widget_key(child),
                index,
            },
            Mutation::RemoveChild { parent, child } => AndroidCommand::RemoveChild {
                parent: widget_key(parent),
                child: widget_key(child),
            },
            Mutation::Destroy { id } => AndroidCommand::Destroy { id: widget_key(id) },
            Mutation::SetRoot { id } => AndroidCommand::SetRoot { id: widget_key(id) },
            Mutation::AddGesture { id, gesture } => AndroidCommand::AddGesture {
                id: widget_key(id),
                gesture,
            },
            Mutation::SetContentSize { id, size } => AndroidCommand::SetContentSize {
                id: widget_key(id),
                width: size.width,
                height: size.height,
            },
            Mutation::SetBackdrop { color } => AndroidCommand::SetBackdrop {
                argb: color.to_argb_u32(),
            },
            Mutation::Haptic { style } => AndroidCommand::Haptic { style },
            Mutation::ScheduleNotification(notification) => {
                AndroidCommand::Request(AndroidPlatformRequest::ScheduleNotification(notification))
            }
            Mutation::CancelNotification { id } => {
                AndroidCommand::Request(AndroidPlatformRequest::CancelNotification { id })
            }
            Mutation::AuthenticateBiometric { reason } => {
                AndroidCommand::Request(AndroidPlatformRequest::AuthenticateBiometric { reason })
            }
            Mutation::StartLocation | Mutation::RequestLocation => {
                AndroidCommand::Request(AndroidPlatformRequest::StartLocation)
            }
            Mutation::StopLocation | Mutation::StopLocationUpdates => {
                AndroidCommand::Request(AndroidPlatformRequest::StopLocation)
            }
            Mutation::StartMotion {
                accelerometer,
                gyroscope,
            } => AndroidCommand::Request(AndroidPlatformRequest::StartMotion {
                accelerometer,
                gyroscope,
            }),
            Mutation::StopMotion => AndroidCommand::Request(AndroidPlatformRequest::StopMotion),
            Mutation::PresentMediaPicker { max_selection } => {
                AndroidCommand::Request(AndroidPlatformRequest::PresentMediaPicker {
                    max_selection,
                })
            }
            Mutation::PresentDocumentPicker { types } => {
                AndroidCommand::Request(AndroidPlatformRequest::PresentDocumentPicker { types })
            }
            Mutation::RegisterBackgroundTask { identifier } => {
                AndroidCommand::Request(AndroidPlatformRequest::RegisterBackgroundTask {
                    identifier,
                })
            }
            Mutation::ScheduleBackgroundTask {
                identifier,
                earliest_seconds,
            } => AndroidCommand::Request(AndroidPlatformRequest::ScheduleBackgroundTask {
                identifier,
                earliest_seconds,
            }),
            Mutation::SetClipboard { text } => {
                AndroidCommand::Request(AndroidPlatformRequest::SetClipboard { text })
            }
            Mutation::ShareText { text } => {
                AndroidCommand::Request(AndroidPlatformRequest::ShareText { text })
            }
            Mutation::AnnounceAccessibility { message } => {
                AndroidCommand::Request(AndroidPlatformRequest::AnnounceAccessibility { message })
            }
            Mutation::RequestFocus { id } => {
                AndroidCommand::Request(AndroidPlatformRequest::RequestFocus { id: widget_key(id) })
            }
            Mutation::SetTorch { on } => {
                AndroidCommand::Request(AndroidPlatformRequest::SetTorch { on })
            }
            Mutation::RegisterForPushNotifications => {
                AndroidCommand::Request(AndroidPlatformRequest::RegisterForPushNotifications)
            }
            Mutation::SetAppBadge { count } => {
                AndroidCommand::Request(AndroidPlatformRequest::SetAppBadge { count })
            }
            Mutation::ScrollTo {
                id,
                offset_x,
                offset_y,
                animated,
            } => AndroidCommand::ScrollTo {
                id: widget_key(id),
                offset_x,
                offset_y,
                animated,
            },
            Mutation::ScrollToTop { id, animated } => AndroidCommand::ScrollToTop {
                id: widget_key(id),
                animated,
            },
        }
    }
}

/// A single drained Android frame encoded for JNI or host adapters.
#[allow(missing_docs)]
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidCommandBatch {
    pub commands: Vec<AndroidWireCommand>,
}

impl AndroidCommandBatch {
    /// Converts native backend commands into the host-facing wire representation.
    pub fn from_commands(commands: Vec<AndroidCommand>) -> Self {
        AndroidCommandBatch {
            commands: commands.into_iter().map(AndroidWireCommand::from).collect(),
        }
    }

    /// Number of commands in this frame batch.
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// Whether this frame contains no host work.
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Encodes the whole frame as JSON for a JNI boundary that wants one string.
    pub fn encode_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

/// Android command payload with only host-serializable values.
#[allow(missing_docs)]
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AndroidWireCommand {
    Create {
        id: u64,
        class_name: String,
    },
    SetAttribute {
        id: u64,
        attr: AndroidWireAttribute,
    },
    SetFrame {
        id: u64,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    InsertChild {
        parent: u64,
        child: u64,
        index: u64,
    },
    RemoveChild {
        parent: u64,
        child: u64,
    },
    Destroy {
        id: u64,
    },
    SetRoot {
        id: u64,
    },
    AddGesture {
        id: u64,
        gesture: String,
    },
    SetContentSize {
        id: u64,
        width: f32,
        height: f32,
    },
    SetBackdrop {
        argb: u32,
    },
    Haptic {
        style: String,
    },
    Request {
        request: AndroidWirePlatformRequest,
    },
    ScrollTo {
        id: u64,
        offset_x: f32,
        offset_y: f32,
        animated: bool,
    },
    ScrollToTop {
        id: u64,
        animated: bool,
    },
}

impl From<AndroidCommand> for AndroidWireCommand {
    fn from(command: AndroidCommand) -> Self {
        match command {
            AndroidCommand::Create { id, class } => AndroidWireCommand::Create {
                id,
                class_name: class.class_name().to_string(),
            },
            AndroidCommand::SetAttribute { id, attr } => AndroidWireCommand::SetAttribute {
                id,
                attr: android_wire_attribute(attr),
            },
            AndroidCommand::SetFrame {
                id,
                x,
                y,
                width,
                height,
            } => AndroidWireCommand::SetFrame {
                id,
                x,
                y,
                width,
                height,
            },
            AndroidCommand::InsertChild {
                parent,
                child,
                index,
            } => AndroidWireCommand::InsertChild {
                parent,
                child,
                index: index as u64,
            },
            AndroidCommand::RemoveChild { parent, child } => {
                AndroidWireCommand::RemoveChild { parent, child }
            }
            AndroidCommand::Destroy { id } => AndroidWireCommand::Destroy { id },
            AndroidCommand::SetRoot { id } => AndroidWireCommand::SetRoot { id },
            AndroidCommand::AddGesture { id, gesture } => AndroidWireCommand::AddGesture {
                id,
                gesture: debug_label(gesture),
            },
            AndroidCommand::SetContentSize { id, width, height } => {
                AndroidWireCommand::SetContentSize { id, width, height }
            }
            AndroidCommand::SetBackdrop { argb } => AndroidWireCommand::SetBackdrop { argb },
            AndroidCommand::Haptic { style } => AndroidWireCommand::Haptic {
                style: debug_label(style),
            },
            AndroidCommand::Request(request) => AndroidWireCommand::Request {
                request: AndroidWirePlatformRequest::from(request),
            },
            AndroidCommand::ScrollTo {
                id,
                offset_x,
                offset_y,
                animated,
            } => AndroidWireCommand::ScrollTo {
                id,
                offset_x,
                offset_y,
                animated,
            },
            AndroidCommand::ScrollToTop { id, animated } => {
                AndroidWireCommand::ScrollToTop { id, animated }
            }
        }
    }
}

/// Android platform request payload with primitive/string host values.
#[allow(missing_docs)]
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AndroidWirePlatformRequest {
    ScheduleNotification {
        id: String,
        title: String,
        body: String,
        delay_seconds: u32,
    },
    CancelNotification {
        id: String,
    },
    AuthenticateBiometric {
        reason: String,
    },
    StartLocation,
    StopLocation,
    StartMotion {
        accelerometer: bool,
        gyroscope: bool,
    },
    StopMotion,
    PresentMediaPicker {
        max_selection: u64,
    },
    PresentDocumentPicker {
        types: Vec<String>,
    },
    RegisterBackgroundTask {
        identifier: String,
    },
    ScheduleBackgroundTask {
        identifier: String,
        earliest_seconds: f64,
    },
    SetClipboard {
        text: String,
    },
    ShareText {
        text: String,
    },
    AnnounceAccessibility {
        message: String,
    },
    RequestFocus {
        id: u64,
    },
    SetTorch {
        on: bool,
    },
    RegisterForPushNotifications,
    SetAppBadge {
        count: u32,
    },
}

impl From<AndroidPlatformRequest> for AndroidWirePlatformRequest {
    fn from(request: AndroidPlatformRequest) -> Self {
        match request {
            AndroidPlatformRequest::ScheduleNotification(notification) => {
                AndroidWirePlatformRequest::ScheduleNotification {
                    id: notification.id,
                    title: notification.title,
                    body: notification.body,
                    delay_seconds: notification.delay_seconds,
                }
            }
            AndroidPlatformRequest::CancelNotification { id } => {
                AndroidWirePlatformRequest::CancelNotification { id }
            }
            AndroidPlatformRequest::AuthenticateBiometric { reason } => {
                AndroidWirePlatformRequest::AuthenticateBiometric { reason }
            }
            AndroidPlatformRequest::StartLocation => AndroidWirePlatformRequest::StartLocation,
            AndroidPlatformRequest::StopLocation => AndroidWirePlatformRequest::StopLocation,
            AndroidPlatformRequest::StartMotion {
                accelerometer,
                gyroscope,
            } => AndroidWirePlatformRequest::StartMotion {
                accelerometer,
                gyroscope,
            },
            AndroidPlatformRequest::StopMotion => AndroidWirePlatformRequest::StopMotion,
            AndroidPlatformRequest::PresentMediaPicker { max_selection } => {
                AndroidWirePlatformRequest::PresentMediaPicker {
                    max_selection: max_selection as u64,
                }
            }
            AndroidPlatformRequest::PresentDocumentPicker { types } => {
                AndroidWirePlatformRequest::PresentDocumentPicker { types }
            }
            AndroidPlatformRequest::RegisterBackgroundTask { identifier } => {
                AndroidWirePlatformRequest::RegisterBackgroundTask { identifier }
            }
            AndroidPlatformRequest::ScheduleBackgroundTask {
                identifier,
                earliest_seconds,
            } => AndroidWirePlatformRequest::ScheduleBackgroundTask {
                identifier,
                earliest_seconds,
            },
            AndroidPlatformRequest::SetClipboard { text } => {
                AndroidWirePlatformRequest::SetClipboard { text }
            }
            AndroidPlatformRequest::ShareText { text } => {
                AndroidWirePlatformRequest::ShareText { text }
            }
            AndroidPlatformRequest::AnnounceAccessibility { message } => {
                AndroidWirePlatformRequest::AnnounceAccessibility { message }
            }
            AndroidPlatformRequest::RequestFocus { id } => {
                AndroidWirePlatformRequest::RequestFocus { id }
            }
            AndroidPlatformRequest::SetTorch { on } => AndroidWirePlatformRequest::SetTorch { on },
            AndroidPlatformRequest::RegisterForPushNotifications => {
                AndroidWirePlatformRequest::RegisterForPushNotifications
            }
            AndroidPlatformRequest::SetAppBadge { count } => {
                AndroidWirePlatformRequest::SetAppBadge { count }
            }
        }
    }
}

/// Android attribute payload with callbacks replaced by listener intent markers.
#[allow(missing_docs)]
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(tag = "name", content = "value", rename_all = "snake_case")]
pub enum AndroidWireAttribute {
    Text(String),
    FontSize(f32),
    TextColor(u32),
    BackgroundColor(u32),
    CornerRadius(f32),
    Opacity(f32),
    BorderWidth(f32),
    BorderColor(u32),
    Shadow {
        color_argb: u32,
        radius: f32,
        dx: f32,
        dy: f32,
    },
    ImageSource(String),
    ImageData(Vec<u8>),
    BoolValue(bool),
    FloatValue(f32),
    TintColor(u32),
    Placeholder(String),
    Items(Vec<String>),
    Range {
        min: f32,
        max: f32,
        step: f32,
    },
    AccessibilityLabel(String),
    AccessibilityHint(String),
    AccessibilityRole(String),
    AccessibilityHidden(bool),
    Direction(String),
    FontWeight(f32),
    Italic(bool),
    TextAlign(String),
    Transform {
        translate_x: f32,
        translate_y: f32,
        scale_x: f32,
        scale_y: f32,
        rotate: f32,
    },
    Gradient {
        colors_argb: Vec<u32>,
        start_x: f32,
        start_y: f32,
        end_x: f32,
        end_y: f32,
    },
    NumberOfLines(u32),
    Horizontal(bool),
    Refreshing(bool),
    ReturnKey(String),
    Secure(bool),
    QrScanning(bool),
    FontFamily(String),
    KeyboardType(String),
    RichText(Vec<AndroidWireTextSpan>),
    Url(String),
    Html(String),
    TextStyle(String),
    DatePickerMode(String),
    DatePickerStyle(String),
    DateValue(f64),
    DateMin(f64),
    DateMax(f64),
    ItemCount(u64),
    EstimatedItemHeight(f32),
    AnimateLayout(bool),
    MapCenter {
        latitude: f64,
        longitude: f64,
    },
    MapSpan {
        lat_span: f64,
        lon_span: f64,
    },
    MapAnnotation {
        annotation_id: String,
        latitude: f64,
        longitude: f64,
        title: String,
    },
    AccessibilitySelected(bool),
    AccessibilityDisabled(bool),
    AccessibilityExpanded(bool),
    AccessibilityBusy(bool),
    HitSlop {
        top: f32,
        right: f32,
        bottom: f32,
        left: f32,
    },
    LetterSpacing(f32),
    LineHeight(f32),
    TextDecoration(String),
    TextShadow {
        color_argb: u32,
        offset_x: f32,
        offset_y: f32,
        blur: f32,
    },
    ScrollEnabled(bool),
    ShowsScrollIndicator(bool),
    PagingEnabled(bool),
    ContentInset {
        top: f32,
        right: f32,
        bottom: f32,
        left: f32,
    },
    EventListener {
        event: String,
    },
    SwipeListener {
        direction: String,
    },
    Cursor(String),
    PlaceholderColor(u32),
    InputPrefix(String),
    InputSuffix(String),
    ClearButton(bool),
    ReadOnly(bool),
    MaxLength(u64),
    BlurRadius(f32),
    ClipToBounds(bool),
    ZIndex(i32),
    StatusBarStyle(String),
    AspectRatio(f32),
    FlexOrder(i32),
    UserSelectText(bool),
    ParagraphSpacing(f32),
    FontStyle(String),
    AccessibilityGroup(bool),
    AccessibilityHeadingLevel(u8),
    AccessibilityActions(Vec<String>),
    DynamicType(bool),
    AccessibilityValueString(String),
    KeyboardDismissMode(String),
    ImageResizeMode(String),
    ContextMenu(Vec<AndroidWireMenuItem>),
    Unsupported {
        name: String,
    },
}

/// Android host data for a rich-text span.
#[allow(missing_docs)]
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidWireTextSpan {
    pub text: String,
    pub color_argb: Option<u32>,
    pub font_size: Option<f32>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub letter_spacing: Option<f32>,
}

/// Android host data for a context-menu row without the Rust action closure.
#[allow(missing_docs)]
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidWireMenuItem {
    pub title: String,
    pub icon: Option<String>,
    pub destructive: bool,
}

fn android_wire_attribute(attr: Attribute) -> AndroidWireAttribute {
    match attr {
        Attribute::Text(value) => AndroidWireAttribute::Text(value),
        Attribute::FontSize(value) => AndroidWireAttribute::FontSize(value),
        Attribute::TextColor(color) => AndroidWireAttribute::TextColor(color.to_argb_u32()),
        Attribute::BackgroundColor(color) => {
            AndroidWireAttribute::BackgroundColor(color.to_argb_u32())
        }
        Attribute::CornerRadius(value) => AndroidWireAttribute::CornerRadius(value),
        Attribute::Opacity(value) => AndroidWireAttribute::Opacity(value),
        Attribute::BorderWidth(value) => AndroidWireAttribute::BorderWidth(value),
        Attribute::BorderColor(color) => AndroidWireAttribute::BorderColor(color.to_argb_u32()),
        Attribute::Shadow(shadow) => AndroidWireAttribute::Shadow {
            color_argb: shadow.color.to_argb_u32(),
            radius: shadow.radius,
            dx: shadow.dx,
            dy: shadow.dy,
        },
        Attribute::ImageSource(value) => AndroidWireAttribute::ImageSource(value),
        Attribute::BoolValue(value) => AndroidWireAttribute::BoolValue(value),
        Attribute::FloatValue(value) => AndroidWireAttribute::FloatValue(value),
        Attribute::TintColor(color) => AndroidWireAttribute::TintColor(color.to_argb_u32()),
        Attribute::Placeholder(value) => AndroidWireAttribute::Placeholder(value),
        Attribute::Items(items) => AndroidWireAttribute::Items(items),
        Attribute::Range { min, max, step } => AndroidWireAttribute::Range { min, max, step },
        Attribute::AccessibilityLabel(value) => AndroidWireAttribute::AccessibilityLabel(value),
        Attribute::AccessibilityHint(value) => AndroidWireAttribute::AccessibilityHint(value),
        Attribute::AccessibilityRole(role) => {
            AndroidWireAttribute::AccessibilityRole(debug_label(role))
        }
        Attribute::AccessibilityHidden(value) => AndroidWireAttribute::AccessibilityHidden(value),
        Attribute::Direction(value) => AndroidWireAttribute::Direction(debug_label(value)),
        Attribute::FontWeight(value) => AndroidWireAttribute::FontWeight(value),
        Attribute::Italic(value) => AndroidWireAttribute::Italic(value),
        Attribute::TextAlign(value) => AndroidWireAttribute::TextAlign(debug_label(value)),
        Attribute::Transform(transform) => AndroidWireAttribute::Transform {
            translate_x: transform.translate_x,
            translate_y: transform.translate_y,
            scale_x: transform.scale_x,
            scale_y: transform.scale_y,
            rotate: transform.rotate,
        },
        Attribute::Gradient(gradient) => AndroidWireAttribute::Gradient {
            colors_argb: gradient
                .colors
                .into_iter()
                .map(Color::to_argb_u32)
                .collect(),
            start_x: gradient.start.0,
            start_y: gradient.start.1,
            end_x: gradient.end.0,
            end_y: gradient.end.1,
        },
        Attribute::NumberOfLines(value) => AndroidWireAttribute::NumberOfLines(value),
        Attribute::ImageData(bytes) => AndroidWireAttribute::ImageData(bytes.as_ref().clone()),
        Attribute::Horizontal(value) => AndroidWireAttribute::Horizontal(value),
        Attribute::Refreshing(value) => AndroidWireAttribute::Refreshing(value),
        Attribute::ReturnKey(value) => AndroidWireAttribute::ReturnKey(debug_label(value)),
        Attribute::Secure(value) => AndroidWireAttribute::Secure(value),
        Attribute::QrScanning(value) => AndroidWireAttribute::QrScanning(value),
        Attribute::FontFamily(value) => AndroidWireAttribute::FontFamily(value),
        Attribute::KeyboardType(value) => AndroidWireAttribute::KeyboardType(debug_label(value)),
        Attribute::RichText(spans) => {
            AndroidWireAttribute::RichText(spans.into_iter().map(android_wire_text_span).collect())
        }
        Attribute::Url(value) => AndroidWireAttribute::Url(value),
        Attribute::Html(value) => AndroidWireAttribute::Html(value),
        Attribute::TextStyle(value) => AndroidWireAttribute::TextStyle(debug_label(value)),
        Attribute::DatePickerMode(value) => {
            AndroidWireAttribute::DatePickerMode(debug_label(value))
        }
        Attribute::DatePickerStyle(value) => {
            AndroidWireAttribute::DatePickerStyle(debug_label(value))
        }
        Attribute::DateValue(value) => AndroidWireAttribute::DateValue(value),
        Attribute::DateMin(value) => AndroidWireAttribute::DateMin(value),
        Attribute::DateMax(value) => AndroidWireAttribute::DateMax(value),
        Attribute::ItemCount(value) => AndroidWireAttribute::ItemCount(value as u64),
        Attribute::EstimatedItemHeight(value) => AndroidWireAttribute::EstimatedItemHeight(value),
        Attribute::AnimateLayout(value) => AndroidWireAttribute::AnimateLayout(value),
        Attribute::MapCenter {
            latitude,
            longitude,
        } => AndroidWireAttribute::MapCenter {
            latitude,
            longitude,
        },
        Attribute::MapSpan { lat_span, lon_span } => {
            AndroidWireAttribute::MapSpan { lat_span, lon_span }
        }
        Attribute::MapAnnotation {
            annotation_id,
            latitude,
            longitude,
            title,
        } => AndroidWireAttribute::MapAnnotation {
            annotation_id,
            latitude,
            longitude,
            title,
        },
        Attribute::AccessibilitySelected(value) => {
            AndroidWireAttribute::AccessibilitySelected(value)
        }
        Attribute::AccessibilityDisabled(value) => {
            AndroidWireAttribute::AccessibilityDisabled(value)
        }
        Attribute::AccessibilityExpanded(value) => {
            AndroidWireAttribute::AccessibilityExpanded(value)
        }
        Attribute::AccessibilityBusy(value) => AndroidWireAttribute::AccessibilityBusy(value),
        Attribute::HitSlop {
            top,
            right,
            bottom,
            left,
        } => AndroidWireAttribute::HitSlop {
            top,
            right,
            bottom,
            left,
        },
        Attribute::LetterSpacing(value) => AndroidWireAttribute::LetterSpacing(value),
        Attribute::LineHeight(value) => AndroidWireAttribute::LineHeight(value),
        Attribute::TextDecoration(value) => {
            AndroidWireAttribute::TextDecoration(debug_label(value))
        }
        Attribute::TextShadow {
            color,
            offset_x,
            offset_y,
            blur,
        } => AndroidWireAttribute::TextShadow {
            color_argb: color.to_argb_u32(),
            offset_x,
            offset_y,
            blur,
        },
        Attribute::ScrollEnabled(value) => AndroidWireAttribute::ScrollEnabled(value),
        Attribute::ShowsScrollIndicator(value) => AndroidWireAttribute::ShowsScrollIndicator(value),
        Attribute::PagingEnabled(value) => AndroidWireAttribute::PagingEnabled(value),
        Attribute::ContentInset {
            top,
            right,
            bottom,
            left,
        } => AndroidWireAttribute::ContentInset {
            top,
            right,
            bottom,
            left,
        },
        Attribute::OnPressIn(_) => AndroidWireAttribute::EventListener {
            event: "press_in".to_string(),
        },
        Attribute::OnPressOut(_) => AndroidWireAttribute::EventListener {
            event: "press_out".to_string(),
        },
        Attribute::Cursor(value) => AndroidWireAttribute::Cursor(debug_label(value)),
        Attribute::OnSwipe { direction, .. } => AndroidWireAttribute::SwipeListener {
            direction: debug_label(direction),
        },
        Attribute::PlaceholderColor(color) => {
            AndroidWireAttribute::PlaceholderColor(color.to_argb_u32())
        }
        Attribute::InputPrefix(value) => AndroidWireAttribute::InputPrefix(value),
        Attribute::InputSuffix(value) => AndroidWireAttribute::InputSuffix(value),
        Attribute::ClearButton(value) => AndroidWireAttribute::ClearButton(value),
        Attribute::ReadOnly(value) => AndroidWireAttribute::ReadOnly(value),
        Attribute::MaxLength(value) => AndroidWireAttribute::MaxLength(value as u64),
        Attribute::BlurRadius(value) => AndroidWireAttribute::BlurRadius(value),
        Attribute::ClipToBounds(value) => AndroidWireAttribute::ClipToBounds(value),
        Attribute::ZIndex(value) => AndroidWireAttribute::ZIndex(value),
        Attribute::StatusBarStyle(value) => {
            AndroidWireAttribute::StatusBarStyle(debug_label(value))
        }
        Attribute::AspectRatio(value) => AndroidWireAttribute::AspectRatio(value),
        Attribute::FlexOrder(value) => AndroidWireAttribute::FlexOrder(value),
        Attribute::UserSelectText(value) => AndroidWireAttribute::UserSelectText(value),
        Attribute::ParagraphSpacing(value) => AndroidWireAttribute::ParagraphSpacing(value),
        Attribute::FontStyle(value) => AndroidWireAttribute::FontStyle(debug_label(value)),
        Attribute::AccessibilityGroup(value) => AndroidWireAttribute::AccessibilityGroup(value),
        Attribute::AccessibilityHeadingLevel(value) => {
            AndroidWireAttribute::AccessibilityHeadingLevel(value)
        }
        Attribute::AccessibilityActions(value) => AndroidWireAttribute::AccessibilityActions(value),
        Attribute::DynamicType(value) => AndroidWireAttribute::DynamicType(value),
        Attribute::AccessibilityValueString(value) => {
            AndroidWireAttribute::AccessibilityValueString(value)
        }
        Attribute::OnScrollChange(_) => AndroidWireAttribute::EventListener {
            event: "scroll_change".to_string(),
        },
        Attribute::OnScrollBegin(_) => AndroidWireAttribute::EventListener {
            event: "scroll_begin".to_string(),
        },
        Attribute::OnScrollEnd(_) => AndroidWireAttribute::EventListener {
            event: "scroll_end".to_string(),
        },
        Attribute::KeyboardDismissMode(value) => {
            AndroidWireAttribute::KeyboardDismissMode(debug_label(value))
        }
        Attribute::ImageResizeMode(value) => {
            AndroidWireAttribute::ImageResizeMode(debug_label(value))
        }
        Attribute::ImageOnLoad(_) => AndroidWireAttribute::EventListener {
            event: "image_load".to_string(),
        },
        Attribute::ImageOnError(_) => AndroidWireAttribute::EventListener {
            event: "image_error".to_string(),
        },
        Attribute::DrawList(_) => AndroidWireAttribute::Unsupported {
            name: "draw_list".to_string(),
        },
        Attribute::ContextMenu(items) => AndroidWireAttribute::ContextMenu(
            items.into_iter().map(android_wire_menu_item).collect(),
        ),
    }
}

fn android_wire_text_span(span: crate::dom::TextSpan) -> AndroidWireTextSpan {
    AndroidWireTextSpan {
        text: span.text,
        color_argb: span.color.map(Color::to_argb_u32),
        font_size: span.font_size,
        bold: span.bold,
        italic: span.italic,
        underline: span.underline,
        strikethrough: span.strikethrough,
        letter_spacing: span.letter_spacing,
    }
}

fn android_wire_menu_item(item: crate::dom::MenuItem) -> AndroidWireMenuItem {
    AndroidWireMenuItem {
        title: item.title,
        icon: item.icon,
        destructive: item.destructive,
    }
}

fn debug_label(value: impl std::fmt::Debug) -> String {
    format!("{value:?}")
}

/// Android platform-service work requested by app code.
#[derive(Debug, Clone, PartialEq)]
pub enum AndroidPlatformRequest {
    /// Schedule a local notification.
    ScheduleNotification(LocalNotification),
    /// Cancel a local notification.
    CancelNotification {
        /// Notification id.
        id: String,
    },
    /// Show a biometric prompt.
    AuthenticateBiometric {
        /// Prompt reason.
        reason: String,
    },
    /// Start location updates.
    StartLocation,
    /// Stop location updates.
    StopLocation,
    /// Start motion sensor updates.
    StartMotion {
        /// Whether accelerometer updates are requested.
        accelerometer: bool,
        /// Whether gyroscope updates are requested.
        gyroscope: bool,
    },
    /// Stop motion sensor updates.
    StopMotion,
    /// Present the Android media picker.
    PresentMediaPicker {
        /// Maximum selection count.
        max_selection: usize,
    },
    /// Present the Android document picker.
    PresentDocumentPicker {
        /// MIME or platform type filters.
        types: Vec<String>,
    },
    /// Register a background task name.
    RegisterBackgroundTask {
        /// Task identifier.
        identifier: String,
    },
    /// Schedule background work.
    ScheduleBackgroundTask {
        /// Task identifier.
        identifier: String,
        /// Minimum delay before execution.
        earliest_seconds: f64,
    },
    /// Copy text to the clipboard.
    SetClipboard {
        /// Clipboard text.
        text: String,
    },
    /// Present a share sheet.
    ShareText {
        /// Text to share.
        text: String,
    },
    /// Announce a screen-reader message.
    AnnounceAccessibility {
        /// Announcement text.
        message: String,
    },
    /// Move accessibility/input focus.
    RequestFocus {
        /// Target widget id.
        id: u64,
    },
    /// Enable or disable the camera torch.
    SetTorch {
        /// Whether the torch should be on.
        on: bool,
    },
    /// Register for push notifications.
    RegisterForPushNotifications,
    /// Set the app badge count.
    SetAppBadge {
        /// Badge count.
        count: u32,
    },
}

/// A backend that records Android host commands for JNI glue to drain.
pub struct AndroidBackend {
    commands: AndroidCommandQueue,
}

impl Default for AndroidBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl AndroidBackend {
    /// Creates an empty Android backend command queue.
    pub fn new() -> Self {
        AndroidBackend {
            commands: Rc::new(RefCell::new(Vec::new())),
        }
    }

    /// Returns a shared handle to the pending command queue.
    pub fn commands(&self) -> AndroidCommandQueue {
        self.commands.clone()
    }

    /// Drains pending commands from this backend.
    pub fn drain_commands(&self) -> Vec<AndroidCommand> {
        std::mem::take(&mut *self.commands.borrow_mut())
    }

    /// Drains pending commands as one host-facing frame batch.
    pub fn drain_command_batch(&self) -> AndroidCommandBatch {
        AndroidCommandBatch::from_commands(self.drain_commands())
    }

    /// Drains pending commands and encodes the batch as JSON.
    pub fn drain_command_batch_json(&self) -> Result<String, serde_json::Error> {
        self.drain_command_batch().encode_json()
    }
}

impl Backend for AndroidBackend {
    fn apply(&mut self, mutation: Mutation) {
        self.commands
            .borrow_mut()
            .push(AndroidCommand::from_mutation(mutation));
    }
}

/// A running Android app plus its command queue.
pub struct AndroidDriver {
    app: App,
    commands: AndroidCommandQueue,
}

impl AndroidDriver {
    /// Mounts an app using the Android command backend.
    pub fn new<V: View>(viewport: Size, make_view: impl FnOnce() -> V) -> Self {
        let backend = AndroidBackend::new();
        let commands = backend.commands();
        let app = App::new(Host::new(backend), viewport, make_view);
        AndroidDriver { app, commands }
    }

    /// Returns the event sink used by JNI callbacks.
    pub fn event_sink(&self) -> EventSink {
        self.app.event_sink()
    }

    /// Enqueues a native event for delivery on the next tick.
    pub fn dispatch_event(&self, event: Event) {
        self.event_sink().dispatch(event);
    }

    /// Enqueues one decoded host event for delivery on the next tick.
    pub fn dispatch_wire_event(&self, event: AndroidWireEvent) {
        self.dispatch_event(event.into_event());
    }

    /// Decodes one JSON host event and enqueues it for delivery on the next tick.
    pub fn dispatch_wire_event_json(
        &self,
        payload: &str,
    ) -> Result<(), crate::wire::WireProtocolError> {
        self.dispatch_wire_event(AndroidWireEvent::decode_json(payload)?);
        Ok(())
    }

    /// Validates and enqueues a batch of decoded host events in order.
    pub fn dispatch_wire_event_batch(
        &self,
        batch: AndroidWireEventBatch,
    ) -> Result<(), crate::wire::WireProtocolError> {
        for event in batch.into_events()? {
            self.dispatch_event(event);
        }
        Ok(())
    }

    /// Decodes, validates, and enqueues a JSON host event batch in order.
    pub fn dispatch_wire_event_batch_json(
        &self,
        payload: &str,
    ) -> Result<(), crate::wire::WireProtocolError> {
        self.dispatch_wire_event_batch(AndroidWireEventBatch::decode_json(payload)?)
    }

    /// Advances one frame.
    pub fn tick(&mut self) {
        self.app.tick();
    }

    /// Updates the Activity viewport.
    pub fn set_viewport(&mut self, viewport: Size) {
        self.app.set_viewport(viewport);
    }

    /// Drains commands emitted since the previous drain.
    pub fn drain_commands(&self) -> Vec<AndroidCommand> {
        std::mem::take(&mut *self.commands.borrow_mut())
    }

    /// Drains commands emitted since the previous drain as one host-facing frame batch.
    pub fn drain_command_batch(&self) -> AndroidCommandBatch {
        AndroidCommandBatch::from_commands(self.drain_commands())
    }

    /// Drains commands emitted since the previous drain and encodes the batch as JSON.
    pub fn drain_command_batch_json(&self) -> Result<String, serde_json::Error> {
        self.drain_command_batch().encode_json()
    }

    /// Returns mutable access to the underlying app for platform-specific state updates.
    pub fn app_mut(&mut self) -> &mut App {
        &mut self.app
    }
}

impl crate::host::HostDriver for AndroidDriver {
    fn tick(&mut self) {
        AndroidDriver::tick(self);
    }

    fn set_viewport(&mut self, viewport: Size) {
        AndroidDriver::set_viewport(self, viewport);
    }

    fn dispatch_wire_event_batch_json(
        &self,
        payload: &str,
    ) -> Result<(), crate::wire::WireProtocolError> {
        AndroidDriver::dispatch_wire_event_batch_json(self, payload)
    }

    fn drain_command_batch_json(&self) -> Result<String, serde_json::Error> {
        AndroidDriver::drain_command_batch_json(self)
    }
}

/// Mounts an Android host session around a raxon app.
pub fn mount_android_host_session<V: View>(
    viewport: Size,
    make_view: impl FnOnce() -> V,
) -> AndroidHostSession {
    crate::host::HostSession::new(AndroidDriver::new(viewport, make_view))
}

/// Mounts an Android host session into a registry and returns its opaque handle.
pub fn mount_android_host_session_in_registry<V: View>(
    registry: &mut AndroidHostSessionRegistry,
    viewport: Size,
    make_view: impl FnOnce() -> V,
) -> crate::host::HostSessionHandle {
    registry.insert_driver(AndroidDriver::new(viewport, make_view))
}

/// Converts a color into Android's packed `0xAARRGGBB` layout.
pub const fn color_to_argb(color: Color) -> u32 {
    color.to_argb_u32()
}

/// Converts a widget id into the stable integer key used by Android views.
pub fn widget_key(id: WidgetId) -> u64 {
    id.to_u64()
}

/// Creates a zero-sized frame command for tests and host bootstrap code.
pub fn frame_command(id: WidgetId, rect: Rect) -> AndroidCommand {
    AndroidCommand::SetFrame {
        id: widget_key(id),
        x: rect.origin.x,
        y: rect.origin.y,
        width: rect.size.width,
        height: rect.size.height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::{Attribute, Mutation, WidgetKind};
    use crate::reactive::create_signal;
    use crate::view::{button, column, text};

    #[test]
    fn maps_widget_kinds_to_android_classes() {
        assert_eq!(
            AndroidViewClass::from_widget_kind(WidgetKind::Text).class_name(),
            "android.widget.TextView"
        );
        assert_eq!(
            AndroidViewClass::from_widget_kind(WidgetKind::LazyList).class_name(),
            "androidx.recyclerview.widget.RecyclerView"
        );
    }

    #[test]
    fn converts_backdrop_to_android_argb() {
        let command = AndroidCommand::from_mutation(Mutation::SetBackdrop {
            color: Color::rgba(0x11, 0x22, 0x33, 0x44),
        });

        assert_eq!(command, AndroidCommand::SetBackdrop { argb: 0x4411_2233 });
    }

    #[test]
    fn driver_emits_initial_view_commands() {
        let driver = AndroidDriver::new(Size::new(320.0, 480.0), || {
            column((text("Hello"), button("Tap", || {})))
        });
        let commands = driver.drain_commands();

        assert!(commands.iter().any(|command| matches!(
            command,
            AndroidCommand::Create {
                class: AndroidViewClass::TextView,
                ..
            }
        )));
        assert!(commands.iter().any(|command| matches!(
            command,
            AndroidCommand::SetAttribute {
                attr: Attribute::Text(value),
                ..
            } if value == "Hello"
        )));
        assert!(commands
            .iter()
            .any(|command| matches!(command, AndroidCommand::SetRoot { .. })));
    }

    #[test]
    fn driver_drains_host_command_batch() {
        let driver = AndroidDriver::new(Size::new(320.0, 480.0), || {
            text("Hello").font_size(24.0).color(Color::rgb(1, 2, 3))
        });
        let batch = driver.drain_command_batch();

        assert!(!batch.is_empty());
        assert!(batch.commands.iter().any(|command| matches!(
            command,
            AndroidWireCommand::Create { class_name, .. }
                if class_name == "android.widget.TextView"
        )));
        assert!(batch.commands.iter().any(|command| matches!(
            command,
            AndroidWireCommand::SetAttribute {
                attr: AndroidWireAttribute::Text(value),
                ..
            } if value == "Hello"
        )));
        assert!(batch.commands.iter().any(|command| matches!(
            command,
            AndroidWireCommand::SetAttribute {
                attr: AndroidWireAttribute::FontSize(24.0),
                ..
            }
        )));
        assert!(batch.commands.iter().any(|command| matches!(
            command,
            AndroidWireCommand::SetAttribute {
                attr: AndroidWireAttribute::TextColor(0xff01_0203),
                ..
            }
        )));

        let encoded = batch.encode_json().expect("batch encodes as JSON");
        assert!(encoded.contains("android.widget.TextView"));
        assert!(driver.drain_command_batch().is_empty());
    }

    #[test]
    fn driver_dispatches_wire_event_batch_in_order() {
        let tapped = create_signal(0);
        let tapped_for_button = tapped;
        let mut driver = AndroidDriver::new(Size::new(320.0, 480.0), || {
            button("Tap", move || tapped_for_button.update(|count| *count += 1))
        });
        let batch = driver.drain_command_batch();
        let button_id = batch
            .commands
            .iter()
            .find_map(|command| match command {
                AndroidWireCommand::Create { id, class_name }
                    if class_name == "android.widget.Button" =>
                {
                    Some(*id)
                }
                _ => None,
            })
            .expect("button create command is present");
        let events = AndroidWireEventBatch::new(vec![
            AndroidWireEvent::Tap { target: button_id },
            AndroidWireEvent::Tap { target: button_id },
        ]);
        let encoded = events.encode_json().expect("event batch encodes");

        driver
            .dispatch_wire_event_batch_json(&encoded)
            .expect("event batch dispatches");
        driver.tick();

        assert_eq!(tapped.get(), 2);
    }

    #[test]
    fn host_session_dispatches_events_ticks_and_drains_commands() {
        let count = create_signal(0);
        let text_count = count;
        let button_count = count;
        let mut session = mount_android_host_session(Size::new(320.0, 480.0), || {
            column((
                text(move || format!("Count {}", text_count.get())),
                button("Tap", move || button_count.update(|value| *value += 1)),
            ))
        });
        let batch = session.driver().drain_command_batch();
        let button_id = batch
            .commands
            .iter()
            .find_map(|command| match command {
                AndroidWireCommand::Create { id, class_name }
                    if class_name == "android.widget.Button" =>
                {
                    Some(*id)
                }
                _ => None,
            })
            .expect("button create command is present");
        let events = AndroidWireEventBatch::new(vec![AndroidWireEvent::Tap { target: button_id }]);
        let encoded = events.encode_json().expect("event batch encodes");

        let commands = session
            .dispatch_events_tick_and_drain_command_batch_json(&encoded)
            .expect("session round-trip succeeds");
        let commands_json: serde_json::Value =
            serde_json::from_str(&commands).expect("commands are valid JSON");
        let commands = commands_json["commands"]
            .as_array()
            .expect("commands is an array");

        assert!(commands.iter().any(|command| {
            command["type"].as_str() == Some("set_attribute")
                && command["attr"]["name"].as_str() == Some("text")
                && command["attr"]["value"].as_str() == Some("Count 1")
        }));
    }

    #[test]
    fn host_session_registry_routes_opaque_handles() {
        let count = create_signal(0);
        let text_count = count;
        let button_count = count;
        let mut registry = AndroidHostSessionRegistry::new();
        let handle =
            mount_android_host_session_in_registry(&mut registry, Size::new(320.0, 480.0), || {
                column((
                    text(move || format!("Count {}", text_count.get())),
                    button("Tap", move || button_count.update(|value| *value += 1)),
                ))
            });
        assert!(registry.contains(handle));

        let initial = registry
            .get(handle)
            .expect("session exists")
            .driver()
            .drain_command_batch();
        let button_id = initial
            .commands
            .iter()
            .find_map(|command| match command {
                AndroidWireCommand::Create { id, class_name }
                    if class_name == "android.widget.Button" =>
                {
                    Some(*id)
                }
                _ => None,
            })
            .expect("button create command is present");
        let encoded = AndroidWireEventBatch::new(vec![AndroidWireEvent::Tap { target: button_id }])
            .encode_json()
            .expect("event batch encodes");

        let commands = registry
            .dispatch_events_tick_and_drain_command_batch_json(handle, &encoded)
            .expect("registry dispatches into session");

        assert!(commands.contains("\"Count 1\""));
        assert!(registry.remove(handle).is_some());
        assert_eq!(
            registry.tick(handle),
            Err(crate::host::HostSessionError::UnknownSession {
                handle: handle.to_raw(),
            })
        );
    }
}
