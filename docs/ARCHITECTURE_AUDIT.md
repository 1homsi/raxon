# rax — Architecture Audit & 10-Year Roadmap

> Lead-architect review. Brutally critical by mandate. Assume thousands of
> developers and millions of devices. Optimize for *maintainability over a
> decade*, not for shipping more code this week.

Date: 2026-06-25 · Reviewed commit state: `rax-core`, `rax-reactive`, `rax-dom`.

---

## 0. Status snapshot — what exists vs. what we claim

> **Vertical slice achieved (2026-06-25):** the counter demo runs on the iOS
> Simulator — native `UILabel`/`UIButton` laid out by the flexbox engine, driven
> entirely by Rust, with a tap→signal→one-mutation reactive loop. iOS came before
> Android only because the dev machine has Xcode but no Android SDK; the backend
> seam is identical for both.

| Layer | Built | Tested | Verdict |
|---|---|---|---|
| `rax-core` — geometry, arena, color, layout style | ✅ | ✅ | Solid leaf |
| `rax-reactive` — signals/memos/effects + Runtime + ownership (R1) | ✅ | ✅ | Glitch-free, isolatable |
| `rax-scheduler` — frame phases, tasks, marshaling (R2) | ✅ | ✅ | Built; runtime hot-path still uses direct ticks |
| `rax-dom` — element tree, mutation + event seam (R3) | ✅ | ✅ | Bidirectional |
| `rax-layout` — taffy flexbox → frames | ✅ | ✅ | Leaf text-measure is a heuristic (see M1 debt) |
| `rax-view` — macro-free builder (R4) | ✅ | ✅ | Dynamic lists/conditionals still TODO |
| `rax-runtime` — App: mount + layout + events + frames | ✅ | ✅ | Drives ticks directly (scheduler wiring pending) |
| `rax-ios` — UIKit backend via objc2 (pure Rust) | ✅ | runs on sim | Bootstrap uses deprecated UIScreen/window (M3 debt) |
| 30+ subsystems (text/IME, nav, animation, a11y, async, CLI, …) | ❌ | — | Greenfield |

~66 host tests, clippy-clean. The load-bearing decisions (R1–R3) are locked; the
public surfaces (View, Style, Backend/Event, Mutation) are stable enough to build
on. Known debt is tracked in the milestone notes.

---

## Part I — The decisions that become impossible to fix later

These are ranked. Each is a **STOP**: do not write more code on top of the current shape until these are resolved, because they change the *signature of every call site* downstream.

> **R1 — RESOLVED (2026-06-25).** `rax-reactive` rewritten: explicit `Runtime`
> (multi-instance, isolated) + `Owner`/`Scope` ownership tree + per-thread default
> for ergonomics. Re-running effects now dispose their nested reactivity (leak
> closed). Verified by `tests/ownership.rs` and `tests/isolation.rs`. Module split
> by domain: `runtime/{node,reactor,engine,mod}`, `handle`, `effect`, `control`.
> Remaining for R1: thread-confinement is still by convention; cross-thread writes
> get enforced in R2 (scheduler marshaling).

### R1 — The reactive runtime is a global thread-local singleton. This must become an explicit `Runtime` + ownership tree.

**What we have:** one `thread_local! REACTOR`. `signal.get()`/`set()` reach into ambient global state.

**Why it is fatal long-term:**
- **No isolation.** You cannot host two independent UI roots on one thread (multi-window, an embedded preview pane, the inspector rendering its own tree, server-driven UI snapshots, SSR-style prerender for tests). Every serious framework needs this eventually; retrofitting it means changing every signal signature.
- **No ownership/disposal tree.** Effects created inside effects/components are not auto-disposed. We patched *one* level of this in `rax-dom` (effects die with their widget), but that is a band-aid: memos, derived signals, and nested scopes still leak. Leptos shipped, then spent two major versions rebuilding ownership. We can avoid that tax by doing it once, now.
- **Threading correctness landmine.** Thread-local + "async-first" = silent bugs. An async task that resolves on a worker thread and calls `signal.set()` touches a *different* reactor (or panics). The rule "all signal mutation happens on the UI thread" must be **enforced by the type system or the scheduler**, not left as folklore.
- **Testing fragility.** Tests pass today only because libtest happens to use one thread per test. A single-threaded test runner, `#[tokio::test]`, or doctests sharing a thread would cause cross-test contamination.

