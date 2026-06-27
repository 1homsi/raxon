//! The retained element tree.
//!
//! Each node owns: its kind, parent/child links, and the reactive effects that
//! bind its attributes. When a node is removed we dispose its effects (so they
//! stop emitting) and tear down the subtree depth-first. This is also where the
//! reactive-runtime "ownership" gap noted in `rax-reactive` is closed for the UI:
//! effect lifetime is tied to element lifetime.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::{channel, Receiver, Sender};

use crate::core::{Arena, LayoutStyle};
use crate::reactive::{create_effect, Effect};

use super::backend::Host;
use super::event::{Event, EventKind, EventSink};
use super::mutation::{Attribute, Mutation, WidgetId, WidgetKind};

/// Default font size (logical points) used for text measurement until set.
const DEFAULT_FONT_SIZE: f32 = 16.0;

/// A one-shot builder that materializes one view subtree into the tree and
/// returns its root. Produced by dynamic views (a higher layer) and stored,
/// type-erased, so `rax-dom` need not know about the view layer.
pub type BuildThunk = Box<dyn FnOnce(&mut Tree) -> WidgetId>;

/// A registered event handler and the kind of event it responds to.
struct Handler {
    kind: EventKind,
    callback: Box<dyn FnMut(&Event)>,
}

struct ElementNode {
    kind: WidgetKind,
    parent: Option<WidgetId>,
    children: Vec<WidgetId>,
    /// Retained layout inputs, consumed by the layout pass.
    style: LayoutStyle,
    /// Font size, captured from paint attributes, used for text measurement.
    font_size: f32,
    /// Reactive bindings owned by this node, disposed when it is removed.
    effects: Vec<Effect>,
    /// Event handlers, dropped when the node is removed.
    handlers: Vec<Handler>,
    /// For dynamic nodes: the next subtree to build, set by the tracking effect
    /// and consumed by [`Tree::run_dynamic`].
    pending: Option<Rc<RefCell<Option<BuildThunk>>>>,
    /// Latest text content (static or reactive), shared so the layout pass can
    /// read it for intrinsic-width measurement without a tree borrow.
    measure_text: Rc<RefCell<Option<String>>>,
}

/// The retained element tree, paired with the backend it emits mutations to.
pub struct Tree {
    host: Host,
    nodes: Arena<ElementNode>,
    root: Option<WidgetId>,
    /// Handlers for app-global (untargeted) events.
    global_handlers: Vec<Handler>,
    /// Inbound event queue, filled by [`EventSink`]s and drained on the UI thread.
    event_tx: Sender<Event>,
    event_rx: Receiver<Event>,
    /// Dynamic nodes whose subtree needs rebuilding. Effects push here (no tree
    /// borrow); [`run_dynamic`](Tree::run_dynamic) drains it with `&mut Tree`.
    dirty: Rc<RefCell<Vec<WidgetId>>>,
}

impl Tree {
    /// Creates an empty tree that emits mutations through `host`.
    pub fn new(host: Host) -> Self {
        let (event_tx, event_rx) = channel();
        Tree {
            host,
            nodes: Arena::new(),
            root: None,
            global_handlers: Vec::new(),
            event_tx,
            event_rx,
            dirty: Rc::new(RefCell::new(Vec::new())),
        }
    }

    /// The current root widget, if one has been set.
    pub fn root(&self) -> Option<WidgetId> {
        self.root
    }

    /// Marks `id` as the tree root and tells the backend to attach it to the
    /// platform's content view.
    pub fn set_root(&mut self, id: WidgetId) {
        self.root = Some(id);
        if self.nodes.get(id.0).is_some() {
            self.host.emit(Mutation::SetRoot { id });
        }
    }

    /// Creates a layout container view.
    pub fn create_view(&mut self) -> WidgetId {
        self.create(WidgetKind::View)
    }

    /// Creates a text widget.
    pub fn create_text(&mut self) -> WidgetId {
        self.create(WidgetKind::Text)
    }

    /// Creates a tappable button widget.
    pub fn create_button(&mut self) -> WidgetId {
        self.create(WidgetKind::Button)
    }

    /// Creates an image view.
    pub fn create_image(&mut self) -> WidgetId {
        self.create(WidgetKind::Image)
    }

    /// Creates an on/off switch.
    pub fn create_switch(&mut self) -> WidgetId {
        self.create(WidgetKind::Switch)
    }

