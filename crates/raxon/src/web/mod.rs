//! Web backend foundation.
//!
//! This module provides the pure-Rust half of the WebAssembly DOM backend: a
//! command queue that translates raxon [`Mutation`](crate::dom::Mutation)s into
//! DOM operations. A small wasm host can drain these commands, apply them to the
//! browser DOM, and dispatch browser events back through
//! [`EventSink`](crate::dom::EventSink).

#![forbid(unsafe_code)]

use std::cell::RefCell;
use std::rc::Rc;

use crate::core::{Color, Rect, Size};
use crate::dom::{
    Attribute, Backend, Event, EventSink, GestureKind, HapticStyle, Host, LocalNotification,
    Mutation, PermissionKind, WidgetId, WidgetKind,
};
use crate::runtime::App;
use crate::view::View;

/// A shared queue of DOM commands produced by [`WebDomBackend`].
pub type DomCommandQueue = Rc<RefCell<Vec<DomCommand>>>;

/// Web host session used by generated browser glue.
pub type WebHostSession = crate::host::HostSession<WebDriver>;

/// Web host-session registry keyed by opaque browser-safe handles.
pub type WebHostSessionRegistry = crate::host::HostSessionRegistry<WebDriver>;

/// Web binding runtime used by generated browser glue.
pub type WebHostBridge = crate::host::HostBridge<WebDriver>;

/// Host-originated web event payload for JavaScript adapters.
pub type DomWireEvent = crate::wire::WireEvent;

/// Batch of host-originated web events for JavaScript adapters.
pub type DomWireEventBatch = crate::wire::WireEventBatch;

/// DOM element kinds used by the first web backend pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomElementKind {
    /// A generic `div` container.
    Div,
    /// Inline text content.
    Span,
    /// A native `button`.
    Button,
    /// An `img` element.
    Image,
    /// An `input type=checkbox`.
    Checkbox,
    /// An `input type=range`.
    Range,
    /// An `input type=text`.
    TextInput,
    /// A scrollable `div`.
    ScrollDiv,
    /// An indeterminate progress indicator.
    ActivityIndicator,
    /// A `progress` element.
    Progress,
    /// A segmented-control host.
    Segmented,
    /// A numeric input used as a stepper.
    Stepper,
    /// Date or datetime input.
    DateInput,
    /// A `textarea`.
    TextArea,
    /// A camera preview video host.
    Video,
    /// An `iframe`.
    Iframe,
    /// A virtualized list host.
    VirtualList,
    /// A map host.
    MapHost,
    /// A `canvas`.
    Canvas,
}

impl DomElementKind {
    /// Maps a framework widget kind to the DOM element used by the web backend.
    pub const fn from_widget_kind(kind: WidgetKind) -> Self {
        match kind {
            WidgetKind::View | WidgetKind::Stack => DomElementKind::Div,
            WidgetKind::Text => DomElementKind::Span,
            WidgetKind::Button => DomElementKind::Button,
            WidgetKind::Image => DomElementKind::Image,
            WidgetKind::Switch => DomElementKind::Checkbox,
            WidgetKind::Slider => DomElementKind::Range,
            WidgetKind::TextInput => DomElementKind::TextInput,
            WidgetKind::Scroll => DomElementKind::ScrollDiv,
            WidgetKind::ActivityIndicator => DomElementKind::ActivityIndicator,
            WidgetKind::Progress => DomElementKind::Progress,
            WidgetKind::Segmented => DomElementKind::Segmented,
            WidgetKind::Stepper => DomElementKind::Stepper,
            WidgetKind::DatePicker => DomElementKind::DateInput,
            WidgetKind::TextArea => DomElementKind::TextArea,
            WidgetKind::Camera => DomElementKind::Video,
            WidgetKind::WebView => DomElementKind::Iframe,
            WidgetKind::LazyList => DomElementKind::VirtualList,
            WidgetKind::MapView => DomElementKind::MapHost,
            WidgetKind::Canvas => DomElementKind::Canvas,
        }
    }

    /// HTML tag name to create for this element kind.
    pub const fn tag_name(self) -> &'static str {
        match self {
            DomElementKind::Div
            | DomElementKind::ScrollDiv
            | DomElementKind::ActivityIndicator
            | DomElementKind::Segmented
            | DomElementKind::VirtualList
            | DomElementKind::MapHost => "div",
            DomElementKind::Span => "span",
            DomElementKind::Button => "button",
            DomElementKind::Image => "img",
            DomElementKind::Checkbox
            | DomElementKind::Range
            | DomElementKind::TextInput
            | DomElementKind::Stepper
            | DomElementKind::DateInput => "input",
            DomElementKind::Progress => "progress",
            DomElementKind::TextArea => "textarea",
            DomElementKind::Video => "video",
            DomElementKind::Iframe => "iframe",
            DomElementKind::Canvas => "canvas",
        }
    }

    /// Input type for element kinds represented by `<input>`.
    pub const fn input_type(self) -> Option<&'static str> {
        match self {
            DomElementKind::Checkbox => Some("checkbox"),
            DomElementKind::Range => Some("range"),
            DomElementKind::TextInput => Some("text"),
            DomElementKind::Stepper => Some("number"),
            DomElementKind::DateInput => Some("datetime-local"),
            _ => None,
        }
    }
}

/// A command for the wasm host layer to apply to the browser DOM.
#[derive(Debug, Clone, PartialEq)]
pub enum DomCommand {
    /// Create a DOM element.
    Create {
        /// Stable widget id.
        id: u64,
        /// Element kind.
        kind: DomElementKind,
    },
    /// Set a platform-neutral attribute on an element.
    SetAttribute {
        /// Stable widget id.
        id: u64,
        /// Attribute payload.
        attr: Attribute,
    },
    /// Apply absolute layout coordinates as CSS pixels.
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
    /// Insert a child into a parent element.
    InsertChild {
        /// Parent widget id.
        parent: u64,
        /// Child widget id.
        child: u64,
        /// Child index.
        index: usize,
    },
    /// Remove a child from a parent element.
    RemoveChild {
        /// Parent widget id.
        parent: u64,
        /// Child widget id.
        child: u64,
    },
    /// Remove an element from the DOM and free its host state.
    Destroy {
        /// Stable widget id.
        id: u64,
    },
    /// Attach the root element to the web mount node.
    SetRoot {
        /// Root widget id.
        id: u64,
    },
    /// Register a DOM event listener for a gesture.
    AddGesture {
        /// Stable widget id.
        id: u64,
        /// Gesture kind.
        gesture: GestureKind,
    },
    /// Set scrollable content dimensions.
    SetContentSize {
        /// Scroll widget id.
        id: u64,
        /// Content width.
        width: f32,
        /// Content height.
        height: f32,
    },
    /// Set document/body backdrop color.
    SetBackdrop {
        /// CSS color string.
        css_color: String,
    },
    /// Request web haptic feedback when available.
    Haptic {
        /// Haptic style.
        style: HapticStyle,
    },
    /// Invoke a browser/platform service.
    Request(WebPlatformRequest),
    /// Scroll an element to an explicit offset.
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
    /// Scroll an element to the top-left origin.
    ScrollToTop {
        /// Scroll widget id.
        id: u64,
        /// Whether the scroll should animate.
        animated: bool,
    },
}