**Recommendation:** Introduce `Runtime` (owns the arena) and `Owner`/`Scope` (a tree of disposal scopes). Signals belong to a runtime; the "current runtime" is entered explicitly (`runtime.enter(|| …)`), and `Owner` nodes form a parent/child tree so disposing a scope disposes everything created under it. The ergonomic global can remain as a *thin default* for app code, but the explicit runtime must exist underneath. **This is the #1 API to freeze before writing the component model.**

### R2 — Effects flush eagerly and synchronously inside `set()`. This must become scheduler-driven, coalesced to a frame.

**What we have:** `signal.set()` → immediately runs dependent effects → immediately emits mutations.

**Why it is fatal long-term:**
- **Jank & wasted work.** Setting five signals in an event handler runs effects five times and emits five mutation bursts. `batch()` mitigates manually, but a real UI must coalesce to one commit per frame *automatically*, aligned to the display refresh (iOS `CADisplayLink`, Android `Choreographer`).
- **No prioritization.** There is no notion of input-priority vs. animation-priority vs. idle work. Production apps need this to stay at 60/120fps under load (React's concurrent mode and Flutter's pipeline both exist for this reason).
- **Layout has nowhere to live.** Layout must run *after* all signal-driven attribute changes for a frame and *before* mutations are flushed to native. The current "emit immediately" model has no phase where layout can sit. This is a structural gap, not a tuning problem.

**Recommendation:** Split the frame into phases: **(1)** signals mark dependents dirty (no eager run); **(2)** a `Scheduler` flushes effects once per frame to produce attribute/structure mutations; **(3)** the layout engine computes geometry → emits `SetFrame` mutations; **(4)** the command buffer is handed to the backend in one commit. The reactive runtime's `flush_effects` becomes scheduler-owned, not `set`-owned. **Freeze the Scheduler interface before the layout engine.**

> **R3 — RESOLVED (2026-06-25).** `rax-dom` gained the inbound dual of `Mutation`:
> an `Event`/`EventKind` schema, a `Send` `EventSink` for backends, a per-widget
> handler registry with bubbling, app-global handlers, and a `drain_events` path
> for the scheduler's `PreFrame` phase. Round trip verified in `tests/events.rs`
> (platform event → handler → signal write → one mutation). **M0 complete.**
> Deferred to M1 integration: wiring the scheduler `Commit` phase to a buffered
> `Host` flush, and `PreFrame` to `Tree::drain_events` — both live in `rax-runtime`.

### R3 — The command buffer is one-directional. Native → engine events have no defined channel.

**What we have:** `Backend::apply(Mutation)` — engine talks *to* the platform. Nothing comes *back*.

**Why it is fatal long-term:**
- A button tap, a scroll offset, a `UITextField` edit, a keyboard frame change, a back-press, a lifecycle callback — all originate on the platform side and must reach signals. Without a designed inbound channel, the event system, gestures, text input, and navigation **cannot be built**, and bolting a second mechanism on later will fight the first.
- **Controlled text input** (the single hardest part of RN parity) is inherently bidirectional and race-prone: the native field reports an edit, a signal updates, an effect pushes the value back, and cursor/IME composition state must not be clobbered. This demands a first-class, ordered, identity-stable event protocol — not an afterthought.

**Recommendation:** Define the inbound dual of `Mutation` now: an `Event`/`HostMessage` enum (`Tap{id}`, `ScrollChanged{id, offset}`, `TextChanged{id, value, selection}`, `FocusChanged`, `KeyboardWillShow{rect}`, `Lifecycle(...)`, `BackPressed`) delivered through the Scheduler onto the UI thread, where it lands as signal writes. The `Backend` trait gains an outbound sink (`EventSink`) the platform calls. **Freeze the `Event` schema alongside `Mutation`.**