    /// Creates a value slider.
    pub fn create_slider(&mut self) -> WidgetId {
        self.create(WidgetKind::Slider)
    }

    /// Creates a single-line editable text field.
    pub fn create_text_input(&mut self) -> WidgetId {
        self.create(WidgetKind::TextInput)
    }

    /// Creates a scroll container.
    pub fn create_scroll(&mut self) -> WidgetId {
        self.create(WidgetKind::Scroll)
    }

    /// Creates a spinning activity indicator.
    pub fn create_activity_indicator(&mut self) -> WidgetId {
        self.create(WidgetKind::ActivityIndicator)
    }

    /// Creates a determinate progress bar.
    pub fn create_progress(&mut self) -> WidgetId {
        self.create(WidgetKind::Progress)
    }

    /// Creates a horizontal segmented control.
    pub fn create_segmented(&mut self) -> WidgetId {
        self.create(WidgetKind::Segmented)
    }

    /// Creates a -/+ stepper.
    pub fn create_stepper(&mut self) -> WidgetId {
        self.create(WidgetKind::Stepper)
    }

    /// Creates a native date/time picker.
    pub fn create_date_picker(&mut self) -> WidgetId {
        self.create(WidgetKind::DatePicker)
    }

    /// Creates a multi-line editable text area.
    pub fn create_text_area(&mut self) -> WidgetId {
        self.create(WidgetKind::TextArea)
    }

    /// Creates an absolute-position overlay container (ZStack).
    pub fn create_stack(&mut self) -> WidgetId {
        self.create(WidgetKind::Stack)
    }

    /// Creates a camera preview widget (backed by `AVCaptureSession` on iOS).
    pub fn create_camera(&mut self) -> WidgetId {
        self.create(WidgetKind::Camera)
    }

    /// Creates an embedded web view (WKWebView on iOS).
    pub fn create_web_view(&mut self) -> WidgetId {
        self.create(WidgetKind::WebView)
    }

    /// Creates a virtualized lazy list (UITableView on iOS).
    pub fn create_lazy_list(&mut self) -> WidgetId {
        self.create(WidgetKind::LazyList)
    }

    /// Creates a native map view (MKMapView on iOS).
    pub fn create_map_view(&mut self) -> WidgetId {
        self.create(WidgetKind::MapView)
    }

    /// Creates a vector drawing canvas (a `CALayer`-backed `UIView` on iOS).
    pub fn create_canvas(&mut self) -> WidgetId {
        self.create(WidgetKind::Canvas)
    }

    /// Starts CoreLocation GPS updates. Results arrive as global `Event::LocationUpdated`.
    pub fn start_location(&mut self) {
        self.host.emit(Mutation::StartLocation);
    }

    /// Stops CoreLocation GPS updates.
    pub fn stop_location(&mut self) {
        self.host.emit(Mutation::StopLocation);
    }

    /// Starts CoreMotion sensor updates. Results arrive as global `Event::MotionUpdated`.
    pub fn start_motion(&mut self, accelerometer: bool, gyroscope: bool) {
        self.host.emit(Mutation::StartMotion {
            accelerometer,
            gyroscope,
        });
    }

    /// Stops CoreMotion sensor updates.
    pub fn stop_motion(&mut self) {
        self.host.emit(Mutation::StopMotion);
    }

    /// Emits a content-size update for a scroll container.
    pub fn set_content_size(&mut self, id: WidgetId, size: crate::core::Size) {
        if self.nodes.get(id.0).is_some() {
            self.host.emit(Mutation::SetContentSize { id, size });
        }
    }

    /// Emits a backdrop-color update (the fill behind the root / safe area).
    pub fn set_backdrop(&mut self, color: crate::core::Color) {
        self.host.emit(Mutation::SetBackdrop { color });
    }

    /// Triggers a one-shot haptic feedback pulse via the backend.
    pub fn haptic(&mut self, style: super::mutation::HapticStyle) {
        self.host.emit(Mutation::Haptic { style });
    }

    /// Schedules a local notification via the UserNotifications framework.
    pub fn schedule_notification(&mut self, notif: super::mutation::LocalNotification) {
        self.host.emit(Mutation::ScheduleNotification(notif));
    }

    /// Cancels a pending local notification by its identifier.
    pub fn cancel_notification(&mut self, id: String) {
        self.host.emit(Mutation::CancelNotification { id });
    }