impl DomCommand {
    /// Translates a framework mutation into a DOM host command.
    pub fn from_mutation(mutation: Mutation) -> Self {
        match mutation {
            Mutation::Create { id, kind } => DomCommand::Create {
                id: widget_key(id),
                kind: DomElementKind::from_widget_kind(kind),
            },
            Mutation::SetAttribute { id, attr } => DomCommand::SetAttribute {
                id: widget_key(id),
                attr,
            },
            Mutation::SetFrame { id, rect } => DomCommand::SetFrame {
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
            } => DomCommand::InsertChild {
                parent: widget_key(parent),
                child: widget_key(child),
                index,
            },
            Mutation::RemoveChild { parent, child } => DomCommand::RemoveChild {
                parent: widget_key(parent),
                child: widget_key(child),
            },
            Mutation::Destroy { id } => DomCommand::Destroy { id: widget_key(id) },
            Mutation::SetRoot { id } => DomCommand::SetRoot { id: widget_key(id) },
            Mutation::AddGesture { id, gesture } => DomCommand::AddGesture {
                id: widget_key(id),
                gesture,
            },
            Mutation::SetContentSize { id, size } => DomCommand::SetContentSize {
                id: widget_key(id),
                width: size.width,
                height: size.height,
            },
            Mutation::SetBackdrop { color } => DomCommand::SetBackdrop {
                css_color: color_to_css(color),
            },
            Mutation::Haptic { style } => DomCommand::Haptic { style },
            Mutation::ScheduleNotification(notification) => {
                DomCommand::Request(WebPlatformRequest::ScheduleNotification(notification))
            }
            Mutation::CancelNotification { id } => {
                DomCommand::Request(WebPlatformRequest::CancelNotification { id })
            }
            Mutation::AuthenticateBiometric { reason } => {
                DomCommand::Request(WebPlatformRequest::AuthenticateBiometric { reason })
            }
            Mutation::CheckPermission { permission } => {
                DomCommand::Request(WebPlatformRequest::CheckPermission { permission })
            }
            Mutation::RequestPermission { permission } => {
                DomCommand::Request(WebPlatformRequest::RequestPermission { permission })
            }
            Mutation::StartLocation | Mutation::RequestLocation => {
                DomCommand::Request(WebPlatformRequest::StartLocation)
            }
            Mutation::StopLocation | Mutation::StopLocationUpdates => {
                DomCommand::Request(WebPlatformRequest::StopLocation)
            }
            Mutation::StartMotion {
                accelerometer,
                gyroscope,
            } => DomCommand::Request(WebPlatformRequest::StartMotion {
                accelerometer,
                gyroscope,
            }),
            Mutation::StopMotion => DomCommand::Request(WebPlatformRequest::StopMotion),
            Mutation::PresentMediaPicker { max_selection } => {
                DomCommand::Request(WebPlatformRequest::PresentMediaPicker { max_selection })
            }
            Mutation::PresentDocumentPicker { types } => {
                DomCommand::Request(WebPlatformRequest::PresentDocumentPicker { types })
            }
            Mutation::RegisterBackgroundTask { identifier } => {
                DomCommand::Request(WebPlatformRequest::RegisterBackgroundTask { identifier })
            }
            Mutation::ScheduleBackgroundTask {
                identifier,
                earliest_seconds,
            } => DomCommand::Request(WebPlatformRequest::ScheduleBackgroundTask {
                identifier,
                earliest_seconds,
            }),
            Mutation::SetClipboard { text } => {
                DomCommand::Request(WebPlatformRequest::SetClipboard { text })
            }
            Mutation::ShareText { text } => {
                DomCommand::Request(WebPlatformRequest::ShareText { text })
            }
            Mutation::OpenExternalUrl { url } => {
                DomCommand::Request(WebPlatformRequest::OpenExternalUrl { url })
            }
            Mutation::AnnounceAccessibility { message } => {
                DomCommand::Request(WebPlatformRequest::AnnounceAccessibility { message })
            }
            Mutation::RequestFocus { id } => {
                DomCommand::Request(WebPlatformRequest::RequestFocus { id: widget_key(id) })
            }
            Mutation::SetTorch { on } => DomCommand::Request(WebPlatformRequest::SetTorch { on }),
            Mutation::RegisterForPushNotifications => {
                DomCommand::Request(WebPlatformRequest::RegisterForPushNotifications)
            }
            Mutation::SetAppBadge { count } => {
                DomCommand::Request(WebPlatformRequest::SetAppBadge { count })
            }
            Mutation::ScrollTo {
                id,
                offset_x,
                offset_y,
                animated,
            } => DomCommand::ScrollTo {
                id: widget_key(id),
                offset_x,
                offset_y,
                animated,
            },
            Mutation::ScrollToTop { id, animated } => DomCommand::ScrollToTop {
                id: widget_key(id),
                animated,
            },
        }
    }
}

/// A single drained web frame encoded for wasm or JavaScript host adapters.
#[allow(missing_docs)]
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomCommandBatch {
    pub commands: Vec<DomWireCommand>,
}