> **R4 — IN PROGRESS (2026-06-25).** `rax-view` built: a macro-free typed
> tuple-builder (`column`/`row`/`text`/`button` + modifiers) lowering to the
> `Tree`. `View::build(self) -> WidgetId` (build-once; signals update values,
> dedicated views will handle dynamic structure). The counter is proven
> end-to-end in `tests/counter.rs` (build + reactive update + tap). **Decision:
> the builder is the foundational API and is never required; an optional `rsx!`
> macro (future `rax-macro` crate) will expand *into* these calls — JSX→builder,
> Dioxus RSX→builder.** Remaining for R4: dynamic lists (keyed `For`) and
> conditionals (`Show`); ergonomic review at scale before freezing.

### R4 — The public component (`View`) API does not exist — and it is the single most important thing to design.

**What we have:** an *imperative* `Tree` (`create_view`, `bind`, `append`). That is the right *internal* lowering target, but it is **not** the API thousands of developers will write. The declarative component model — the thing that makes this "React-like" — is undesigned. Everything (DX, docs, tutorials, third-party components, the entire value proposition) hinges on it, and it is the API most expensive to change after release.

**Recommendation:** Before more subsystems, design and prototype the `View`/component API and prove it lowers cleanly to `Tree` + reactivity, including the two hard cases: **dynamic lists** (keyed reconciliation — the *only* place we need diffing) and **conditional subtrees** (`Show`). Write the counter, a list, and a conditional in the proposed API and review the ergonomics *as a user* before committing.

### R5 — "100% Rust" is false at the entry point, and the FFI/ABI strategy is undefined.

**Brutal truth:** You cannot have *zero* Kotlin/Swift/ObjC. Android needs a `JNI_OnLoad` + an `Activity`/`SurfaceView` host; iOS needs an `AppDelegate`/`UIApplicationMain` and a view-controller host. There will be a few hundred lines of platform glue. The honest, defensible claim is: **"No platform language in *application* code or the public API; all app logic and UI is Rust."** Define this precisely in the README, or the project loses credibility on first inspection.

Also undefined and load-bearing: the Rust↔platform boundary itself — Android via JNI (`cargo-ndk`, `.so` in an `.aar`), iOS via a static lib + a thin generated shim. The `Mutation`/`Event` types crossing JNI per-item will be a throughput bottleneck; plan a **batched, encoded command buffer** across FFI from the start (one JNI call per frame carrying a buffer, not one call per mutation).

---

## Part II — Subsystem-by-subsystem audit

Fully expanded for the load-bearing subsystems. Far-future subsystems (Part II.C) are summarized honestly rather than padded — writing ten paragraphs on NFC today would be fiction.

### II.A — Built or imminent

#### Reactive runtime / Signals (`rax-reactive`) — exists
- **Purpose:** Fine-grained dependency tracking so a state change updates only the views that read it.
- **Responsibilities:** signals (sources), memos (cached derivations), effects (sinks), automatic dep tracking, glitch-free propagation, batching.
- **Public API:** `create_signal/memo/effect`, `Signal::{get,set,with,update}`, `Memo::{get,with}`, `Effect::dispose`, `batch`, `untrack`.
- **Internal:** thread-local `Reactor` over `rax-core::Arena`; Clean/Check/Dirty pull algorithm; `Box<dyn Any>` value erasure (the only RTTI in the framework).
- **Dependencies:** `rax-core`.
- **Bottlenecks:** `Vec<Index>` source/observer lists do linear `contains` on subscribe (fine for small fan-out, quadratic for pathological graphs); `Box<dyn Any>` + clone on every `get` for non-Copy types.
- **Scalability concerns:** **R1 (global), R2 (eager flush)**. No owner tree → leaks. No batching to frame.
- **Testing:** 13 behavioural tests incl. diamond/glitch-freedom, dynamic deps, batch, dispose. Strong. Needs: property tests for graph invariants; leak assertions once owners exist.
- **Missing:** explicit runtime, owner/scope tree, scheduler integration, cross-thread write marshaling.
- **Blocks shipping:** **YES** (via R1/R2).