    /// Triggers a biometric authentication prompt (Face ID / Touch ID).
    /// The result is delivered as a global [`Event::BiometricResult`].
    pub fn authenticate_biometric(&mut self, reason: String) {
        self.host.emit(Mutation::AuthenticateBiometric { reason });
    }

    /// Queries the current authorization state for a platform permission.
    pub fn check_permission(&mut self, permission: super::mutation::PermissionKind) {
        self.host.emit(Mutation::CheckPermission { permission });
    }

    /// Requests a platform permission from the user.
    pub fn request_permission(&mut self, permission: super::mutation::PermissionKind) {
        self.host.emit(Mutation::RequestPermission { permission });
    }

    /// Presents the system media picker (PHPickerViewController on iOS).
    /// Results arrive as global [`Event::MediaPicked`] or [`Event::MediaPickerCancelled`].
    pub fn present_media_picker(&mut self, max_selection: usize) {
        self.host
            .emit(Mutation::PresentMediaPicker { max_selection });
    }

    /// Presents the system document picker (UIDocumentPickerViewController on
    /// iOS). `types` are UTType identifiers (empty = any file). Results arrive
    /// as a global [`Event::DocumentPicked`].
    pub fn present_document_picker(&mut self, types: Vec<String>) {
        self.host.emit(Mutation::PresentDocumentPicker { types });
    }

    /// Registers a background task identifier with BGTaskScheduler.
    /// Must be called during app launch before the first background task fires.
    pub fn register_background_task(&mut self, identifier: String) {
        self.host
            .emit(Mutation::RegisterBackgroundTask { identifier });
    }

    /// Schedules the next execution of a registered background task.
    pub fn schedule_background_task(&mut self, identifier: String, earliest_seconds: f64) {
        self.host.emit(Mutation::ScheduleBackgroundTask {
            identifier,
            earliest_seconds,
        });
    }

    /// Copies text to the system clipboard.
    pub fn set_clipboard(&mut self, text: String) {
        self.host.emit(Mutation::SetClipboard { text });
    }

    /// Presents the system share sheet with the given text.
    pub fn share_text(&mut self, text: String) {
        self.host.emit(Mutation::ShareText { text });
    }

    /// Opens a URL with the platform's default external handler.
    pub fn open_external_url(&mut self, url: String) {
        self.host.emit(Mutation::OpenExternalUrl { url });
    }

    /// Posts an accessibility announcement that VoiceOver reads immediately.
    ///
    /// Maps to `UIAccessibilityPostNotification(UIAccessibilityAnnouncementNotification, message)`
    /// on iOS (notification value 1008).
    pub fn announce_accessibility(&mut self, message: String) {
        self.host.emit(Mutation::AnnounceAccessibility { message });
    }

    /// Moves VoiceOver focus to the native view backing `id`.
    ///
    /// Maps to `UIAccessibilityPostNotification(UIAccessibilityScreenChangedNotification, view)`
    /// on iOS (notification value 1000).
    pub fn request_focus(&mut self, id: WidgetId) {
        self.host.emit(Mutation::RequestFocus { id });
    }

    /// Requests GPS location updates. Results arrive as global `Event::LocationUpdated`.
    /// iOS: calls `[CLLocationManager startUpdatingLocation]`.
    pub fn request_location(&mut self) {
        self.host.emit(Mutation::RequestLocation);
    }

    /// Stops GPS location updates.
    pub fn stop_location_updates(&mut self) {
        self.host.emit(Mutation::StopLocationUpdates);
    }

    /// Enables or disables the device torch (flashlight).
    /// iOS: sets `AVCaptureDevice` torch mode.
    pub fn set_torch(&mut self, on: bool) {
        self.host.emit(Mutation::SetTorch { on });
    }

    /// Registers for Apple Push Notification Service (APNS) remote notifications.
    /// iOS: calls `[UIApplication registerForRemoteNotifications]`.
    pub fn register_for_push(&mut self) {
        self.host.emit(Mutation::RegisterForPushNotifications);
    }

    /// Sets the app badge count on the home screen icon.
    /// iOS: calls `[[UIApplication sharedApplication] setApplicationIconBadgeNumber: count]`.
    pub fn set_app_badge(&mut self, count: u32) {
        self.host.emit(Mutation::SetAppBadge { count });
    }

