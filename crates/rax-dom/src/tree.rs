//! The retained element tree.
//!
//! Each node owns: its kind, parent/child links, and the reactive effects that
//! bind its attributes. When a node is removed we dispose its effects (so they
//! stop emitting) and tear down the subtree depth-first. This is also where the
//! reactive-runtime "ownership" gap noted in `rax-reactive` is closed for the UI:
//! effect lifetime is tied to element lifetime.

use std::sync::mpsc::{channel, Receiver, Sender};

use rax_core::{Arena, LayoutStyle};
use rax_reactive::{create_effect, Effect};

use crate::backend::Host;
use crate::event::{Event, EventKind, EventSink};
use crate::mutation::{Attribute, Mutation, WidgetId, WidgetKind};

/// Default font size (logical points) used for text measurement until set.
const DEFAULT_FONT_SIZE: f32 = 16.0;

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
        }
    }

    /// The current root widget, if one has been set.
    pub fn root(&self) -> Option<WidgetId> {
        self.root
    }

    /// Marks `id` as the tree root. (Backends treat the root specially: it is
    /// attached to the platform's content view rather than to a parent widget.)
    pub fn set_root(&mut self, id: WidgetId) {
        self.root = Some(id);
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

    fn create(&mut self, kind: WidgetKind) -> WidgetId {
        let index = self.nodes.insert(ElementNode {
            kind,
            parent: None,
            children: Vec::new(),
            style: LayoutStyle::default(),
            font_size: DEFAULT_FONT_SIZE,
            effects: Vec::new(),
            handlers: Vec::new(),
        });
        let id = WidgetId(index);
        self.host.emit(Mutation::Create { id, kind });
        id
    }

    /// Sets the retained layout style for a node (consumed by the layout pass,
    /// not forwarded to the backend).
    pub fn set_style(&mut self, id: WidgetId, style: LayoutStyle) {
        if let Some(node) = self.nodes.get_mut(id.0) {
            node.style = style;
        }
    }

    /// Sets a paint attribute, forwarding it to the backend. Font size is also
    /// captured for text measurement.
    pub fn set(&mut self, id: WidgetId, attr: Attribute) {
        let Some(node) = self.nodes.get_mut(id.0) else {
            return;
        };
        if let Attribute::FontSize(size) = attr {
            node.font_size = size;
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
        let effect = create_effect(move || {
            let attr = f();
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
}