#### Element tree + Mutation buffer + Renderer seam (`rax-dom`) — exists
- **Purpose:** Retained tree of widgets; produce a backend-agnostic mutation stream; define the one trait platforms implement.
- **Responsibilities:** node identity/lifetime, parent/child structure, reactive attribute binding→mutation, subtree teardown + effect disposal.
- **Public API:** `Tree`, `WidgetId`, `WidgetKind`, `Attribute`, `Mutation`, `Backend`, `Host`, `RecordingBackend`.
- **Internal:** `Arena<ElementNode>`; effects own attribute bindings; `Host` = `Rc<RefCell<dyn Backend>>`.
- **Dependencies:** `rax-core`, `rax-reactive`.
- **Bottlenecks:** `Attribute` carries owned `String`/values → per-update allocation; `Mutation` is heap-y; **no `SetFrame`/layout output**; per-mutation FFI (R5).
- **Scalability concerns:** flat `Attribute` enum will balloon and every backend must exhaustively match it (versioning hazard); one-directional (R3); no reconciler for dynamic children.
- **Testing:** 7 e2e tests via `RecordingBackend` incl. the "one mutation per change" thesis and teardown ordering. Good pattern; reuse everywhere.
- **Missing:** keyed list reconciliation, conditional subtrees, layout mutations, inbound events, command-buffer encoding/pooling.
- **Blocks shipping:** **YES**.

#### Scheduler — **does not exist (critical gap)**
- **Purpose:** Own the frame loop; coalesce reactive flushes, layout, and commit into ordered phases at display cadence; prioritize work.
- **Public API (proposed, freeze early):** `Scheduler::request_frame()`, phase callbacks, `spawn_on_ui(task)`, priority lanes (Input/Animation/Default/Idle).
- **Dependencies:** `rax-reactive`, `rax-dom`, platform vsync (Choreographer/CADisplayLink).
- **Risks:** wrong phase ordering is an architectural mistake (R2). Integration with async runtime wakers.
- **Blocks shipping:** **YES.** This is the missing spine connecting reactivity→layout→render.

#### Component model / `View` trait — **does not exist (R4)** — Blocks shipping: **YES**
#### Virtual tree / Reconciler — **partial.** With signals, diffing is needed *only* for dynamic sequences/conditionals; the static tree binds once. Keyed list reconciliation is unbuilt. Blocks shipping: **YES** (lists are table stakes).

### II.B — Near-term, design-sensitive

#### Layout engine (flexbox)
- **Purpose:** Compute geometry for native views from fl/sex constraints.
- **Public API (freeze early):** `Style` (flex-direction/justify/align/wrap/grow/shrink/basis/margin/padding/min/max/aspect), `compute_layout(root, available) -> tree of Rect`.
- **Internal:** Consider **adopting `taffy`** (mature, stable Rust flexbox/grid, used by Bevy/Zed/Dioxus) rather than writing our own — reinventing Yoga is a multi-year sink and a maintenance liability. *Recommend: depend on `taffy`, wrap it behind our `Style` type.* This is a place to **not** be 100%-NIH.
- **Bottlenecks:** full relayout on any change; need dirty-subtree layout + measure caching. Text measurement requires a platform round-trip (intrinsic sizing of `UILabel`/`TextView`) — a hard cross-boundary dependency.
- **Blocks shipping:** **YES.**

#### Styling system
- **Purpose:** Typed style props → layout + paint attributes; theming; density (dp/pt/px) resolution.
- **Scalability:** decide now whether styles are inline-only (RN) or support a cascade/theme. Inline + explicit theme context is the maintainable choice; avoid CSS cascade.
- **Blocks shipping:** partial (need enough for v1 widgets).