    /// Programmatically scrolls a scroll view to the given content offset.
    ///
    /// iOS: calls `[UIScrollView setContentOffset:animated:]`.
    pub fn scroll_to(&mut self, id: WidgetId, offset_x: f32, offset_y: f32, animated: bool) {
        if self.nodes.get(id.0).is_some() {
            self.host.emit(Mutation::ScrollTo {
                id,
                offset_x,
                offset_y,
                animated,
            });
        }
    }

    /// Programmatically scrolls a scroll view back to the top (offset 0, 0).
    ///
    /// iOS: calls `[UIScrollView setContentOffset:{0,0} animated:]`.
    pub fn scroll_to_top(&mut self, id: WidgetId, animated: bool) {
        if self.nodes.get(id.0).is_some() {
            self.host.emit(Mutation::ScrollToTop { id, animated });
        }
    }

    fn create(&mut self, kind: WidgetKind) -> WidgetId {
        let index = self.nodes.insert(ElementNode {
            kind,
            parent: None,
            children: Vec::new(),
            style: LayoutStyle::default(),
            font_size: DEFAULT_FONT_SIZE,
            effects: Vec::new(),
            handlers: Vec::new(),
            pending: None,
            measure_text: Rc::new(RefCell::new(None)),
        });
        let id = WidgetId(index);
        self.host.emit(Mutation::Create { id, kind });
        id
    }

    /// Creates a **dynamic** container whose children are (re)built by `selector`
    /// whenever a signal it reads changes.
    ///
    /// `selector` is run inside a tracking effect; each run produces a
    /// [`BuildThunk`] for the new subtree and schedules a rebuild. The rebuild
    /// itself happens later in [`run_dynamic`](Tree::run_dynamic) with exclusive
    /// access — so the effect never re-enters the tree, sidestepping borrow
    /// conflicts entirely.
    pub fn create_dynamic(
        &mut self,
        mut selector: impl FnMut() -> BuildThunk + 'static,
    ) -> WidgetId {
        let id = self.create_view();
        let pending = Rc::new(RefCell::new(None));
        let pending_writer = pending.clone();
        let dirty = self.dirty.clone();
        let effect = create_effect(move || {
            let thunk = selector();
            *pending_writer.borrow_mut() = Some(thunk);
            dirty.borrow_mut().push(id);
        });
        if let Some(node) = self.nodes.get_mut(id.0) {
            node.effects.push(effect);
            node.pending = Some(pending);
        }
        id
    }

    /// Applies all pending dynamic rebuilds, looping until none remain (a freshly
    /// built subtree may itself contain dynamic nodes). Called once per frame by
    /// the runtime, outside any event handler.
    pub fn run_dynamic(&mut self) {
        // Bounded to guard against pathological rebuild cycles.
        for _ in 0..64 {
            let batch: Vec<WidgetId> = {
                let mut dirty = self.dirty.borrow_mut();
                if dirty.is_empty() {
                    return;
                }
                core::mem::take(&mut *dirty)
            };
            for id in batch {
                self.rebuild_dynamic(id);
            }
        }
    }

    fn rebuild_dynamic(&mut self, id: WidgetId) {
        let Some(pending) = self.nodes.get(id.0).and_then(|n| n.pending.clone()) else {
            return;
        };
        let Some(thunk) = pending.borrow_mut().take() else {
            return;
        };
        // Tear down the previous subtree (disposing its bindings), then build anew.
        for child in self.children_of(id).to_vec() {
            self.remove(child);
        }
        let child = thunk(self);
        self.append(id, child);
    }

    /// Sets the retained layout style for a node (consumed by the layout pass,
    /// not forwarded to the backend).
    pub fn set_style(&mut self, id: WidgetId, style: LayoutStyle) {
        if let Some(node) = self.nodes.get_mut(id.0) {
            node.style = style;
        }
    }

    /// Emits a frame (layout output) for `id` to the backend.
    pub fn set_frame(&mut self, id: WidgetId, rect: crate::core::Rect) {
        if self.nodes.get(id.0).is_some() {
            self.host.emit(Mutation::SetFrame { id, rect });
        }
    }

    /// Sets a paint attribute, forwarding it to the backend. Font size is also
    /// captured for text measurement.
    pub fn set(&mut self, id: WidgetId, attr: Attribute) {
        let Some(node) = self.nodes.get_mut(id.0) else {
            return;
        };
        match &attr {
            Attribute::FontSize(size) => node.font_size = *size,
            Attribute::Text(text) => *node.measure_text.borrow_mut() = Some(text.clone()),
            // Join segment titles so the layout heuristic can size the control.
            Attribute::Items(items) => {
                *node.measure_text.borrow_mut() = Some(items.join(" "));
            }
            _ => {}
        }
        self.host.emit(Mutation::SetAttribute { id, attr });
    }

