# Extensibility & Plugins

How the world extends `rax` without forking it — the mechanism behind
[super-customizability](../03-customizability.md) and the native-API catalog. ⬜.

## Plugin platform
- ⬜ **plugin ABI** — versioned, semver-gated, stable third-party contract
- ⬜ platform-channel codegen (declare an interface in Rust → generate iOS/Android glue)
- ⬜ typed message passing Rust ↔ platform (sync + async + streams/events)
- ⬜ permission handling baked into plugins
- ⬜ graceful unsupported-platform fallbacks
- ⬜ plugin lifecycle (init/teardown, app-lifecycle hooks)
- ⬜ capability sandboxing / permission manifest
- ⬜ plugin conformance tests + certification
- ⬜ plugin registry / discovery / versioning

## Custom UI extension points
- ⬜ custom widgets via the `View` trait (compose primitives)
- ⬜ custom native `WidgetKind` registration at the render seam
- ⬜ host-view embedding (drop arbitrary `UIView`/`android.view.View`/DOM node)
- ⬜ custom `Layout` algorithms
- ⬜ custom rendering via the GPU path (`rax-vello`)
- ⬜ component registry: replace any built-in widget app-wide
- ⬜ pluggable transitions, navigators, easing curves
- 🟡 custom `Backend` implementations (target new platforms yourself)

## Framework extension points
- ⬜ middleware/interceptors for state, navigation, network
- ⬜ custom executors (async runtime)
- ⬜ custom storage backends
- ⬜ theme packages / design systems as dependencies
- ⬜ macros/derives for ergonomic component & store authoring (where they earn it)

## Ecosystem & governance
- ⬜ first-party module set (camera, location, push, BLE, NFC, IAP, …)
- ⬜ community plugin guidelines + authoring guide
- ⬜ semver + compatibility matrix for plugins
- ⬜ security review process for published plugins
- ⬜ "new architecture"-style stable boundaries so plugins survive core upgrades