#### Renderer abstraction + Android + iOS backends
- **Purpose:** Apply mutations to native views; deliver events back.
- **Public API (freeze — third parties will write backends):** `Backend::apply(&CommandBuffer)`, `EventSink`, lifecycle hooks, root attach.
- **Internal:** Android = JNI via `jni`/`cargo-ndk`, view recycling, main-looper marshaling; iOS = `objc2`/`core-foundation`, UIKit, main-thread dispatch.
- **Bottlenecks:** FFI crossing (R5) — batch per frame; view creation cost — pool/recycle; main-thread contention.
- **Risks:** **the two backends drifting in behavior** (the classic RN bug class). Mitigate with a **shared conformance test suite** every backend must pass.
- **Blocks shipping:** **YES** (at least one backend).

#### Event system / Gestures / Focus
- **Purpose:** Route platform input to handlers; recognize gestures; manage focus order.
- **Hard fork (decide now):** **native gesture recognizers** (consistent with native feel, but cross-platform divergence) vs. a **Rust gesture arena** (Flutter-style, consistent across platforms, but you fight the platform's own recognizers and scroll views). *Recommendation: native recognizers for v1 (tap/scroll/pan) exposed through a unified `Event` schema; revisit a Rust arena only if divergence hurts.* This depends entirely on **R3**.
- **Blocks shipping:** **YES** (tap + scroll minimum).

#### Text rendering / IME / Keyboard
- **Brutal:** this is where RN-likes go to die. With native widgets you inherit shaping/bidi/emoji/a11y for free — *huge* win and the main reason to pick native widgets. But **controlled `TextInput`** (value bound to a signal, IME composition, autocorrect, selection, secure entry, keyboard avoidance) is bidirectional, race-prone, and platform-divergent. Needs R3 plus a carefully specified text-edit protocol (debounced, composition-aware, source-of-truth rules).
- **Blocks shipping:** **YES** (no real app ships without text input).

#### Accessibility
- **Purpose:** Expose semantics to VoiceOver/TalkBack.
- **Decision:** with native widgets, **map semantic props to platform a11y APIs** (not AccessKit, which targets custom-drawn UIs). Make a11y props **first-class attributes from day one** — retrofitting accessibility is brutal and often legally required. Cheap if early, expensive if late.
- **Blocks shipping:** Should be a v1 gate for credibility; pragmatically Milestone 3.

#### Navigation
- **Fork:** native nav containers (`UINavigationController`/Fragment back-stack) vs. Rust-owned stack rendering plain views. *Recommendation: Rust-owned navigation state with native transition primitives*, so deep-linking, state restoration, and testing live in Rust. Depends on R3 (back-press, lifecycle).
- **Blocks shipping:** **YES** for multi-screen apps.

#### Animation system
- **Purpose:** Drive interpolated values at frame cadence; spring/timing curves; gesture-driven.
- **Internal:** an `Animation` is a signal driven by the Scheduler's frame tick. **Depends entirely on the Scheduler (R2).** Must run on the UI thread, ideally offloading to platform Core Animation / `ValueAnimator` where possible to stay smooth during JS-thread-equivalent stalls.
- **Blocks shipping:** No for v1; **YES** for competitiveness.

#### Async task runtime
- **Decision:** do **not** hard-depend on tokio (binary size, mobile fit). Define an `Executor` trait; default to a small executor (`async-executor`/`smol`-style) with a UI-thread `LocalSpawn`. Wakers must route completions to the Scheduler so signal writes happen on the UI thread (ties to R1/R2).
- **Blocks shipping:** **YES** (any networked app).

### II.C — Future subsystems (honest, terse — not yet design-critical)

| Subsystem | Status | Main risk / note | Blocks v1 ship? |
|---|---|---|---|
| Asset pipeline | none | bundling/density variants/`include_dir` vs platform assets; define resolution early-ish | No |
| Resource management | none | image/texture cache eviction, memory pressure callbacks | No |
| Image loading | none | async decode off-thread → native image view; cache; depends on async + FFI | M1/M2 |
| Networking | none | use `reqwest`/`hyper` behind a trait; TLS/cert/cookies; binary size | No (M2) |
| Local storage | none | KV (prefs) + SQLite (`rusqlite`); migration story | No (M2) |
| Permissions | none | inherently platform; plugin-shaped; async request flow | No (M3/4) |
| Platform channels | none | the generic Rust↔platform call mechanism; **design with R3/R5** so plugins reuse it | M3 |
| Plugin system | none | **freeze the plugin ABI carefully** — third-party stability contract; semver discipline | M4 |
| CLI | none | `cargo`-wrapping; `rax new/run/build`; device deploy | M1 (DX) |
| Project templates | none | trivial after CLI | M1 |
| Build system | none | `cargo-ndk`, xcframework packaging, codesigning; CI matrix | M1 (Android), M3 (iOS) |
| Hot reload | none | **HIGH RISK** — see Part IV note; likely "fast rebuild + state preserve", not true HMR | M3 (downgrade scope) |
| Inspector | none | tree/props/signal-graph viewer; reuse `RecordingBackend`-style introspection | M3 |
| Logging | none | `tracing` behind a facade → platform logcat/oslog | M1 (cheap, do early) |
| Error overlay | none | dev-only red-box from panics + `Result` surfaces | M2 |
| Testing framework | partial | `RecordingBackend` is the seed; add a headless host + finder/query API + snapshot of mutation streams | M1 ongoing |
| Benchmark suite | none | `criterion` micro + frame-time macro on device; **establish baselines before optimizing** | M1 ongoing |
| Documentation | this file + rustdoc | needs guide/book + API stability docs; doc-tests as examples | ongoing |
| Versioning strategy | none | see Part III | before 1.0 |

---

## Part III — APIs to freeze before writing more code

Freezing means: design, prototype against real usage, review, then treat as semver-protected. In dependency order:

1. **`Runtime` + `Owner`/`Scope`** (R1) — every signal call site depends on it.
2. **`Scheduler` phase model** (R2) — reactivity, layout, animation, async all plug in here.
3. **`Mutation` *and* `Event`/`HostMessage` schemas** (R3) — the bidirectional command-buffer contract; also the FFI wire format (R5).
4. **`Backend` + `EventSink` trait** — third parties write backends; breaking it breaks the ecosystem.
5. **The `View`/component public API** (R4) — the developer-facing surface; the most expensive to change.
6. **`Style` type** — wraps the layout engine; user-facing.
7. **Plugin ABI** — only when we get there, but it is a hard stability boundary forever after.

Everything else can evolve behind these.

---

## Part IV — Roadmap

Each milestone lists blockers, debt, risks, expected perf issues, and the APIs that must be stable by its end. Ordered to **resolve R1–R5 before they calcify**.

### Milestone 0 — Foundations refactor (NEW, must precede feature work)
**Goal:** fix R1–R3 while the blast radius is 3 crates, not 30.
- Explicit `Runtime` + `Owner`/`Scope` ownership tree; keep ergonomic global default.
- `Scheduler` with phased frame (mark → flush effects → layout → commit) + priority lanes.
- Bidirectional command buffer: define `Event`/`HostMessage`; `Backend` gains `EventSink`.
- Decide & document the FFI wire format (batched, encoded) and the "100% Rust" scope (R5).
- **Blockers:** none external — pure design+refactor.
- **Debt created:** ergonomic global must be kept in sync with explicit runtime.
- **Risks:** over-engineering the scheduler before a backend exists to validate cadence. Mitigate with a fake clock-driven scheduler + tests.
- **Perf:** establish `criterion` baselines for signal propagation now.
- **Stabilize:** Runtime, Scheduler phases, Mutation+Event schema.

### Milestone 1 — Vertical slice on Android
**Scope:** Reactive runtime (post-M0) · Component/`View` API (R4) · Layout (adopt `taffy`) · Android backend (JNI) · Text · Button · Image · basic Styling · `tracing` logging · CLI `new`/`run` · headless test host · benchmarks.
- **Blockers:** M0 done; `cargo-ndk` build pipeline; text intrinsic-size round-trip to platform.
- **Debt:** Android-only quirks may leak into shared code — guard with the conformance suite even before iOS exists.
- **Risks:** the `View` API proving unergonomic *after* layout depends on it (prototype it in M0/early-M1). JNI per-call overhead (batch the buffer).
- **Perf issues:** view creation cost (recycle); first-frame latency; main-thread marshaling.
- **Stabilize:** `View` API, `Style`, `Backend`+`EventSink`, CLI surface.

### Milestone 2 — Real-app capability
**Scope:** Lists (keyed reconciliation + recycling) · ScrollView · TextInput + IME (R3-dependent) · Navigation · Animations (Scheduler-driven) · Networking (trait + default) · async runtime · local storage · error overlay.
- **Blockers:** Scheduler (M0); event channel (M0); list recycling needs layout maturity.
- **Debt:** controlled-input edge cases; nav state restoration.
- **Risks:** **TextInput/IME is the highest-risk item in the project** — budget generously, test on real IMEs (CJK, emoji, autocorrect). List perf at 10k items.
- **Perf:** scroll jank, recycling correctness, animation/scroll contention on the UI thread.
- **Stabilize:** list/scroll/input/navigation APIs (heavily used → expensive to change).

### Milestone 3 — Second platform + polish
**Scope:** iOS backend (objc2/UIKit) · Accessibility (both platforms) · Inspector · Hot reload (scoped down) · cross-platform conformance suite hardened.
- **Blockers:** stable `Backend`/`Event` contract (so iOS is "just" an implementation).
- **Debt:** any Android assumptions baked into shared crates surface here — pay it down.
- **Risks:** **two backends diverging** (conformance suite is the only defense). **Hot reload may be technically infeasible as true HMR in Rust** (no stable ABI, slow recompiles); de-risk by scoping to fast incremental rebuild + signal-state preservation across reload, not live code patching. A11y retrofit pain if not seeded in M1.
- **Stabilize:** a11y prop schema; inspector protocol.

### Milestone 4 — Ecosystem & native capability
**Scope:** Plugin API/ABI · Platform channels (generalized) · Camera · Bluetooth · NFC · Notifications · permissions framework.
- **Blockers:** platform-channel mechanism (seeded in M0/M2); a *frozen* plugin ABI.
- **Debt:** every plugin is a long-term support obligation; keep the core set small and curated.
- **Risks:** plugin ABI mistakes are forever — version it explicitly, gate with a compatibility test harness.
- **Stabilize:** plugin ABI, platform-channel protocol.

### Pre-1.0 — Versioning & stability
- Pre-1.0: minor = breaking is acceptable, but **document a stability tier per crate** (`rax-core` stable; `rax-view` evolving; backends internal).
- Adopt SemVer + a deprecation policy + MSRV policy (stable Rust, N-2 versions).
- A public **API-stability doc** and `cargo-semver-checks` in CI before 1.0.
- Conformance test suite is a **release gate**, not optional.

---

## Part V — What blocks shipping, in one view

**Hard blockers (no real app without these):** Scheduler · Component/`View` API · bidirectional events (R3) · Layout · ≥1 native backend · Text+Button+Image · Lists/ScrollView · TextInput/IME · Navigation · async+networking · CLI/build.

**Credibility blockers:** Accessibility · the honest "100% Rust" framing (R5) · a stability policy · the cross-backend conformance suite.

**Single biggest risk:** TextInput/IME bidirectional correctness across platforms. **Single biggest architectural trap:** shipping the global reactor + eager flush (R1/R2) into a public API and never being able to claw it back.

---

## Appendix — Recommended deviations from the original spec

> **Ratified 2026-06-25:** Milestone 0 (R1–R3 refactor) approved to run before
> feature work. Layout will **adopt `taffy`** behind a `rax-style::Style` type.

1. **Adopt `taffy` for layout** instead of hand-rolling flexbox (NIH here is a decade-long liability). — *Decision ratified.*
2. **Reframe "100% Rust"** to "no platform language in app code / public API."
3. **Insert Milestone 0** (Runtime/Scheduler/Event refactor) before any feature work.
4. **Downgrade "hot reload"** expectations to fast-rebuild + state-preservation until/unless binary-patching (Subsecond-style) proves viable.
5. **Keep the curated plugin set tiny**; the ABI is a permanent contract.