    /// Binds an attribute to a reactive computation.
    ///
    /// `f` is run immediately (emitting the initial value) and re-run whenever a
    /// signal it reads changes — emitting **exactly one** `SetAttribute` per
    /// change. This is the core payoff of fine-grained reactivity: no tree diff,
    /// just a targeted update. The binding lives as long as the widget.
    pub fn bind(&mut self, id: WidgetId, mut f: impl FnMut() -> Attribute + 'static) {
        if self.nodes.get(id.0).is_none() {
            return;
        }
        let host = self.host.clone();
        let measure_text = self.nodes.get(id.0).unwrap().measure_text.clone();
        let effect = create_effect(move || {
            let attr = f();
            // Keep the measurable text current so re-layout reflects new content.
            if let Attribute::Text(text) = &attr {
                *measure_text.borrow_mut() = Some(text.clone());
            }
            host.emit(Mutation::SetAttribute { id, attr });
        });
        // Safe: existence checked above, and ids are stable handles.
        self.nodes.get_mut(id.0).unwrap().effects.push(effect);
    }

    /// Appends `child` as the last child of `parent`.
    pub fn append(&mut self, parent: WidgetId, child: WidgetId) {
        let index = match self.nodes.get(parent.0) {
            Some(p) => p.children.len(),
            None => return,
        };
        self.insert_child(parent, index, child);
    }

    /// Inserts `child` into `parent` at `index`.
    pub fn insert_child(&mut self, parent: WidgetId, index: usize, child: WidgetId) {
        // Validate both endpoints before mutating anything.
        if self.nodes.get(parent.0).is_none() || self.nodes.get(child.0).is_none() {
            return;
        }
        if let Some(c) = self.nodes.get_mut(child.0) {
            c.parent = Some(parent);
        }
        let clamped = {
            let p = self.nodes.get_mut(parent.0).unwrap();
            let i = index.min(p.children.len());
            p.children.insert(i, child);
            i
        };
        self.host.emit(Mutation::InsertChild {
            parent,
            index: clamped,
            child,
        });
    }

    /// Removes `id` and its entire subtree, disposing all reactive bindings and
    /// emitting `RemoveChild` (from its parent) followed by `Destroy` for every
    /// node, children-first.
    pub fn remove(&mut self, id: WidgetId) {
        if self.nodes.get(id.0).is_none() {
            return;
        }

        // Detach from parent's child list first (one RemoveChild for the root of
        // the removed subtree; descendants leave with their parent implicitly).
        let parent = self.nodes.get(id.0).and_then(|n| n.parent);
        if let Some(parent) = parent {
            if let Some(p) = self.nodes.get_mut(parent.0) {
                p.children.retain(|c| *c != id);
            }
            self.host.emit(Mutation::RemoveChild { parent, child: id });
        }

        self.destroy_subtree(id);

        if self.root == Some(id) {
            self.root = None;
        }
    }

    /// Depth-first teardown: dispose effects and emit `Destroy`, children first
    /// so a backend can rely on leaves being gone before their container.
    fn destroy_subtree(&mut self, id: WidgetId) {
        let Some(node) = self.nodes.get_mut(id.0) else {
            return;
        };
        let children = core::mem::take(&mut node.children);
        let effects = core::mem::take(&mut node.effects);

        for effect in effects {
            effect.dispose();
        }
        for child in children {
            self.destroy_subtree(child);
        }

        self.nodes.remove(id.0);
        self.host.emit(Mutation::Destroy { id });
    }

    // --- events (inbound seam) ---------------------------------------------

    /// Registers a handler for `kind` events targeting `id`. Multiple handlers
    /// per widget/kind are allowed and run in registration order. The handler
    /// lives as long as the widget.
    pub fn on(&mut self, id: WidgetId, kind: EventKind, callback: impl FnMut(&Event) + 'static) {
        if let Some(node) = self.nodes.get_mut(id.0) {
            node.handlers.push(Handler {
                kind,
                callback: Box::new(callback),
            });
        }
    }

