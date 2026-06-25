# Developer Experience & Tooling

Match Expo/RN CLI + Metro + Flutter CLI + DevTools + hot reload. ✅ · 🟡 · ⬜.

## CLI & project
- ✅ `rax new <name>` (scaffolds Cargo.toml + src/lib.rs + .gitignore)
- ✅ `rax run [--target ios-sim|ios]` (prints cargo build command + xcodebuild invocation)
- ✅ `rax build [--target ios-sim|ios|android|macos]` (prints cargo cross-compile command + Xcode link notes)
- ✅ `rax test [-- args]` (runs `cargo test`); `rax lint` (`cargo clippy --all-targets`); `rax fmt [--check]` (`cargo fmt`)
- ✅ `rax doctor` (checks rustc, cargo, iOS/wasm targets, Xcode CLI tools)
- ✅ `rax add <crate>` (prints `cargo add` command); ⬜ `rax generate` (codegen/scaffold)
- ⬜ project templates + starter kits

## Build & toolchain
- ⬜ cross-compile orchestration (cargo-ndk, xcframework, wasm)
- ⬜ incremental build caching, fast rebuilds
- ⬜ asset pipeline (density variants, fonts, bundling, tree-shaking)
- ⬜ app-size analyzer, dependency graph
- ⬜ environment/config + secrets management, build flavors/variants
- ⬜ monorepo support, CI presets

## Hot reload / fast refresh
- ⬜ **fast refresh**: rebuild + reload with **state preserved**
- ⬜ error-recovery reload, retry-last-action
- ⬜ true hot-reload via binary patching (Subsecond-style) if feasible
- ⬜ live theme/asset reload

## DevTools & inspector
- ⬜ element-tree inspector (select on device → highlight)
- ⬜ props + computed style viewer; layout overlay (margins/padding/frames)
- ⬜ **signal-graph inspector** (deps, recompute counts, time-travel)
- ⬜ network panel, log/console panel, storage panel
- ⬜ performance flame charts, frame-time/jank view, memory view
- ⬜ accessibility inspector
- ⬜ standalone devtools app + VS Code / JetBrains extensions
- ⬜ remote debugging (device ↔ desktop)

## Error handling & diagnostics
- ⬜ red-box error overlay (panics + `Result` surfaces) with source mapping
- ✅ structured logging (`rax-log`: `rax_debug!/info!/warn!/error!` macros; `Level` enum; `set_min_level`; routes to `println!` on iOS → Xcode console)
- ⬜ great compiler-error ergonomics for the view API (type-erasure boundaries)
- ⬜ crash reporting / symbolication SDK hooks
- ⬜ analytics/observability hooks

## Testing
- ✅ recording backend; host-side pipeline tests
- 🟡 headless host + finder/query API (find-by-text/role/testID)
- ✅ widget interaction tests (tap / value / long-press / double-tap / pan / arbitrary events)
- ⬜ snapshot tests (mutation stream + golden images per platform)
- ⬜ the cross-platform **conformance suite** (release gate)
- ⬜ property/fuzz tests (layout, reconciler, reactivity)
- ⬜ device-farm integration, coverage gates, flaky-test management
- ⬜ e2e/integration test driver

## Docs & learning
- ⬜ the book (guide), full API docs, runnable examples gallery
- ⬜ interactive playground (web target), cookbook, migration-from-RN guide
- ⬜ codemod/assistant for RN→rax
