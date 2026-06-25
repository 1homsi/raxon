# Navigation & Routing

Match React Navigation / Expo Router and Flutter Navigator 2.0 / go_router.
Rust-owned navigation state with native transition primitives. ⬜ planned.

## Navigators
- ✅ stack navigator (push/pop/replace/popToTop/popToRoot)
- ✅ tab navigator (bottom tabs + top tabs)
- ⬜ modal / sheet presentation (full / page-sheet / form-sheet / sizes)
- ⬜ drawer / side-menu navigator
- ⬜ nested navigators (tabs containing stacks, etc.)
- ⬜ split-view / master-detail (tablet/desktop adaptive)

## Routing
- ✅ typed routes (compile-checked params)
- ⬜ declarative URL routing (path patterns, params, query)
- ⬜ deep links + universal/app links
- ⬜ web-history integration (for the web target)
- ⬜ redirects / guards / auth gating
- ⬜ not-found / fallback routes
- ⬜ programmatic navigation API + imperative ref

## Transitions & gestures
- ⬜ default platform transitions (iOS push/Android shared-axis)
- ⬜ custom transitions (pluggable, fully overridable)
- ⬜ interactive pop / swipe-back gesture
- ⬜ predictive back (Android), interruptible transitions
- ⬜ shared-element / hero transitions
- ⬜ transition lifecycle hooks

## State & lifecycle
- ⬜ navigation state restoration (kill/restore)
- ⬜ screen focus/blur lifecycle events
- ⬜ params passing + result return (e.g. pick-and-return)
- ⬜ back-handling (hardware back, escape key)
- ⬜ navigation events / listeners / analytics hooks
- ⬜ preserve/lazy screen mounting; keep-alive tabs

## Advanced
- ⬜ server-driven navigation
- ⬜ deep-link preview / handoff / quick actions
- ⬜ nav devtools (current stack inspector)
