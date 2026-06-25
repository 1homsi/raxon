# Async, Networking & Data

Match `fetch`/axios/react-query (RN) and `http`/`dio`/`riverpod` (Flutter). ⬜.

## Async runtime
- ✅ `Executor` trait (pluggable); small default executor (not tokio-locked)
- ⬜ UI-thread `spawn_local` + worker-thread `spawn`
- ⬜ wakers marshal completions onto the UI thread (scheduler)
- ⬜ timers, intervals, debounce/throttle, `next_frame`
- ⬜ cancellation tied to component/ownership scopes
- ⬜ structured concurrency helpers

## Networking
- 🟡 HTTP client behind a trait (GET/POST/etc., headers, query, body)
- ⬜ TLS, cert pinning, cookies, redirects, timeouts, retries/backoff
- ⬜ multipart upload, streaming download, progress
- ⬜ JSON (serde) + form/urlencoded; typed request/response
- ⬜ WebSocket client, Server-Sent Events
- ⬜ GraphQL helper (queries/mutations/subscriptions)
- ⬜ gRPC (optional)
- ⬜ connectivity awareness, offline detection, request queueing

## Data fetching layer (react-query-style)
- ⬜ query caching, staleness, revalidation, dedup
- ⬜ mutations + cache invalidation
- ⬜ optimistic updates + rollback
- ⬜ pagination / infinite queries
- ⬜ background refetch, polling, focus refetch
- 🟡 `Resource`/suspense integration with the reactive layer
- ⬜ offline-first sync primitives

## Serialization & data
- ⬜ serde-based codecs (JSON/MsgPack/CBOR/Protobuf)
- ⬜ schema validation
- ⬜ date/number parsing (locale-aware via i18n)