impl DomCommandBatch {
    /// Converts DOM backend commands into the host-facing wire representation.
    pub fn from_commands(commands: Vec<DomCommand>) -> Self {
        DomCommandBatch {
            commands: commands.into_iter().map(DomWireCommand::from).collect(),
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

    /// Encodes the whole frame as JSON for a wasm boundary that wants one payload.
    pub fn encode_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

/// DOM command payload with only host-serializable values.
#[allow(missing_docs)]
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DomWireCommand {
    Create {
        id: u64,
        tag_name: String,
        input_type: Option<String>,
        /// Whether this node is a scroll container (needs `overflow: auto`).
        #[serde(default)]
        scrollable: bool,
    },
    SetAttribute {
        id: u64,
        attr: DomWireAttribute,
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
        css_color: String,
    },
    Haptic {
        style: String,
    },
    Request {
        request: DomWirePlatformRequest,
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

impl From<DomCommand> for DomWireCommand {
    fn from(command: DomCommand) -> Self {
        match command {
            DomCommand::Create { id, kind } => DomWireCommand::Create {
                id,
                tag_name: kind.tag_name().to_string(),
                input_type: kind.input_type().map(str::to_string),
                scrollable: matches!(kind, DomElementKind::ScrollDiv),
            },
            DomCommand::SetAttribute { id, attr } => DomWireCommand::SetAttribute {
                id,
                attr: dom_wire_attribute(attr),
            },
            DomCommand::SetFrame {
                id,
                x,
                y,
                width,
                height,
            } => DomWireCommand::SetFrame {
                id,
                x,
                y,
                width,
                height,
            },
            DomCommand::InsertChild {
                parent,
                child,
                index,
            } => DomWireCommand::InsertChild {
                parent,
                child,
                index: index as u64,
            },
            DomCommand::RemoveChild { parent, child } => {
                DomWireCommand::RemoveChild { parent, child }
            }
            DomCommand::Destroy { id } => DomWireCommand::Destroy { id },
            DomCommand::SetRoot { id } => DomWireCommand::SetRoot { id },
            DomCommand::AddGesture { id, gesture } => DomWireCommand::AddGesture {
                id,
                gesture: debug_label(gesture),
            },
            DomCommand::SetContentSize { id, width, height } => {
                DomWireCommand::SetContentSize { id, width, height }
            }
            DomCommand::SetBackdrop { css_color } => DomWireCommand::SetBackdrop { css_color },
            DomCommand::Haptic { style } => DomWireCommand::Haptic {
                style: debug_label(style),
            },
            DomCommand::Request(request) => DomWireCommand::Request {
                request: DomWirePlatformRequest::from(request),
            },
            DomCommand::ScrollTo {
                id,
                offset_x,
                offset_y,
                animated,
            } => DomWireCommand::ScrollTo {
                id,
                offset_x,
                offset_y,
                animated,
            },
            DomCommand::ScrollToTop { id, animated } => {
                DomWireCommand::ScrollToTop { id, animated }
            }
        }
    }
}

/// Web platform request payload with primitive/string host values.
#[allow(missing_docs)]
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DomWirePlatformRequest {
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
    CheckPermission {
        permission: PermissionKind,
    },
    RequestPermission {
        permission: PermissionKind,
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
    OpenExternalUrl {
        url: String,
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

impl From<WebPlatformRequest> for DomWirePlatformRequest {
    fn from(request: WebPlatformRequest) -> Self {
        match request {
            WebPlatformRequest::ScheduleNotification(notification) => {
                DomWirePlatformRequest::ScheduleNotification {
                    id: notification.id,
                    title: notification.title,
                    body: notification.body,
                    delay_seconds: notification.delay_seconds,
                }
            }
            WebPlatformRequest::CancelNotification { id } => {
                DomWirePlatformRequest::CancelNotification { id }
            }
            WebPlatformRequest::AuthenticateBiometric { reason } => {
                DomWirePlatformRequest::AuthenticateBiometric { reason }
            }
            WebPlatformRequest::CheckPermission { permission } => {
                DomWirePlatformRequest::CheckPermission { permission }
            }
            WebPlatformRequest::RequestPermission { permission } => {
                DomWirePlatformRequest::RequestPermission { permission }
            }
            WebPlatformRequest::StartLocation => DomWirePlatformRequest::StartLocation,
            WebPlatformRequest::StopLocation => DomWirePlatformRequest::StopLocation,
            WebPlatformRequest::StartMotion {
                accelerometer,
                gyroscope,
            } => DomWirePlatformRequest::StartMotion {
                accelerometer,
                gyroscope,
            },
            WebPlatformRequest::StopMotion => DomWirePlatformRequest::StopMotion,
            WebPlatformRequest::PresentMediaPicker { max_selection } => {
                DomWirePlatformRequest::PresentMediaPicker {
                    max_selection: max_selection as u64,
                }
            }
            WebPlatformRequest::PresentDocumentPicker { types } => {
                DomWirePlatformRequest::PresentDocumentPicker { types }
            }
            WebPlatformRequest::RegisterBackgroundTask { identifier } => {
                DomWirePlatformRequest::RegisterBackgroundTask { identifier }
            }
            WebPlatformRequest::ScheduleBackgroundTask {
                identifier,
                earliest_seconds,
            } => DomWirePlatformRequest::ScheduleBackgroundTask {
                identifier,
                earliest_seconds,
            },
            WebPlatformRequest::SetClipboard { text } => {
                DomWirePlatformRequest::SetClipboard { text }
            }
            WebPlatformRequest::ShareText { text } => DomWirePlatformRequest::ShareText { text },
            WebPlatformRequest::OpenExternalUrl { url } => {
                DomWirePlatformRequest::OpenExternalUrl { url }
            }
            WebPlatformRequest::AnnounceAccessibility { message } => {
                DomWirePlatformRequest::AnnounceAccessibility { message }
            }
            WebPlatformRequest::RequestFocus { id } => DomWirePlatformRequest::RequestFocus { id },
            WebPlatformRequest::SetTorch { on } => DomWirePlatformRequest::SetTorch { on },
            WebPlatformRequest::RegisterForPushNotifications => {
                DomWirePlatformRequest::RegisterForPushNotifications
            }
            WebPlatformRequest::SetAppBadge { count } => {
                DomWirePlatformRequest::SetAppBadge { count }
            }
        }
    }
}

/// DOM attribute payload with callbacks replaced by listener intent markers.
#[allow(missing_docs)]
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(tag = "name", content = "value", rename_all = "snake_case")]
pub enum DomWireAttribute {
    Text(String),
    FontSize(f32),
    TextColor(String),
    BackgroundColor(String),
    CornerRadius(f32),
    Opacity(f32),
    BorderWidth(f32),
    BorderColor(String),
    Shadow {
        color: String,
        radius: f32,
        dx: f32,
        dy: f32,
    },
    ImageSource(String),
    ImageData(Vec<u8>),
    BoolValue(bool),
    FloatValue(f32),
    TintColor(String),
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
        colors: Vec<String>,
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
    RichText(Vec<DomWireTextSpan>),
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
        color: String,
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
    PlaceholderColor(String),
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
    ContextMenu(Vec<DomWireMenuItem>),
    DrawList(Vec<DomWireDrawCmd>),
    Unsupported {
        name: String,
    },
}

/// A stroke (outline) for a [`DomWireDrawCmd`]: width + CSS color.
#[allow(missing_docs)]
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomWireStroke {
    pub width: f32,
    pub color: String,
}

/// One vector drawing command for a canvas widget, with colors as CSS strings
/// so the JS host can paint it directly onto a `<canvas>` 2D context.
#[allow(missing_docs)]
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DomWireDrawCmd {
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        width: f32,
        color: String,
    },
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        fill: Option<String>,
        stroke: Option<DomWireStroke>,
    },
    Circle {
        cx: f32,
        cy: f32,
        r: f32,
        fill: Option<String>,
        stroke: Option<DomWireStroke>,
    },
    Path {
        points: Vec<[f32; 2]>,
        closed: bool,
        fill: Option<String>,
        stroke: Option<DomWireStroke>,
    },
    Text {
        x: f32,
        y: f32,
        text: String,
        size: f32,
        color: String,
        align: String,
    },
}

/// DOM host data for a rich-text span.
#[allow(missing_docs)]
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomWireTextSpan {
    pub text: String,
    pub color: Option<String>,
    pub font_size: Option<f32>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub letter_spacing: Option<f32>,
}

/// DOM host data for a context-menu row without the Rust action closure.
#[allow(missing_docs)]
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomWireMenuItem {
    pub title: String,
    pub icon: Option<String>,
    pub destructive: bool,
}

fn dom_wire_attribute(attr: Attribute) -> DomWireAttribute {
    match attr {
        Attribute::Text(value) => DomWireAttribute::Text(value),
        Attribute::FontSize(value) => DomWireAttribute::FontSize(value),
        Attribute::TextColor(color) => DomWireAttribute::TextColor(color_to_css(color)),
        Attribute::BackgroundColor(color) => DomWireAttribute::BackgroundColor(color_to_css(color)),
        Attribute::CornerRadius(value) => DomWireAttribute::CornerRadius(value),
        Attribute::Opacity(value) => DomWireAttribute::Opacity(value),
        Attribute::BorderWidth(value) => DomWireAttribute::BorderWidth(value),
        Attribute::BorderColor(color) => DomWireAttribute::BorderColor(color_to_css(color)),
        Attribute::Shadow(shadow) => DomWireAttribute::Shadow {
            color: color_to_css(shadow.color),
            radius: shadow.radius,
            dx: shadow.dx,
            dy: shadow.dy,
        },
        Attribute::ImageSource(value) => DomWireAttribute::ImageSource(value),
        Attribute::BoolValue(value) => DomWireAttribute::BoolValue(value),
        Attribute::FloatValue(value) => DomWireAttribute::FloatValue(value),
        Attribute::TintColor(color) => DomWireAttribute::TintColor(color_to_css(color)),
        Attribute::Placeholder(value) => DomWireAttribute::Placeholder(value),
        Attribute::Items(items) => DomWireAttribute::Items(items),
        Attribute::Range { min, max, step } => DomWireAttribute::Range { min, max, step },
        Attribute::AccessibilityLabel(value) => DomWireAttribute::AccessibilityLabel(value),
        Attribute::AccessibilityHint(value) => DomWireAttribute::AccessibilityHint(value),
        Attribute::AccessibilityRole(role) => {
            DomWireAttribute::AccessibilityRole(debug_label(role))
        }
        Attribute::AccessibilityHidden(value) => DomWireAttribute::AccessibilityHidden(value),
        Attribute::Direction(value) => DomWireAttribute::Direction(debug_label(value)),
        Attribute::FontWeight(value) => DomWireAttribute::FontWeight(value),
        Attribute::Italic(value) => DomWireAttribute::Italic(value),
        Attribute::TextAlign(value) => DomWireAttribute::TextAlign(debug_label(value)),
        Attribute::Transform(transform) => DomWireAttribute::Transform {
            translate_x: transform.translate_x,
            translate_y: transform.translate_y,
            scale_x: transform.scale_x,
            scale_y: transform.scale_y,
            rotate: transform.rotate,
        },
        Attribute::Gradient(gradient) => DomWireAttribute::Gradient {
            colors: gradient.colors.into_iter().map(color_to_css).collect(),
            start_x: gradient.start.0,
            start_y: gradient.start.1,
            end_x: gradient.end.0,
            end_y: gradient.end.1,
        },
        Attribute::NumberOfLines(value) => DomWireAttribute::NumberOfLines(value),
        Attribute::ImageData(bytes) => DomWireAttribute::ImageData(bytes.as_ref().clone()),
        Attribute::Horizontal(value) => DomWireAttribute::Horizontal(value),
        Attribute::Refreshing(value) => DomWireAttribute::Refreshing(value),
        Attribute::ReturnKey(value) => DomWireAttribute::ReturnKey(debug_label(value)),
        Attribute::Secure(value) => DomWireAttribute::Secure(value),
        Attribute::QrScanning(value) => DomWireAttribute::QrScanning(value),
        Attribute::FontFamily(value) => DomWireAttribute::FontFamily(value),
        Attribute::KeyboardType(value) => DomWireAttribute::KeyboardType(debug_label(value)),
        Attribute::RichText(spans) => {
            DomWireAttribute::RichText(spans.into_iter().map(dom_wire_text_span).collect())
        }
        Attribute::Url(value) => DomWireAttribute::Url(value),
        Attribute::Html(value) => DomWireAttribute::Html(value),
        Attribute::TextStyle(value) => DomWireAttribute::TextStyle(debug_label(value)),
        Attribute::DatePickerMode(value) => DomWireAttribute::DatePickerMode(debug_label(value)),
        Attribute::DatePickerStyle(value) => DomWireAttribute::DatePickerStyle(debug_label(value)),
        Attribute::DateValue(value) => DomWireAttribute::DateValue(value),
        Attribute::DateMin(value) => DomWireAttribute::DateMin(value),
        Attribute::DateMax(value) => DomWireAttribute::DateMax(value),
        Attribute::ItemCount(value) => DomWireAttribute::ItemCount(value as u64),
        Attribute::EstimatedItemHeight(value) => DomWireAttribute::EstimatedItemHeight(value),
        Attribute::AnimateLayout(value) => DomWireAttribute::AnimateLayout(value),
        Attribute::MapCenter {
            latitude,
            longitude,
        } => DomWireAttribute::MapCenter {
            latitude,
            longitude,
        },
        Attribute::MapSpan { lat_span, lon_span } => {
            DomWireAttribute::MapSpan { lat_span, lon_span }
        }
        Attribute::MapAnnotation {
            annotation_id,
            latitude,
            longitude,
            title,
        } => DomWireAttribute::MapAnnotation {
            annotation_id,
            latitude,
            longitude,
            title,
        },
        Attribute::AccessibilitySelected(value) => DomWireAttribute::AccessibilitySelected(value),
        Attribute::AccessibilityDisabled(value) => DomWireAttribute::AccessibilityDisabled(value),
        Attribute::AccessibilityExpanded(value) => DomWireAttribute::AccessibilityExpanded(value),
        Attribute::AccessibilityBusy(value) => DomWireAttribute::AccessibilityBusy(value),
        Attribute::HitSlop {
            top,
            right,
            bottom,
            left,
        } => DomWireAttribute::HitSlop {
            top,
            right,
            bottom,
            left,
        },
        Attribute::LetterSpacing(value) => DomWireAttribute::LetterSpacing(value),
        Attribute::LineHeight(value) => DomWireAttribute::LineHeight(value),
        Attribute::TextDecoration(value) => DomWireAttribute::TextDecoration(debug_label(value)),
        Attribute::TextShadow {
            color,
            offset_x,
            offset_y,
            blur,
        } => DomWireAttribute::TextShadow {
            color: color_to_css(color),
            offset_x,
            offset_y,
            blur,
        },
        Attribute::ScrollEnabled(value) => DomWireAttribute::ScrollEnabled(value),
        Attribute::ShowsScrollIndicator(value) => DomWireAttribute::ShowsScrollIndicator(value),
        Attribute::PagingEnabled(value) => DomWireAttribute::PagingEnabled(value),
        Attribute::ContentInset {
            top,
            right,
            bottom,
            left,
        } => DomWireAttribute::ContentInset {
            top,
            right,
            bottom,
            left,
        },
        Attribute::OnPressIn(_) => DomWireAttribute::EventListener {
            event: "press_in".to_string(),
        },
        Attribute::OnPressOut(_) => DomWireAttribute::EventListener {
            event: "press_out".to_string(),
        },
        Attribute::Cursor(value) => DomWireAttribute::Cursor(debug_label(value)),
        Attribute::OnSwipe { direction, .. } => DomWireAttribute::SwipeListener {
            direction: debug_label(direction),
        },
        Attribute::PlaceholderColor(color) => {
            DomWireAttribute::PlaceholderColor(color_to_css(color))
        }
        Attribute::InputPrefix(value) => DomWireAttribute::InputPrefix(value),
        Attribute::InputSuffix(value) => DomWireAttribute::InputSuffix(value),
        Attribute::ClearButton(value) => DomWireAttribute::ClearButton(value),
        Attribute::ReadOnly(value) => DomWireAttribute::ReadOnly(value),
        Attribute::MaxLength(value) => DomWireAttribute::MaxLength(value as u64),
        Attribute::BlurRadius(value) => DomWireAttribute::BlurRadius(value),
        Attribute::ClipToBounds(value) => DomWireAttribute::ClipToBounds(value),
        Attribute::ZIndex(value) => DomWireAttribute::ZIndex(value),
        Attribute::StatusBarStyle(value) => DomWireAttribute::StatusBarStyle(debug_label(value)),
        Attribute::AspectRatio(value) => DomWireAttribute::AspectRatio(value),
        Attribute::FlexOrder(value) => DomWireAttribute::FlexOrder(value),
        Attribute::UserSelectText(value) => DomWireAttribute::UserSelectText(value),
        Attribute::ParagraphSpacing(value) => DomWireAttribute::ParagraphSpacing(value),
        Attribute::FontStyle(value) => DomWireAttribute::FontStyle(debug_label(value)),
        Attribute::AccessibilityGroup(value) => DomWireAttribute::AccessibilityGroup(value),
        Attribute::AccessibilityHeadingLevel(value) => {
            DomWireAttribute::AccessibilityHeadingLevel(value)
        }
        Attribute::AccessibilityActions(value) => DomWireAttribute::AccessibilityActions(value),
        Attribute::DynamicType(value) => DomWireAttribute::DynamicType(value),
        Attribute::AccessibilityValueString(value) => {
            DomWireAttribute::AccessibilityValueString(value)
        }
        Attribute::OnScrollChange(_) => DomWireAttribute::EventListener {
            event: "scroll_change".to_string(),
        },
        Attribute::OnScrollBegin(_) => DomWireAttribute::EventListener {
            event: "scroll_begin".to_string(),
        },
        Attribute::OnScrollEnd(_) => DomWireAttribute::EventListener {
            event: "scroll_end".to_string(),
        },
        Attribute::KeyboardDismissMode(value) => {
            DomWireAttribute::KeyboardDismissMode(debug_label(value))
        }
        Attribute::ImageResizeMode(value) => DomWireAttribute::ImageResizeMode(debug_label(value)),
        Attribute::ImageOnLoad(_) => DomWireAttribute::EventListener {
            event: "image_load".to_string(),
        },
        Attribute::ImageOnError(_) => DomWireAttribute::EventListener {
            event: "image_error".to_string(),
        },
        Attribute::DrawList(cmds) => {
            DomWireAttribute::DrawList(cmds.into_iter().map(dom_wire_draw_cmd).collect())
        }
        Attribute::ContextMenu(items) => {
            DomWireAttribute::ContextMenu(items.into_iter().map(dom_wire_menu_item).collect())
        }
    }
}

fn dom_wire_text_span(span: crate::dom::TextSpan) -> DomWireTextSpan {
    DomWireTextSpan {
        text: span.text,
        color: span.color.map(color_to_css),
        font_size: span.font_size,
        bold: span.bold,
        italic: span.italic,
        underline: span.underline,
        strikethrough: span.strikethrough,
        letter_spacing: span.letter_spacing,
    }
}

fn dom_wire_menu_item(item: crate::dom::MenuItem) -> DomWireMenuItem {
    DomWireMenuItem {
        title: item.title,
        icon: item.icon,
        destructive: item.destructive,
    }
}

fn dom_wire_stroke(stroke: crate::dom::Stroke) -> DomWireStroke {
    DomWireStroke {
        width: stroke.width,
        color: color_to_css(stroke.color),
    }
}

fn dom_wire_draw_cmd(cmd: crate::dom::DrawCmd) -> DomWireDrawCmd {
    use crate::dom::DrawCmd;
    match cmd {
        DrawCmd::Line {
            x1,
            y1,
            x2,
            y2,
            width,
            color,
        } => DomWireDrawCmd::Line {
            x1,
            y1,
            x2,
            y2,
            width,
            color: color_to_css(color),
        },
        DrawCmd::Rect {
            x,
            y,
            w,
            h,
            radius,
            fill,
            stroke,
        } => DomWireDrawCmd::Rect {
            x,
            y,
            w,
            h,
            radius,
            fill: fill.map(color_to_css),
            stroke: stroke.map(dom_wire_stroke),
        },
        DrawCmd::Circle {
            cx,
            cy,
            r,
            fill,
            stroke,
        } => DomWireDrawCmd::Circle {
            cx,
            cy,
            r,
            fill: fill.map(color_to_css),
            stroke: stroke.map(dom_wire_stroke),
        },
        DrawCmd::Path {
            points,
            closed,
            fill,
            stroke,
        } => DomWireDrawCmd::Path {
            points: points.into_iter().map(|(x, y)| [x, y]).collect(),
            closed,
            fill: fill.map(color_to_css),
            stroke: stroke.map(dom_wire_stroke),
        },
        DrawCmd::Text {
            x,
            y,
            text,
            size,
            color,
            align,
        } => DomWireDrawCmd::Text {
            x,
            y,
            text,
            size,
            color: color_to_css(color),
            align: text_align_str(align).to_string(),
        },
    }
}

fn text_align_str(align: crate::dom::TextAlign) -> &'static str {
    match align {
        crate::dom::TextAlign::Start => "left",
        crate::dom::TextAlign::Center => "center",
        crate::dom::TextAlign::End => "right",
    }
}

fn debug_label(value: impl std::fmt::Debug) -> String {
    format!("{value:?}")
}

/// Browser/platform-service work requested by app code.
#[derive(Debug, Clone, PartialEq)]
pub enum WebPlatformRequest {
    /// Schedule a web notification.
    ScheduleNotification(LocalNotification),
    /// Cancel a web notification.
    CancelNotification {
        /// Notification id.
        id: String,
    },
    /// Request WebAuthn or platform authentication.
    AuthenticateBiometric {
        /// Prompt reason.
        reason: String,
    },
    /// Check a browser/platform permission without prompting.
    CheckPermission {
        /// Permission to check.
        permission: PermissionKind,
    },
    /// Request a browser/platform permission.
    RequestPermission {
        /// Permission to request.
        permission: PermissionKind,
    },
    /// Start geolocation updates.
    StartLocation,
    /// Stop geolocation updates.
    StopLocation,
    /// Start browser motion sensor updates.
    StartMotion {
        /// Whether accelerometer updates are requested.
        accelerometer: bool,
        /// Whether gyroscope updates are requested.
        gyroscope: bool,
    },
    /// Stop motion sensor updates.
    StopMotion,
    /// Present a media picker.
    PresentMediaPicker {
        /// Maximum selection count.
        max_selection: usize,
    },
    /// Present a file picker.
    PresentDocumentPicker {
        /// Accepted MIME/type filters.
        types: Vec<String>,
    },
    /// Register background work.
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
    /// Invoke the Web Share API.
    ShareText {
        /// Text to share.
        text: String,
    },
    /// Open a URL in a browser tab/window.
    OpenExternalUrl {
        /// URL to open.
        url: String,
    },
    /// Announce a screen-reader message.
    AnnounceAccessibility {
        /// Announcement text.
        message: String,
    },
    /// Move focus to an element.
    RequestFocus {
        /// Target widget id.
        id: u64,
    },
    /// Enable or disable torch where media APIs expose it.
    SetTorch {
        /// Whether the torch should be on.
        on: bool,
    },
    /// Register for push notifications.
    RegisterForPushNotifications,
    /// Set a badge count where the Badging API exists.
    SetAppBadge {
        /// Badge count.
        count: u32,
    },
}

/// A backend that records DOM commands for a wasm host to drain.
pub struct WebDomBackend {
    commands: DomCommandQueue,
}

impl Default for WebDomBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl WebDomBackend {
    /// Creates an empty DOM backend command queue.
    pub fn new() -> Self {
        WebDomBackend {
            commands: Rc::new(RefCell::new(Vec::new())),
        }
    }

    /// Returns a shared handle to the pending command queue.
    pub fn commands(&self) -> DomCommandQueue {
        self.commands.clone()
    }

    /// Drains pending commands from this backend.
    pub fn drain_commands(&self) -> Vec<DomCommand> {
        std::mem::take(&mut *self.commands.borrow_mut())
    }

    /// Drains pending commands as one host-facing frame batch.
    pub fn drain_command_batch(&self) -> DomCommandBatch {
        DomCommandBatch::from_commands(self.drain_commands())
    }

    /// Drains pending commands and encodes the batch as JSON.
    pub fn drain_command_batch_json(&self) -> Result<String, serde_json::Error> {
        self.drain_command_batch().encode_json()
    }
}

impl Backend for WebDomBackend {
    fn apply(&mut self, mutation: Mutation) {
        self.commands
            .borrow_mut()
            .push(DomCommand::from_mutation(mutation));
    }
}

/// A running web app plus its DOM command queue.
pub struct WebDriver {
    app: App,
    commands: DomCommandQueue,
}

impl WebDriver {
    /// Mounts an app using the DOM command backend.
    pub fn new<V: View>(viewport: Size, make_view: impl FnOnce() -> V) -> Self {
        // Route HTTP through the JS host's `fetch` so `get`/`post`/`use_query`
        // work out of the box on the web. Apps may still override with
        // `net::set_client`.
        crate::net::install_web_http_client();
        let backend = WebDomBackend::new();
        let commands = backend.commands();
        let app = App::new(Host::new(backend), viewport, make_view);
        WebDriver { app, commands }
    }

    /// Returns the event sink used by browser callbacks.
    pub fn event_sink(&self) -> EventSink {
        self.app.event_sink()
    }

    /// Enqueues a browser event for delivery on the next tick.
    pub fn dispatch_event(&self, event: Event) {
        self.event_sink().dispatch(event);
    }

    /// Enqueues one decoded host event for delivery on the next tick.
    pub fn dispatch_wire_event(&self, event: DomWireEvent) {
        self.dispatch_event(event.into_event());
    }

    /// Decodes one JSON host event and enqueues it for delivery on the next tick.
    pub fn dispatch_wire_event_json(
        &self,
        payload: &str,
    ) -> Result<(), crate::wire::WireProtocolError> {
        self.dispatch_wire_event(DomWireEvent::decode_json(payload)?);
        Ok(())
    }

    /// Validates and enqueues a batch of decoded host events in order.
    pub fn dispatch_wire_event_batch(
        &self,
        batch: DomWireEventBatch,
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
        self.dispatch_wire_event_batch(DomWireEventBatch::decode_json(payload)?)
    }

    /// Advances one frame.
    pub fn tick(&mut self) {
        self.app.tick();
    }

    /// Updates the browser viewport.
    pub fn set_viewport(&mut self, viewport: Size) {
        self.app.set_viewport(viewport);
    }

    /// Drains commands emitted since the previous drain.
    pub fn drain_commands(&self) -> Vec<DomCommand> {
        std::mem::take(&mut *self.commands.borrow_mut())
    }

    /// Drains commands emitted since the previous drain as one host-facing frame batch.
    pub fn drain_command_batch(&self) -> DomCommandBatch {
        DomCommandBatch::from_commands(self.drain_commands())
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

impl crate::host::HostDriver for WebDriver {
    fn tick(&mut self) {
        WebDriver::tick(self);
    }

    fn set_viewport(&mut self, viewport: Size) {
        WebDriver::set_viewport(self, viewport);
    }

    fn dispatch_wire_event_batch_json(
        &self,
        payload: &str,
    ) -> Result<(), crate::wire::WireProtocolError> {
        WebDriver::dispatch_wire_event_batch_json(self, payload)
    }

    fn drain_command_batch_json(&self) -> Result<String, serde_json::Error> {
        WebDriver::drain_command_batch_json(self)
    }
}

/// Mounts a web host session around a raxon app.
pub fn mount_web_host_session<V: View>(
    viewport: Size,
    make_view: impl FnOnce() -> V,
) -> WebHostSession {
    crate::host::HostSession::new(WebDriver::new(viewport, make_view))
}

/// Mounts a web host session into a registry and returns its opaque handle.
pub fn mount_web_host_session_in_registry<V: View>(
    registry: &mut WebHostSessionRegistry,
    viewport: Size,
    make_view: impl FnOnce() -> V,
) -> crate::host::HostSessionHandle {
    registry.insert_driver(WebDriver::new(viewport, make_view))
}

impl crate::host::HostBridge<WebDriver> {
    /// Mounts a web app into this binding runtime.
    pub fn mount_web<V: View>(
        &mut self,
        viewport: Size,
        make_view: impl FnOnce() -> V,
    ) -> crate::host::HostSessionHandle {
        self.insert_driver(WebDriver::new(viewport, make_view))
    }
}

/// Converts a color into an `rgba(r, g, b, a)` CSS string.
pub fn color_to_css(color: Color) -> String {
    let alpha = color.a as f32 / 255.0;
    format!("rgba({}, {}, {}, {:.3})", color.r, color.g, color.b, alpha)
}

/// Converts a widget id into the stable integer key used by DOM nodes.
pub fn widget_key(id: WidgetId) -> u64 {
    id.to_u64()
}

/// Creates a frame command for tests and host bootstrap code.
pub fn frame_command(id: WidgetId, rect: Rect) -> DomCommand {
    DomCommand::SetFrame {
        id: widget_key(id),
        x: rect.origin.x,
        y: rect.origin.y,
        width: rect.size.width,
        height: rect.size.height,
    }
}

// ---------------------------------------------------------------------------
// Browser History API — URL-synced client-side routing.
//
// raxon's `nav` handles native push/pop; on the web a router also wants the
// address bar to reflect the route (shareable URLs, back/forward, reload). These
// thin helpers wrap `window.history` / `window.location`. They are no-ops off
// the web so a router module compiles unchanged on every target.
// ---------------------------------------------------------------------------

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
mod browser_history {
    use std::cell::RefCell;

    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::{JsCast, JsValue};

    /// The current URL path (e.g. `/console/dashboard`).
    pub fn location_path() -> String {
        web_sys::window()
            .and_then(|w| w.location().pathname().ok())
            .unwrap_or_else(|| "/".to_string())
    }

    /// The current URL search string without the leading `?`.
    pub fn location_search() -> String {
        web_sys::window()
            .and_then(|w| w.location().search().ok())
            .map(|search| search.trim_start_matches('?').to_string())
            .unwrap_or_default()
    }

    /// The current URL hash string without the leading `#`.
    pub fn location_hash() -> String {
        web_sys::window()
            .and_then(|w| w.location().hash().ok())
            .map(|hash| hash.trim_start_matches('#').to_string())
            .unwrap_or_default()
    }

    /// The current route string: `pathname + search + hash`.
    pub fn location_route() -> String {
        let Some(window) = web_sys::window() else {
            return "/".to_string();
        };
        let location = window.location();
        let mut route = location.pathname().unwrap_or_else(|_| "/".to_string());
        if route.is_empty() {
            route.push('/');
        }
        if let Ok(search) = location.search() {
            route.push_str(&search);
        }
        if let Ok(hash) = location.hash() {
            route.push_str(&hash);
        }
        route
    }

    /// Pushes a new history entry with `path` (adds a back-stack entry).
    pub fn push_path(path: &str) {
        if let Some(history) = web_sys::window().and_then(|w| w.history().ok()) {
            let _ = history.push_state_with_url(&JsValue::NULL, "", Some(path));
        }
    }

    /// Replaces the current history entry with `path` (no back-stack entry).
    pub fn replace_path(path: &str) {
        if let Some(history) = web_sys::window().and_then(|w| w.history().ok()) {
            let _ = history.replace_state_with_url(&JsValue::NULL, "", Some(path));
        }
    }

    thread_local! {
        // Keeps popstate closures alive for the page lifetime.
        static POPSTATE_HOOKS: RefCell<Vec<Closure<dyn FnMut()>>> = const { RefCell::new(Vec::new()) };
    }

    /// Invokes `callback(route)` on browser back/forward navigation.
    pub fn on_popstate(callback: impl Fn(String) + 'static) {
        let closure = Closure::<dyn FnMut()>::new(move || callback(location_route()));
        if let Some(window) = web_sys::window() {
            let _ = window
                .add_event_listener_with_callback("popstate", closure.as_ref().unchecked_ref());
        }
        POPSTATE_HOOKS.with(|hooks| hooks.borrow_mut().push(closure));
    }
}

/// The current URL path. Returns `"/"` off the web.
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
pub use browser_history::{
    location_hash, location_path, location_route, location_search, on_popstate, push_path,
    replace_path,
};

/// The current URL path. Returns `"/"` off the web.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub fn location_path() -> String {
    "/".to_string()
}

/// The current URL search string without the leading `?`. Returns `""` off the web.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub fn location_search() -> String {
    String::new()
}

/// The current URL hash string without the leading `#`. Returns `""` off the web.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub fn location_hash() -> String {
    String::new()
}

/// The current route string (`pathname + search + hash`). Returns `"/"` off the web.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub fn location_route() -> String {
    "/".to_string()
}

/// Pushes a history entry (no-op off the web).
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub fn push_path(_path: &str) {}

/// Replaces the current history entry (no-op off the web).
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub fn replace_path(_path: &str) {}

/// Registers a back/forward handler (no-op off the web).
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub fn on_popstate(_callback: impl Fn(String) + 'static) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::{Attribute, Mutation, WidgetKind};
    use crate::reactive::create_signal;
    use crate::view::{button, column, text};

    #[test]
    fn maps_widget_kinds_to_dom_elements() {
        let slider = DomElementKind::from_widget_kind(WidgetKind::Slider);
        assert_eq!(slider.tag_name(), "input");
        assert_eq!(slider.input_type(), Some("range"));

        let text = DomElementKind::from_widget_kind(WidgetKind::Text);
        assert_eq!(text.tag_name(), "span");
        assert_eq!(text.input_type(), None);
    }

    #[test]
    fn converts_backdrop_to_css_color() {
        let command = DomCommand::from_mutation(Mutation::SetBackdrop {
            color: Color::rgba(10, 20, 30, 128),
        });

        assert_eq!(
            command,
            DomCommand::SetBackdrop {
                css_color: "rgba(10, 20, 30, 0.502)".to_string()
            }
        );
    }

    #[test]
    fn converts_external_url_to_web_platform_request() {
        let command = DomCommand::from_mutation(Mutation::OpenExternalUrl {
            url: "https://example.com".to_string(),
        });

        assert_eq!(
            command,
            DomCommand::Request(WebPlatformRequest::OpenExternalUrl {
                url: "https://example.com".to_string()
            })
        );
        assert_eq!(
            DomWirePlatformRequest::from(WebPlatformRequest::OpenExternalUrl {
                url: "https://example.com".to_string()
            }),
            DomWirePlatformRequest::OpenExternalUrl {
                url: "https://example.com".to_string()
            }
        );
    }

    #[test]
    fn driver_emits_initial_dom_commands() {
        let driver = WebDriver::new(Size::new(320.0, 480.0), || {
            column((text("Hello"), button("Tap", || {})))
        });
        let commands = driver.drain_commands();

        assert!(commands.iter().any(|command| matches!(
            command,
            DomCommand::Create {
                kind: DomElementKind::Span,
                ..
            }
        )));
        assert!(commands.iter().any(|command| matches!(
            command,
            DomCommand::SetAttribute {
                attr: Attribute::Text(value),
                ..
            } if value == "Hello"
        )));
        assert!(commands
            .iter()
            .any(|command| matches!(command, DomCommand::SetRoot { .. })));
    }

    #[test]
    fn driver_drains_host_command_batch() {
        let driver = WebDriver::new(Size::new(320.0, 480.0), || {
            text("Hello").font_size(24.0).color(Color::rgb(1, 2, 3))
        });
        let batch = driver.drain_command_batch();

        assert!(!batch.is_empty());
        assert!(batch.commands.iter().any(|command| matches!(
            command,
            DomWireCommand::Create { tag_name, .. } if tag_name == "span"
        )));
        assert!(batch.commands.iter().any(|command| matches!(
            command,
            DomWireCommand::SetAttribute {
                attr: DomWireAttribute::Text(value),
                ..
            } if value == "Hello"
        )));
        assert!(batch.commands.iter().any(|command| matches!(
            command,
            DomWireCommand::SetAttribute {
                attr: DomWireAttribute::FontSize(24.0),
                ..
            }
        )));
        assert!(batch.commands.iter().any(|command| matches!(
            command,
            DomWireCommand::SetAttribute {
                attr: DomWireAttribute::TextColor(value),
                ..
            } if value == "rgba(1, 2, 3, 1.000)"
        )));

        let encoded = batch.encode_json().expect("batch encodes as JSON");
        assert!(encoded.contains("\"tag_name\":\"span\""));
        assert!(driver.drain_command_batch().is_empty());
    }

    #[test]
    fn driver_dispatches_wire_event_batch_in_order() {
        let tapped = create_signal(0);
        let tapped_for_button = tapped;
        let mut driver = WebDriver::new(Size::new(320.0, 480.0), || {
            button("Tap", move || tapped_for_button.update(|count| *count += 1))
        });
        let batch = driver.drain_command_batch();
        let button_id = batch
            .commands
            .iter()
            .find_map(|command| match command {
                DomWireCommand::Create { id, tag_name, .. } if tag_name == "button" => Some(*id),
                _ => None,
            })
            .expect("button create command is present");
        let events = DomWireEventBatch::new(vec![
            DomWireEvent::Tap { target: button_id },
            DomWireEvent::Tap { target: button_id },
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
        let mut session = mount_web_host_session(Size::new(320.0, 480.0), || {
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
                DomWireCommand::Create { id, tag_name, .. } if tag_name == "button" => Some(*id),
                _ => None,
            })
            .expect("button create command is present");
        let events = DomWireEventBatch::new(vec![DomWireEvent::Tap { target: button_id }]);
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
        let mut registry = WebHostSessionRegistry::new();
        let handle =
            mount_web_host_session_in_registry(&mut registry, Size::new(320.0, 480.0), || {
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
                DomWireCommand::Create { id, tag_name, .. } if tag_name == "button" => Some(*id),
                _ => None,
            })
            .expect("button create command is present");
        let encoded = DomWireEventBatch::new(vec![DomWireEvent::Tap { target: button_id }])
            .encode_json()
            .expect("event batch encodes");
        let event_batch = DomWireEventBatch::decode_json(&encoded).expect("event batch decodes");
        let request = crate::host::HostBridgeRequest::DispatchEventsTickAndDrainCommandBatch {
            handle: handle.to_raw(),
            batch: event_batch,
        };
        let response = registry
            .handle_request_json(
                &serde_json::to_string(&crate::host::HostBridgeJsonRequest::new(request))
                    .expect("request encodes"),
            )
            .expect("registry dispatches into session");
        let response: crate::host::HostBridgeJsonResponse =
            serde_json::from_str(&response).expect("response decodes");
        match response.response {
            crate::host::HostBridgeResponse::CommandBatch { batch } => {
                assert!(batch.to_string().contains("\"Count 1\""));
            }
            _ => panic!("expected command batch response"),
        }

        let destroy = crate::host::HostBridgeRequest::Destroy {
            handle: handle.to_raw(),
        };
        let response = registry
            .handle_request_json(
                &serde_json::to_string(&crate::host::HostBridgeJsonRequest::new(destroy))
                    .expect("request encodes"),
            )
            .expect("destroy request succeeds");
        assert_eq!(
            serde_json::from_str::<crate::host::HostBridgeJsonResponse>(&response)
                .expect("response decodes"),
            crate::host::HostBridgeJsonResponse::new(crate::host::HostBridgeResponse::Destroyed {
                handle: handle.to_raw(),
            })
        );
        assert_eq!(
            registry.tick(handle),
            Err(crate::host::HostSessionError::UnknownSession {
                handle: handle.to_raw(),
            })
        );
    }

    #[test]
    fn host_bridge_mounts_and_returns_json_replies() {
        let count = create_signal(0);
        let text_count = count;
        let button_count = count;
        let mut bridge = WebHostBridge::new();
        let handle = bridge.mount_web(Size::new(320.0, 480.0), || {
            column((
                text(move || format!("Count {}", text_count.get())),
                button("Tap", move || button_count.update(|value| *value += 1)),
            ))
        });
        assert!(bridge.contains(handle));

        let initial = bridge
            .registry()
            .get(handle)
            .expect("session exists")
            .driver()
            .drain_command_batch();
        let button_id = initial
            .commands
            .iter()
            .find_map(|command| match command {
                DomWireCommand::Create { id, tag_name, .. } if tag_name == "button" => Some(*id),
                _ => None,
            })
            .expect("button create command is present");
        let event_batch = DomWireEventBatch::new(vec![DomWireEvent::Tap { target: button_id }]);
        let request = crate::host::HostBridgeJsonRequest::new(
            crate::host::HostBridgeRequest::DispatchEventsTickAndDrainCommandBatch {
                handle: handle.to_raw(),
                batch: event_batch,
            },
        );
        let reply = bridge
            .handle_request_json_reply(&serde_json::to_string(&request).expect("request encodes"));
        let reply: crate::host::HostBridgeJsonReply =
            serde_json::from_str(&reply).expect("reply decodes");
        match reply.result {
            crate::host::HostBridgeJsonReplyResult::Ok {
                response: crate::host::HostBridgeResponse::CommandBatch { batch },
            } => {
                assert!(batch.to_string().contains("\"Count 1\""));
            }
            _ => panic!("expected ok command batch reply"),
        }

        let reply = bridge.handle_request_json_reply(
            &serde_json::to_string(&crate::host::HostBridgeJsonRequest::new(
                crate::host::HostBridgeRequest::DrainCommandBatch { handle: 999 },
            ))
            .expect("request encodes"),
        );
        match serde_json::from_str::<crate::host::HostBridgeJsonReply>(&reply)
            .expect("reply decodes")
            .result
        {
            crate::host::HostBridgeJsonReplyResult::Error { error } => {
                assert_eq!(
                    error.code,
                    crate::host::HostBridgeJsonErrorCode::UnknownSession
                );
                assert_eq!(error.handle, Some(999));
            }
            _ => panic!("expected error reply"),
        }
    }
}
