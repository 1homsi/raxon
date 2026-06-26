# raxon

A reactive, signal-driven native UI framework for Rust.

```toml
[dependencies]
raxon = "0.0.2"
```

---

## Hello, raxon

```rust
use raxon::prelude::*;

fn counter(count: Signal<i32>) -> impl View {
    column((
        text(move || format!("Count: {}", count.get()))
            .font_size(48.0),
        row((
            button("−", move || count.update(|c| *c -= 1)),
            button("+", move || count.update(|c| *c += 1)),
        ))
        .gap(12.0),
    ))
    .padding(24.0)
    .gap(16.0)
}

fn main_app() -> impl View {
    let count = create_signal(0);
    counter(count)
}
```

Add the entry point on iOS:

```rust
#[no_mangle]
pub extern "C" fn raxon_main() {
    raxon::run(App::new(main_app));
}
```

---

## Why raxon

**Fine-grained reactive.** Signals propagate changes surgically — only the views that read a signal re-evaluate. No virtual-DOM diff, no full-subtree re-render.

**Real native widgets.** Renders `UILabel`, `UIButton`, `UIScrollView`, etc. via `objc2`. You get platform text rendering, accessibility, and scroll physics for free.

**Pure Rust public API.** No Swift, no Kotlin, no JavaScript in your app code. Platform glue is inside raxon, written in Rust via `objc2`/JNI — never in your app.

**Testable on the host.** A `RecordingBackend` lets you mount views, drive signals, and assert mutations in plain `cargo test` — no simulator required.

---

## Modules

| Module | What's inside |
|---|---|
| `raxon::reactive` | `Signal`, `Memo`, `Effect`, `Store`, `Resource`, context |
| `raxon::view` | `column`, `row`, `text`, `button`, `image`, `scroll`, `list`, … |
| `raxon::dom` | Virtual element tree, `Mutation`/`Event` seam, `Backend` trait |
| `raxon::runtime` | `App` driver, layout loop, haptics, notifications, biometrics |
| `raxon::ios` | UIKit backend — only compiled on `target_os = "ios"` |
| `raxon::nav` | Stack, tab, modal navigators; deep links; route guards |
| `raxon::net` | HTTP, WebSocket, SSE, reactive query cache |
| `raxon::anim` | Tweens, springs, keyframes, off-thread animation |
| `raxon::store` | Persisted key-value signals (UserDefaults bridge) |
| `raxon::sqlite` | SQLite via rusqlite |
| `raxon::keychain` | Secure credential storage (Keychain on device) |
| `raxon::scheduler` | Frame scheduler, priority tasks |
| `raxon::style` | Theme system — colors, spacing, typography, radius tokens |
| `raxon::intl` | Locale-aware number/date formatting |
| `raxon::i18n` | Message catalog lookup (`t!`, `t_args!`, `t_plural!`) |

Import everything you need for typical app work from the prelude:

```rust
use raxon::prelude::*;
```

---

## Reactive primitives

```rust
// Source
let name = create_signal(String::from("world"));

// Derived (cached, glitch-free)
let greeting = create_memo(move || format!("Hello, {}!", name.get()));

// Side effect
create_effect(move || println!("{}", greeting.get()));

// Struct-of-signals store
#[derive(Clone)]
struct Counter { value: i32, step: i32 }

let store = Store::new(Counter { value: 0, step: 1 });
let doubled = store.select(|s| s.value * 2);   // -> Memo<i32>
store.update(|s| s.value += s.step);
```

---

## Navigation

```rust
use raxon::prelude::*;
use raxon::nav::*;

routes! {
    Home => home_screen,
    Detail(id: u32) => detail_screen,
    Settings => settings_screen,
}

fn home_screen() -> impl View {
    let nav = use_navigator();
    button("Go to settings", move || nav.push(Routes::Settings))
}
```

---

## Async & networking

```rust
use raxon::prelude::*;

fn user_profile(user_id: u32) -> impl View {
    let profile = create_resource(move || async move {
        get(&format!("https://api.example.com/users/{user_id}"))
            .await?
            .json::<UserProfile>()
    });

    dynamic(move || match profile.get() {
        ResourceState::Loading  => text("Loading…").into_view(),
        ResourceState::Ready(p) => text(&p.name).into_view(),
        ResourceState::Error(e) => text(&format!("Error: {e}")).into_view(),
    })
}
```

---

## Status

Early but functional. A reactive multi-screen app with tab navigation, dynamic lists, animations, and styled cards runs on the iOS Simulator today.

| Feature | Status |
|---|---|
| Signals / memos / effects / stores | ✅ |
| View builder — 40+ components | ✅ |
| iOS UIKit backend | ✅ |
| Stack / tab / modal navigation | ✅ |
| Deep links, route guards | ✅ |
| HTTP, WebSocket, SSE, query cache | ✅ |
| Tweens, springs, keyframe animation | ✅ |
| SQLite, Keychain, UserDefaults | ✅ |
| Flexbox layout (taffy) | ✅ |
| Theme system | ✅ |
| i18n / intl | ✅ |
| Android backend | ⬜ |
| Web / WASM backend | ⬜ |

---

## License

MIT OR Apache-2.0