    /// Registers a handler for an app-global event kind (e.g.
    /// [`EventKind::BackPressed`], lifecycle, keyboard).
    pub fn on_global(&mut self, kind: EventKind, callback: impl FnMut(&Event) + 'static) {
        self.global_handlers.push(Handler {
            kind,
            callback: Box::new(callback),
        });
    }

    /// Asks the backend to attach a gesture recognizer to `id`, so it emits the
    /// corresponding event. Pair with [`on`](Tree::on) for the handler.
    pub fn enable_gesture(&mut self, id: WidgetId, gesture: super::mutation::GestureKind) {
        if self.nodes.get(id.0).is_some() {
            self.host.emit(Mutation::AddGesture { id, gesture });
        }
    }

    /// A `Send` handle a backend uses to enqueue platform events.
    pub fn event_sink(&self) -> EventSink {
        EventSink::new(self.event_tx.clone())
    }

    /// Drains and dispatches all queued events. A driver calls this once per
    /// frame (the scheduler's `PreFrame` phase).
    pub fn drain_events(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            self.dispatch(&event);
        }
    }

    /// Routes one event to its handlers immediately.
    ///
    /// Targeted events fire on the target, then **bubble** up through ancestors
    /// (so a container can handle taps on its children). Global events fire on
    /// handlers registered via [`on_global`](Tree::on_global).
    pub fn dispatch(&mut self, event: &Event) {
        let kind = event.kind();
        match event.target() {
            Some(target) => {
                // Walk target -> root, collecting the live ancestor chain.
                let mut chain = Vec::new();
                let mut cursor = Some(target);
                while let Some(id) = cursor {
                    match self.nodes.get(id.0) {
                        Some(node) => {
                            chain.push(id);
                            cursor = node.parent;
                        }
                        None => break,
                    }
                }
                for id in chain {
                    self.run_handlers_on(id, kind, event);
                }
            }
            None => self.run_global_handlers(kind, event),
        }
    }

    /// Runs matching handlers on a single node. Handlers are moved out for the
    /// duration so they cannot alias the node while running (they may freely
    /// write signals, which run effects synchronously).
    fn run_handlers_on(&mut self, id: WidgetId, kind: EventKind, event: &Event) {
        let mut handlers = match self.nodes.get_mut(id.0) {
            Some(node) => core::mem::take(&mut node.handlers),
            None => return,
        };
        for handler in handlers.iter_mut() {
            if handler.kind == kind {
                (handler.callback)(event);
            }
        }
        if let Some(node) = self.nodes.get_mut(id.0) {
            // Keep any handlers registered during dispatch (today: none).
            let added = core::mem::take(&mut node.handlers);
            handlers.extend(added);
            node.handlers = handlers;
        }
    }

    fn run_global_handlers(&mut self, kind: EventKind, event: &Event) {
        let mut handlers = core::mem::take(&mut self.global_handlers);
        for handler in handlers.iter_mut() {
            if handler.kind == kind {
                (handler.callback)(event);
            }
        }
        let added = core::mem::take(&mut self.global_handlers);
        handlers.extend(added);
        self.global_handlers = handlers;
    }

    // --- introspection (for tests / inspector) -----------------------------

    /// Number of live widgets in the tree.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the tree has no widgets.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The children of `id`, in order (empty if `id` is unknown).
    pub fn children_of(&self, id: WidgetId) -> &[WidgetId] {
        match self.nodes.get(id.0) {
            Some(n) => &n.children,
            None => &[],
        }
    }

    /// The widget kind of `id`, if known.
    pub fn kind_of(&self, id: WidgetId) -> Option<WidgetKind> {
        self.nodes.get(id.0).map(|n| n.kind)
    }

    /// The retained layout style of `id`, if known.
    pub fn style_of(&self, id: WidgetId) -> Option<LayoutStyle> {
        self.nodes.get(id.0).map(|n| n.style)
    }

    /// The font size of `id` (defaults to 16pt), used for text measurement.
    pub fn font_size_of(&self, id: WidgetId) -> f32 {
        self.nodes
            .get(id.0)
            .map(|n| n.font_size)
            .unwrap_or(DEFAULT_FONT_SIZE)
    }

    /// The current text content of `id`, used by the layout pass to estimate
    /// intrinsic width. `None` if the node has no text.
    pub fn measure_text_of(&self, id: WidgetId) -> Option<String> {
        self.nodes
            .get(id.0)
            .and_then(|n| n.measure_text.borrow().clone())
    }
}
