# Async, Networking & Data

Match `fetch`/axios/react-query (RN) and `http`/`dio`/`riverpod` (Flutter). ⬜.

## Async runtime
- ✅ `Executor` trait (pluggable); small default executor (not tokio-locked)
- ⬜ UI-thread `spawn_local` + worker-thread `spawn`
- ⬜ wakers marshal completions onto the UI thread (scheduler)
- ✅ debounce/throttle (`debounce<T>(sig, frames) -> Memo<T>`, `throttle<T>(sig, frames) -> Memo<T>` — frame-counter-based); `use_frame_counter() -> Signal<u64>`, `current_frame() -> u64`; ⬜ proper wall-clock timers, intervals, `next_frame`
- ⬜ cancellation tied to component/ownership scopes
- ⬜ structured concurrency helpers

## Networking
- ✅ HTTP client behind a trait (GET/POST/etc., headers, query, body) — ureq-backed `HttpClient`
- ✅ timeouts + retries (`RequestConfig{timeout_secs, retry_count, retry_delay_ms}`; `get_with_config`/`post_with_config`)
- ✅ request interceptors (`add_interceptor(fn(url,headers))` — applied before every request)
- ✅ upload (`upload_bytes(url, data, content_type)` — raw byte POST)
- ⬜ TLS, cert pinning, cookies, redirects, backoff
- ✅ streaming download with progress (`download_with_progress(url, on_progress)` — 8KiB chunks, `DownloadProgress{bytes_received, total_bytes, fraction}`)
- ✅ JSON / typed request-response (`Response::json<T>()`, `Response::json_value()`, `parse_json<T>(response)`, `get_json<T>(url) -> Resource<T>` — serde_json backed)
- ✅ WebSocket client (`connect_ws(url, on_message)` — tungstenite, background thread, send/receive)
- ✅ SSE (`connect_sse(url, on_event)` — ureq BufReader)
- ✅ GraphQL (`graphql(endpoint, query, variables) -> Resource<Response>`)
- ⬜ subscriptions, gRPC
- ⬜ gRPC (optional)
- ⬜ connectivity awareness, offline detection, request queueing

## Data fetching layer (react-query-style)
- ✅ query caching + staleness (`use_query`, `use_query_stale(url, secs)`, `invalidate_query`, `gc_query_cache`)
- ⬜ mutations + cache invalidation
- ⬜ optimistic updates + rollback
- ✅ pagination (`use_paginated(initial_items)` → `Paginated{items,page,loading,has_more}`; `.load_next(fetch_fn)`)
- ⬜ background refetch, polling, focus refetch
- ✅ `Resource`/suspense integration with the reactive layer
- ⬜ offline-first sync primitives

## Serialization & data
- ⬜ serde-based codecs (JSON/MsgPack/CBOR/Protobuf)
- ⬜ schema validation
- ⬜ date/number parsing (locale-aware via i18n)
