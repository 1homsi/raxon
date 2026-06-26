# raxon

A **100% Rust**, signal-driven framework for building **native** mobile apps. No JavaScript, no WebView — your declarative Rust UI renders real platform widgets (UIKit today; Android and WebAssembly DOM foundations in progress).

```toml
[dependencies]
raxon = "0.0.9"
```

```rust
use raxon::prelude::*;

fn counter(count: Signal<i32>) -> impl View {
    column((
        text(move || format!("Count: {}", count.get())).font_size(48.0),
        button("Increment", move || count.update(|c| *c += 1)),
    ))
    .padding(16.0)
    .gap(8.0)
}
```

## Why raxon

- **Fine-grained reactive.** Signals update only the views that read them — no virtual-DOM diff. Structure builds once; values bind in place.
- **Native rendering.** Real platform views via `objc2` (iOS) — native text, accessibility, and scrolling, not a canvas.
- **Pure Rust public API.** No Swift/Kotlin/JS in app code.
- **Stable Rust, no macros required.** The whole pipeline is testable host-side through a recording backend with zero platform code.

## Status

Early but functional. A reactive multi-screen app with tab navigation, dynamic lists, animations, and styled cards runs on the iOS Simulator today.

| Subsystem | Status |
|---|---|
| Signals / memos / effects | ✅ |
| View builder (column, row, text, button, …) | ✅ |
| iOS UIKit backend | ✅ |
| Navigation (stack, tabs, modals, deep links) | ✅ |
| Animation (tweens, springs, keyframes) | ✅ |
| Networking (HTTP, WebSocket, SSE) | ✅ |
| SQLite, Keychain, local storage | ✅ |
| Android backend | 🟡 command backend + driver + binding runtime + generated host glue + host session/registry/versioned bridge + command/event wire |
| Web/WASM backend | 🟡 DOM command backend + driver + binding runtime + generated host glue + host session/registry/versioned bridge + command/event wire |

## Structure

Everything ships as a single `raxon` crate with subsystems as modules:

```
raxon::core       — geometry, color, layout style
raxon::reactive   — signals, memos, effects, stores, context
raxon::dom        — virtual element tree and platform seam
raxon::view       — declarative view builder
raxon::ios        — UIKit backend (cfg'd to iOS targets)
raxon::android    — Android command backend, driver, and host binding adapter
raxon::web        — WebAssembly DOM command backend, driver, and host binding adapter
raxon::host       — shared mount/tick/resize/event-dispatch command-drain loop + opaque session registry + binding runtime + versioned JSON bridge protocol
raxon::wire       — versioned JSON event protocol shared by platform hosts
raxon::runtime    — app driver: layout, events, frames
raxon::nav        — stack/tab/modal navigation
raxon::net        — HTTP, WebSocket, SSE, query cache
raxon::anim       — tweens, springs, easing, keyframes
raxon::store      — persisted key-value signals
raxon::sqlite     — SQLite database access
raxon::keychain   — secure credential storage
raxon::scheduler  — frame scheduler and task priorities
```

## Building & testing

```sh
cargo test -p raxon                              # host-side, no platform needed
cargo check -p raxon --target aarch64-apple-ios-sim
cargo check -p raxon --target aarch64-linux-android
cargo check -p raxon --target wasm32-unknown-unknown
raxon generate --target all                      # Android/Web host glue
```

## License

MIT OR Apache-2.0.
