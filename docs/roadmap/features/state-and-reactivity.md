# State & Reactivity

Our core advantage over RN/Flutter: fine-grained signals instead of a re-render
diff. Match the ergonomics of Redux/Zustand/Recoil/Riverpod/Provider while being
surgical. ✅ · 🟡 · ⬜.

## Primitives
- ✅ signals (sources), memos (derived), effects (sinks)
- ✅ glitch-free propagation, batching, untracked reads
- ✅ explicit `Runtime` + ownership scopes (auto-dispose)
- ⬜ stores (struct-of-signals) + selectors
- ✅ context / providers (dependency injection down the tree)
- ✅ `Resource` (async-aware signal: loading/error/data)
- ⬜ derived collections (fine-grained list reactivity, `keyed_for`)
- ⬜ writable computed / two-way bindings
- ⬜ signal equality customization / structural memo

## App-state patterns
- ⬜ global stores + scoped stores
- ⬜ actions/reducers pattern (opt-in, Elm/Redux-style) on top of signals
- ⬜ middleware / interceptors (logging, persistence, devtools)
- ⬜ selectors with memoization
- ⬜ transactions / batched commits
- ⬜ optimistic updates + rollback

## Async & concurrency
- ⬜ suspense / transitions (pending UI without tearing)
- ⬜ async derivations, debounce/throttle helpers
- ⬜ cross-thread signal writes marshaled to the UI thread (scheduler)
- ⬜ cancellation tied to ownership scopes

## Persistence & time-travel
- ⬜ persisted signals/stores (auto-save/restore)
- ⬜ hydration (SSR/web), state restoration (mobile)
- ⬜ time-travel debugging via devtools
- ⬜ undo/redo helpers

## Tooling
- ⬜ signal-graph inspector (dependencies, recompute counts)
- ⬜ leak detection in CI
- ⬜ lints for common reactivity mistakes
