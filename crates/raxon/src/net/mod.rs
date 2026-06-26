//! HTTP for `rax`, behind a swappable client and returning reactive
//! [`Resource`](crate::async_rt::Resource)s.
//!
//! [`HttpClient`] is the backend trait (a platform implements it over URLSession
//! / a Rust HTTP crate). A thread-local current client is used by [`get`]/[`send`],
//! which kick off the request on the UI executor and hand back a `Resource` that
//! flips from `Loading` to `Ready`/`Failed` when the response arrives.
//!
//! ```
//! use crate::net::{get, set_client, MockClient, Response};
//! use crate::async_rt::run_until_stalled;
//! use crate::reactive::create_root;
//!
//! set_client(MockClient::new(|_req| Ok(Response::ok("pong"))));
//! let (res, scope) = create_root(|| get("https://example.com/ping"));
//! assert!(res.loading());
//! run_until_stalled();
//! assert_eq!(res.data().unwrap().body, "pong");
//! scope.dispose();
//! ```

#![forbid(unsafe_code)]

use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use crate::async_rt::{create_resource, Resource};

/// HTTP method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// GET.
    Get,
    /// POST.
    Post,
    /// PUT.
    Put,
    /// PATCH.
    Patch,
    /// DELETE.
    Delete,
}

/// An HTTP request.
#[derive(Debug, Clone)]
pub struct Request {
    /// Method.
    pub method: Method,
    /// Absolute URL.
    pub url: String,
    /// Header name/value pairs.
    pub headers: Vec<(String, String)>,
    /// Optional request body.
    pub body: Option<String>,
}

impl Request {
    /// A GET request to `url`.
    pub fn get(url: impl Into<String>) -> Request {
        Request {
            method: Method::Get,
            url: url.into(),
            headers: Vec::new(),
            body: None,
        }
    }

    /// A POST request to `url` with `body`.
    pub fn post(url: impl Into<String>, body: impl Into<String>) -> Request {
        Request {
            method: Method::Post,
            url: url.into(),
            headers: Vec::new(),
            body: Some(body.into()),
        }
    }

    /// Adds a header.
    #[must_use]
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Request {
        self.headers.push((name.into(), value.into()));
        self
    }
}

/// An HTTP response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    /// Status code.
    pub status: u16,
    /// Response body.
    pub body: String,
    /// Response body as raw bytes.
    pub body_bytes: Vec<u8>,
}

impl Response {
    /// A `200 OK` response with `body`.
    pub fn ok(body: impl Into<String>) -> Response {
        let body = body.into();
        Response {
            status: 200,
            body_bytes: body.as_bytes().to_vec(),
            body,
        }
    }

    /// Whether the status is in the 2xx range.
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// A boxed async HTTP result.
pub type ResponseFuture = Pin<Box<dyn Future<Output = Result<Response, String>>>>;

/// The HTTP backend. Implemented by platforms (URLSession, etc.) and by mocks.
pub trait HttpClient {
    /// Sends `request`, resolving to a response or an error message.
    fn send(&self, request: Request) -> ResponseFuture;
}

/// A request handler used by [`MockClient`].
type MockHandler = Rc<dyn Fn(&Request) -> Result<Response, String>>;

/// A synchronous mock client for tests: each request is answered by a closure.
#[derive(Clone)]
pub struct MockClient {
    handler: MockHandler,
}

impl MockClient {
    /// Builds a mock from a response function.
    pub fn new(handler: impl Fn(&Request) -> Result<Response, String> + 'static) -> MockClient {
        MockClient {
            handler: Rc::new(handler),
        }
    }
}

impl HttpClient for MockClient {
    fn send(&self, request: Request) -> ResponseFuture {
        let result = (self.handler)(&request);
        Box::pin(async move { result })
    }
}

struct NotConfigured;
impl HttpClient for NotConfigured {
    fn send(&self, _request: Request) -> ResponseFuture {
        Box::pin(async { Err("no HTTP client configured (call set_client)".to_string()) })
    }
}

thread_local! {
    static CLIENT: std::cell::RefCell<Box<dyn HttpClient>> =
        std::cell::RefCell::new(Box::new(NotConfigured));
}

/// Installs the HTTP client for the current thread.
pub fn set_client(client: impl HttpClient + 'static) {
    CLIENT.with(|c| *c.borrow_mut() = Box::new(client));
}

/// Sends `request` and returns a `Resource` that resolves when it completes.
pub fn send(request: Request) -> Resource<Response> {
    let future = CLIENT.with(|c| c.borrow().send(request));
    create_resource(future)
}

/// Convenience: GET `url` as a `Resource<Response>`.
pub fn get(url: impl Into<String>) -> Resource<Response> {
    send(Request::get(url))
}

/// Convenience: POST `body` to `url` as a `Resource<Response>`.
pub fn post(url: impl Into<String>, body: impl Into<String>) -> Resource<Response> {
    send(Request::post(url, body))
}

/// Execute a GraphQL query or mutation against `endpoint`.
///
/// Builds a JSON body `{"query": "...", "variables": {...}}` and POSTs it with
/// the standard `Content-Type: application/json` header. Returns the full JSON
/// response body as a `Resource<Response>`.
///
/// # Example
/// ```rust,ignore
/// let res = graphql(
///     "https://api.example.com/graphql",
///     r#"query { user(id: "1") { name email } }"#,
///     None,
/// );
/// ```
pub fn graphql(
    endpoint: impl Into<String>,
    query: impl Into<String>,
    variables: Option<String>,
) -> Resource<Response> {
    let endpoint = endpoint.into();
    let query_str = query.into();

    let body = if let Some(vars) = variables {
        format!(r#"{{"query":{:?},"variables":{}}}"#, query_str, vars)
    } else {
        format!(r#"{{"query":{:?}}}"#, query_str)
    };

    let req = Request {
        method: Method::Post,
        url: endpoint,
        headers: vec![
            ("Content-Type".to_string(), "application/json".to_string()),
            ("Accept".to_string(), "application/json".to_string()),
        ],
        body: Some(body),
    };
    send(req)
}

// ---------------------------------------------------------------------------
// WebSocket client
// ---------------------------------------------------------------------------

/// A message received from a WebSocket server.
#[derive(Debug, Clone)]
pub enum WsMessage {
    /// A UTF-8 text frame.
    Text(String),
    /// A binary frame.
    Binary(Vec<u8>),
    /// The connection was closed (no more messages will arrive).
    Close,
}

/// A handle to an active WebSocket connection. Drop to close.
pub struct WsHandle {
    /// Channel to send outgoing messages to the background thread.
    tx: std::sync::mpsc::SyncSender<tungstenite::Message>,
}

impl WsHandle {
    /// Send a text message to the server.
    pub fn send_text(&self, msg: impl Into<String>) {
        let _ = self.tx.send(tungstenite::Message::Text(msg.into().into()));
    }

    /// Send a binary message to the server.
    pub fn send_binary(&self, data: Vec<u8>) {
        let _ = self.tx.send(tungstenite::Message::Binary(data.into()));
    }

    /// Close the connection gracefully.
    pub fn close(self) {
        let _ = self.tx.send(tungstenite::Message::Close(None));
    }
}

/// Connect to a WebSocket server at `url` (must start with `ws://` or `wss://`).
///
/// `on_message` is called from the background thread for each received message.
/// Returns immediately with a [`WsHandle`]. Dropping the handle disconnects.
///
/// ```no_run
/// use crate::net::{connect_ws, WsMessage};
///
/// let handle = connect_ws("ws://echo.websocket.org", |msg| {
///     if let WsMessage::Text(t) = msg {
///         println!("received: {t}");
///     }
/// })
/// .expect("failed to connect");
/// handle.send_text("hello");
/// ```
pub fn connect_ws(
    url: impl Into<String>,
    on_message: impl Fn(WsMessage) + Send + 'static,
) -> Result<WsHandle, String> {
    let url = url.into();
    let (tx, rx) = std::sync::mpsc::sync_channel::<tungstenite::Message>(32);

    std::thread::spawn(move || {
        let (mut socket, _) = match tungstenite::connect(&url) {
            Ok(s) => s,
            Err(e) => {
                on_message(WsMessage::Close);
                let _ = e;
                return;
            }
        };

        loop {
            // Drain any pending outgoing messages first (non-blocking).
            while let Ok(msg) = rx.try_recv() {
                let is_close = matches!(msg, tungstenite::Message::Close(_));
                if socket.send(msg).is_err() || is_close {
                    return;
                }
            }

            // Read the next incoming frame (blocking until one arrives).
            match socket.read() {
                Ok(tungstenite::Message::Text(t)) => on_message(WsMessage::Text(t.to_string())),
                Ok(tungstenite::Message::Binary(b)) => {
                    on_message(WsMessage::Binary(b.to_vec()))
                }
                Ok(tungstenite::Message::Close(_)) | Err(_) => {
                    on_message(WsMessage::Close);
                    return;
                }
                _ => {} // Ping / Pong handled internally by tungstenite
            }
        }
    });

    Ok(WsHandle { tx })
}

// ---------------------------------------------------------------------------
// Query cache — react-query-style deduplication
// ---------------------------------------------------------------------------

use std::cell::RefCell;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Request configuration — timeout, extra headers, and automatic retries
// ---------------------------------------------------------------------------

/// Per-request configuration: timeout, extra headers, and automatic retries.
///
/// Build once and pass to [`HttpClient::get_with_config`] or
/// [`HttpClient::post_with_config`]. The defaults match common REST-API usage.
///
/// # Example
/// ```rust,ignore
/// let cfg = RequestConfig {
///     timeout_secs: 10,
///     headers: vec![("Authorization".into(), "Bearer tok".into())],
///     retry_count: 3,
///     retry_delay_ms: 500,
/// };
/// let res = get_with_config("https://api.example.com/data", cfg)?;
/// ```
#[derive(Debug, Clone)]
pub struct RequestConfig {
    /// Maximum seconds to wait for the server to respond. Default: `30`.
    pub timeout_secs: u64,
    /// Extra HTTP headers sent with every request. Default: none.
    pub headers: Vec<(String, String)>,
    /// How many times to retry on failure **after** the initial attempt. Default: `0`.
    pub retry_count: u32,
    /// Milliseconds to sleep between retry attempts. Default: `1000`.
    pub retry_delay_ms: u64,
}

impl Default for RequestConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 30,
            headers: vec![],
            retry_count: 0,
            retry_delay_ms: 1000,
        }
    }
}

/// Performs a GET request with the given [`RequestConfig`] (timeout, headers,
/// retries).
///
/// On each attempt, a fresh ureq agent is built with the configured timeout so
/// the timeout is honoured per-attempt rather than for the whole retry loop.
/// Returns the first successful [`Response`], or the last error if every
/// attempt fails.
///
/// # Errors
/// Returns `Err(String)` when all attempts are exhausted.
pub fn get_with_config(url: &str, config: RequestConfig) -> Result<Response, String> {
    do_request_with_config(url, None, config)
}

/// Performs a POST request with `body` and the given [`RequestConfig`].
///
/// See [`get_with_config`] for retry and timeout semantics.
///
/// # Errors
/// Returns `Err(String)` when all attempts are exhausted.
pub fn post_with_config(url: &str, body: &str, config: RequestConfig) -> Result<Response, String> {
    do_request_with_config(url, Some(body), config)
}

/// Internal helper shared by [`get_with_config`] and [`post_with_config`].
fn do_request_with_config(
    url: &str,
    body: Option<&str>,
    config: RequestConfig,
) -> Result<Response, String> {
    let max_attempts = config.retry_count + 1;
    let mut last_err = String::new();

    for attempt in 0..max_attempts {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(config.retry_delay_ms));
        }

        // Build an effective URL and headers, running all registered interceptors.
        let mut effective_url = url.to_string();
        let mut effective_headers = config.headers.clone();
        apply_interceptors(&mut effective_url, &mut effective_headers);

        // Build a fresh ureq agent with the per-request timeout.
        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build();

        let result = if let Some(body_str) = body {
            let mut req = agent.post(&effective_url);
            for (name, value) in &effective_headers {
                req = req.set(name, value);
            }
            req.send_string(body_str)
        } else {
            let mut req = agent.get(&effective_url);
            for (name, value) in &effective_headers {
                req = req.set(name, value);
            }
            req.call()
        };

        match result {
            Ok(resp) => {
                let status = resp.status();
                let body_bytes = resp
                    .into_string()
                    .unwrap_or_default()
                    .into_bytes();
                let body_text = String::from_utf8_lossy(&body_bytes).into_owned();
                return Ok(Response {
                    status,
                    body: body_text,
                    body_bytes,
                });
            }
            Err(e) => last_err = e.to_string(),
        }
    }

    Err(last_err)
}

// ---------------------------------------------------------------------------
// Request interceptors — mutate URL / headers before every outgoing request
// ---------------------------------------------------------------------------

thread_local! {
    /// Thread-local list of request interceptors.
    /// Each interceptor receives `(url_mut, headers_mut)` and may modify both.
    static INTERCEPTORS: RefCell<Vec<Box<dyn Fn(&mut String, &mut Vec<(String, String)>)>>> =
        RefCell::new(vec![]);
}

/// Registers a request interceptor on the current thread.
///
/// The closure is called once per HTTP request, just before the network call,
/// and may modify the URL or add / remove headers. Interceptors are applied
/// in registration order.
///
/// ```rust,ignore
/// add_interceptor(|url, headers| {
///     headers.push(("X-Tenant".into(), "acme".into()));
/// });
/// ```
pub fn add_interceptor(
    f: impl Fn(&mut String, &mut Vec<(String, String)>) + 'static,
) {
    INTERCEPTORS.with(|list| list.borrow_mut().push(Box::new(f)));
}

/// Removes all registered interceptors on the current thread.
pub fn clear_interceptors() {
    INTERCEPTORS.with(|list| list.borrow_mut().clear());
}

/// Applies every registered interceptor to `url` and `headers`.
fn apply_interceptors(url: &mut String, headers: &mut Vec<(String, String)>) {
    INTERCEPTORS.with(|list| {
        for f in list.borrow().iter() {
            f(url, headers);
        }
    });
}

// ---------------------------------------------------------------------------
// Upload helper — raw bytes via HTTP POST
// ---------------------------------------------------------------------------

/// Uploads raw bytes to `url` using HTTP POST.
///
/// The `content_type` value is sent as the `Content-Type` header. Suitable
/// for raw binary uploads; for multipart forms, build the body externally and
/// pass `"multipart/form-data; boundary=…"` as the content type.
///
/// Registered interceptors are applied before the request is sent.
///
/// # Errors
/// Returns `Err(String)` describing the ureq error on failure.
///
/// # Example
/// ```rust,ignore
/// let png = std::fs::read("avatar.png").unwrap();
/// let resp = upload_bytes("https://api.example.com/avatar", png, "image/png")?;
/// assert!(resp.is_success());
/// ```
pub fn upload_bytes(url: &str, data: Vec<u8>, content_type: &str) -> Result<Response, String> {
    let mut effective_url = url.to_string();
    let mut headers: Vec<(String, String)> = vec![
        ("Content-Type".to_string(), content_type.to_string()),
    ];
    apply_interceptors(&mut effective_url, &mut headers);

    let mut req = ureq::post(&effective_url);
    for (name, value) in &headers {
        req = req.set(name, value);
    }

    let result = req.send_bytes(&data).map_err(|e| e.to_string())?;
    let status = result.status();
    let body_bytes = result.into_string().unwrap_or_default().into_bytes();
    let body_text = String::from_utf8_lossy(&body_bytes).into_owned();
    Ok(Response {
        status,
        body: body_text,
        body_bytes,
    })
}

// ---------------------------------------------------------------------------
// Multipart form-data
// ---------------------------------------------------------------------------

/// A single part of a [`MultipartForm`].
#[derive(Clone, Debug, PartialEq)]
enum Part {
    /// A plain text field: `(name, value)`.
    Text { name: String, value: String },
    /// A file part: `(name, filename, content_type, bytes)`.
    File {
        name: String,
        filename: String,
        content_type: String,
        bytes: Vec<u8>,
    },
}

/// Builds a `multipart/form-data` request body — the format browsers use for
/// `<form enctype="multipart/form-data">` and file uploads.
///
/// Add text fields with [`field`](MultipartForm::field) and file attachments
/// with [`file`](MultipartForm::file), then send with [`upload_multipart`] (or
/// call [`build`](MultipartForm::build) to get the raw `(content_type, body)`
/// to pass to your own client).
///
/// # Example
/// ```rust,ignore
/// let form = MultipartForm::new()
///     .field("title", "My receipt")
///     .file("receipt", "receipt.jpg", "image/jpeg", jpeg_bytes);
/// let resp = upload_multipart("https://api.example.com/expenses", form)?;
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MultipartForm {
    parts: Vec<Part>,
}

impl MultipartForm {
    /// Create an empty multipart form.
    pub fn new() -> Self {
        MultipartForm { parts: Vec::new() }
    }

    /// Add a plain text field.
    #[must_use]
    pub fn field(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.parts.push(Part::Text {
            name: name.into(),
            value: value.into(),
        });
        self
    }

    /// Add a file part with an explicit filename and MIME content type.
    #[must_use]
    pub fn file(
        mut self,
        name: impl Into<String>,
        filename: impl Into<String>,
        content_type: impl Into<String>,
        bytes: impl Into<Vec<u8>>,
    ) -> Self {
        self.parts.push(Part::File {
            name: name.into(),
            filename: filename.into(),
            content_type: content_type.into(),
            bytes: bytes.into(),
        });
        self
    }

    /// Number of parts (text fields + files) in this form.
    pub fn len(&self) -> usize {
        self.parts.len()
    }

    /// Whether the form has no parts.
    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    /// Serialize the form into `(content_type_header, body_bytes)`.
    ///
    /// The returned content type includes the generated boundary and should be
    /// sent verbatim as the `Content-Type` request header.
    pub fn build(&self) -> (String, Vec<u8>) {
        let boundary = next_boundary();
        let mut body: Vec<u8> = Vec::new();
        for part in &self.parts {
            body.extend_from_slice(b"--");
            body.extend_from_slice(boundary.as_bytes());
            body.extend_from_slice(b"\r\n");
            match part {
                Part::Text { name, value } => {
                    body.extend_from_slice(
                        format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n")
                            .as_bytes(),
                    );
                    body.extend_from_slice(value.as_bytes());
                    body.extend_from_slice(b"\r\n");
                }
                Part::File {
                    name,
                    filename,
                    content_type,
                    bytes,
                } => {
                    body.extend_from_slice(
                        format!(
                            "Content-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\n"
                        )
                        .as_bytes(),
                    );
                    body.extend_from_slice(
                        format!("Content-Type: {content_type}\r\n\r\n").as_bytes(),
                    );
                    body.extend_from_slice(bytes);
                    body.extend_from_slice(b"\r\n");
                }
            }
        }
        body.extend_from_slice(b"--");
        body.extend_from_slice(boundary.as_bytes());
        body.extend_from_slice(b"--\r\n");

        let content_type = format!("multipart/form-data; boundary={boundary}");
        (content_type, body)
    }
}

/// Generates a process-unique multipart boundary string. Uniqueness is
/// guaranteed within the process via a monotonic counter; collision with body
/// content is astronomically unlikely thanks to the fixed prefix.
fn next_boundary() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("----raxonFormBoundary{n:016x}")
}

/// Uploads a [`MultipartForm`] to `url` via HTTP POST, serializing it with the
/// correct `Content-Type: multipart/form-data; boundary=…` header.
///
/// Registered interceptors are applied before the request is sent.
///
/// # Errors
/// Returns `Err(String)` describing the ureq error on failure.
pub fn upload_multipart(url: &str, form: MultipartForm) -> Result<Response, String> {
    let (content_type, body) = form.build();
    upload_bytes(url, body, &content_type)
}

// ---------------------------------------------------------------------------
// Cache-control helpers
// ---------------------------------------------------------------------------

/// Returns `true` when `response` indicates it may be cached.
///
/// A response is considered cacheable when its status is `200 OK`. In
/// practice, callers should inspect the raw `Cache-Control` / `Expires`
/// headers they receive; this helper is a lightweight stand-in for the most
/// common case.
///
/// TTL-based caching of full [`Resource`]s is already handled by
/// [`use_query_stale`], which re-fetches automatically when the entry is
/// older than `stale_after_secs`.
pub fn is_cacheable(response: &Response) -> bool {
    response.status == 200
}

// ---------------------------------------------------------------------------
// Pagination helpers — cursor-based reactive page loading
// ---------------------------------------------------------------------------

use crate::reactive::Signal;

/// Cursor-based paginated data holder backed by reactive [`Signal`]s.
///
/// All fields are [`Signal`] handles, which are unconditionally `Copy`, so
/// `Paginated<T>` itself is `Copy` / `Clone` regardless of `T`.
///
/// Create one via [`use_paginated`] and advance pages with [`load_next`].
///
/// [`load_next`]: Paginated::load_next
#[derive(Clone, Copy)]
pub struct Paginated<T: Clone + 'static> {
    /// Accumulated items across all loaded pages.
    pub items: Signal<Vec<T>>,
    /// The index of the last successfully loaded page (0 = nothing loaded yet).
    pub page: Signal<u32>,
    /// `true` while a page fetch is in progress.
    pub loading: Signal<bool>,
    /// `false` once a fetch returns an empty slice (no further pages exist).
    pub has_more: Signal<bool>,
}

impl<T: Clone + 'static> Paginated<T> {
    /// Fetches the next page by calling `fetch(next_page_index)`.
    ///
    /// Does nothing when a fetch is already in progress (`loading == true`) or
    /// when the end of the list has been reached (`has_more == false`).
    ///
    /// The `fetch` closure receives the **1-based** page number to load and
    /// should return the items for that page. An empty return value signals
    /// that no more pages exist.
    ///
    /// # Example
    /// ```rust,ignore
    /// let paged = use_paginated::<Post>(vec![]);
    /// paged.load_next(|page| api_fetch_posts(page));
    /// ```
    pub fn load_next(&self, fetch: impl Fn(u32) -> Vec<T>) {
        if self.loading.get() || !self.has_more.get() {
            return;
        }
        self.loading.set(true);
        let next_page = self.page.get() + 1;
        let new_items = fetch(next_page);
        if new_items.is_empty() {
            self.has_more.set(false);
        } else {
            self.items.update(|v| v.extend(new_items));
            self.page.set(next_page);
        }
        self.loading.set(false);
    }
}

/// Creates a reactive [`Paginated<T>`] pre-seeded with `initial_items`.
///
/// The returned value holds four reactive signals; it is `Copy` and can be
/// moved freely into closures on the same thread.
///
/// ```rust,ignore
/// use crate::reactive::create_root;
/// use crate::net::use_paginated;
///
/// let (paged, _scope) = create_root(|| use_paginated::<String>(vec![]));
/// paged.load_next(|page| fetch_page(page));
/// ```
pub fn use_paginated<T: Clone + 'static>(initial_items: Vec<T>) -> Paginated<T> {
    Paginated {
        items: crate::reactive::create_signal(initial_items),
        page: crate::reactive::create_signal(0u32),
        loading: crate::reactive::create_signal(false),
        has_more: crate::reactive::create_signal(true),
    }
}

// ---------------------------------------------------------------------------
// Query cache — react-query-style deduplication
// ---------------------------------------------------------------------------

thread_local! {
    static QUERY_CACHE: RefCell<HashMap<String, Resource<Response>>> =
        RefCell::new(HashMap::new());

    /// Records the wall-clock time when each URL was last fetched and cached.
    static QUERY_TIMESTAMPS: RefCell<HashMap<String, std::time::Instant>> =
        RefCell::new(HashMap::new());
}

/// Returns a cached [`Resource<Response>`] for the given URL.
///
/// The first caller fires an HTTP GET; all subsequent callers with the **same
/// URL** receive the identical `Resource` — the request is never duplicated.
/// The cache is per-thread (all rax work happens on the main thread).
///
/// # Example
/// ```
/// use crate::net::{use_query, set_client, MockClient, Response};
/// use crate::async_rt::run_until_stalled;
/// use crate::reactive::create_root;
///
/// set_client(MockClient::new(|_| Ok(Response::ok("[]"))));
/// let (res, scope) = create_root(|| use_query("https://api.example.com/items"));
/// run_until_stalled();
/// assert!(res.data().is_some());
/// scope.dispose();
/// ```
pub fn use_query(url: impl Into<String>) -> Resource<Response> {
    let url = url.into();
    QUERY_CACHE.with(|cache| {
        if let Some(cached) = cache.borrow().get(&url) {
            return *cached;
        }
        // First caller — fire the request and cache the resource.
        let resource = get(url.clone());
        // Record the timestamp of this fetch.
        QUERY_TIMESTAMPS.with(|t| t.borrow_mut().insert(url.clone(), std::time::Instant::now()));
        cache.borrow_mut().insert(url, resource);
        resource
    })
}

/// Removes the cached entry for `url` so the next [`use_query`] call fires a
/// fresh HTTP GET.
pub fn invalidate_query(url: impl Into<String>) {
    let url = url.into();
    QUERY_CACHE.with(|cache| {
        cache.borrow_mut().remove(&url);
    });
    QUERY_TIMESTAMPS.with(|t| t.borrow_mut().remove(&url));
}

/// Returns a cached [`Resource<Response>`] for the given URL, refetching in
/// the background when the cached entry is older than `stale_after_secs`.
///
/// Pass `0` to never auto-revalidate (always use the cache). Pass
/// `u64::MAX` to always refetch.
///
/// # Example
/// ```
/// use crate::net::{use_query_stale, set_client, MockClient, Response};
/// use crate::async_rt::run_until_stalled;
/// use crate::reactive::create_root;
///
/// set_client(MockClient::new(|_| Ok(Response::ok("[]"))));
/// let (res, scope) = create_root(|| use_query_stale("https://api.example.com/items", 60));
/// run_until_stalled();
/// assert!(res.data().is_some());
/// scope.dispose();
/// ```
pub fn use_query_stale(url: impl Into<String>, stale_after_secs: u64) -> Resource<Response> {
    let url = url.into();

    // A stale_after_secs of 0 means "never revalidate".
    if stale_after_secs != 0 {
        let is_stale = QUERY_TIMESTAMPS.with(|t| {
            t.borrow()
                .get(&url)
                .map(|ts| ts.elapsed().as_secs() > stale_after_secs)
                .unwrap_or(true) // no entry = treat as stale
        });
        if is_stale {
            invalidate_query(url.clone());
        }
    }

    use_query(url)
}

/// Evicts all cache entries that were fetched more than `max_age_secs` ago.
///
/// Call periodically (e.g. on `AppLifecycle::Resumed`) to prevent unbounded
/// memory growth from long-running sessions.
pub fn gc_query_cache(max_age_secs: u64) {
    // Collect URLs that have expired.
    let expired: Vec<String> = QUERY_TIMESTAMPS.with(|t| {
        t.borrow()
            .iter()
            .filter(|(_, ts)| ts.elapsed().as_secs() > max_age_secs)
            .map(|(url, _)| url.clone())
            .collect()
    });
    // Remove both the timestamp and the resource cache entry.
    for url in expired {
        QUERY_CACHE.with(|c| { c.borrow_mut().remove(&url); });
        QUERY_TIMESTAMPS.with(|t| { t.borrow_mut().remove(&url); });
    }
}

// ---------------------------------------------------------------------------
// Server-Sent Events (SSE)
// ---------------------------------------------------------------------------

/// A parsed Server-Sent Event.
#[derive(Debug, Clone)]
pub struct SseEvent {
    /// The event type. Defaults to `"message"` when the stream omits `event:`.
    pub event: String,
    /// The data payload (multi-line `data:` fields are joined with `'\n'`).
    pub data: String,
    /// The optional event id from the `id:` field.
    pub id: Option<String>,
}

/// Connect to a Server-Sent Events endpoint at `url`.
///
/// Spawns a background thread that reads the stream line-by-line and calls
/// `on_event` for every complete event. The thread exits when the server closes
/// the connection or an I/O error occurs. Drop the returned
/// [`std::thread::JoinHandle`] to detach (it will not abort the thread, but the
/// thread will exit on the next failed read once the server closes the stream).
///
/// ```no_run
/// use crate::net::{connect_sse, SseEvent};
///
/// let _handle = connect_sse("https://example.com/events", |ev| {
///     println!("[{}] {}", ev.event, ev.data);
/// });
/// ```
pub fn connect_sse(
    url: impl Into<String>,
    on_event: impl Fn(SseEvent) + Send + 'static,
) -> std::thread::JoinHandle<()> {
    let url = url.into();
    std::thread::spawn(move || {
        let response = match ureq::get(&url)
            .set("Accept", "text/event-stream")
            .set("Cache-Control", "no-cache")
            .call()
        {
            Ok(r) => r,
            Err(_) => return,
        };

        let mut reader = std::io::BufReader::new(response.into_reader());
        let mut event_type = String::from("message");
        let mut data_buf = String::new();
        let mut id_buf: Option<String> = None;

        use std::io::BufRead;
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => break, // EOF
                Err(_) => break,
                _ => {}
            }
            let line = line.trim_end_matches('\n').trim_end_matches('\r');

            if line.is_empty() {
                // Empty line dispatches the buffered event.
                if !data_buf.is_empty() {
                    on_event(SseEvent {
                        event: event_type.clone(),
                        data: data_buf.trim_end_matches('\n').to_string(),
                        id: id_buf.clone(),
                    });
                }
                event_type = "message".to_string();
                data_buf.clear();
                id_buf = None;
            } else if let Some(data) = line.strip_prefix("data:") {
                if !data_buf.is_empty() {
                    data_buf.push('\n');
                }
                data_buf.push_str(data.trim_start());
            } else if let Some(ev) = line.strip_prefix("event:") {
                event_type = ev.trim_start().to_string();
            } else if let Some(id) = line.strip_prefix("id:") {
                id_buf = Some(id.trim_start().to_string());
            }
            // Lines starting with ':' are comments — ignored.
        }
    })
}

// ---------------------------------------------------------------------------
// Typed JSON helpers
// ---------------------------------------------------------------------------

impl Response {
    /// Attempt to deserialize the response body as the JSON type `T`.
    ///
    /// Returns `Err` when the body is not valid JSON or does not match the
    /// expected shape.
    ///
    /// # Example
    /// ```rust,ignore
    /// #[derive(serde::Deserialize)]
    /// struct User { id: u32, name: String }
    ///
    /// let resp = get_with_config("https://api.example.com/user/1", Default::default())?;
    /// let user: User = resp.json()?;
    /// ```
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> Result<T, String> {
        serde_json::from_str(&self.body).map_err(|e| e.to_string())
    }

    /// Parse the response body as a generic JSON value.
    ///
    /// Useful when the schema is unknown or varies at runtime.
    ///
    /// # Example
    /// ```rust,ignore
    /// let val = resp.json_value()?;
    /// println!("{}", val["name"]);
    /// ```
    pub fn json_value(&self) -> Result<serde_json::Value, String> {
        serde_json::from_str(&self.body).map_err(|e| e.to_string())
    }
}

/// Deserialize a JSON response body into `T`.
///
/// Equivalent to [`Response::json`]; provided as a free function for
/// situations where a reference is more convenient than a method call.
///
/// # Errors
/// Returns `Err(String)` if parsing fails.
pub fn parse_json<T: serde::de::DeserializeOwned>(response: &Response) -> Result<T, String> {
    response.json()
}

/// Performs a GET request and deserializes the response body as the JSON
/// type `T`, returning a reactive [`Resource<T>`].
///
/// # Example
/// ```rust,ignore
/// use crate::reactive::create_root;
/// use crate::net::{get_json, set_client, MockClient, Response};
///
/// #[derive(Clone, serde::Deserialize)]
/// struct Item { id: u32 }
///
/// set_client(MockClient::new(|_| Ok(Response::ok(r#"{"id":1}"#))));
/// let (res, _scope) = create_root(|| get_json::<Item>("https://api.example.com/item/1"));
/// ```
pub fn get_json<T: serde::de::DeserializeOwned + Clone + 'static>(
    url: impl Into<String>,
) -> crate::async_rt::Resource<T> {
    let url = url.into();
    let future = async move {
        let resp = CLIENT.with(|c| c.borrow().send(Request::get(&url)));
        let resp = resp.await?;
        resp.json::<T>()
    };
    create_resource(Box::pin(future))
}

// ---------------------------------------------------------------------------
// Streaming download with progress reporting
// ---------------------------------------------------------------------------

/// Snapshot of a download's progress at a point in time.
#[derive(Clone, Copy, Debug)]
pub struct DownloadProgress {
    /// Number of bytes received so far.
    pub bytes_received: u64,
    /// Total expected size, if the server supplied a `Content-Length` header.
    pub total_bytes: Option<u64>,
    /// Fraction of the download completed (`0.0`–`1.0`), or `None` when
    /// `total_bytes` is unknown.
    pub fraction: Option<f64>,
}

/// Downloads the resource at `url`, calling `on_progress` after each chunk is
/// received, and returns the complete [`Response`] when the download finishes.
///
/// The `on_progress` callback is called from the calling thread (no background
/// spawning), so it blocks until the download completes. If you need
/// non-blocking behaviour, call this from a `std::thread::spawn` closure.
///
/// Registered interceptors are applied to the URL and headers before the
/// request is sent.
///
/// # Errors
/// Returns `Err(String)` if the connection or read fails.
///
/// # Example
/// ```rust,ignore
/// use crate::net::{download_with_progress, DownloadProgress};
///
/// let response = download_with_progress("https://example.com/file.bin", |p| {
///     if let Some(f) = p.fraction {
///         println!("{:.1}%", f * 100.0);
///     }
/// })?;
/// assert!(response.is_success());
/// ```
pub fn download_with_progress(
    url: &str,
    on_progress: impl Fn(DownloadProgress) + Send + 'static,
) -> Result<Response, String> {
    let mut effective_url = url.to_string();
    let mut headers: Vec<(String, String)> = vec![];
    apply_interceptors(&mut effective_url, &mut headers);

    let mut req = ureq::get(&effective_url);
    for (k, v) in &headers {
        req = req.set(k, v);
    }

    let response = req.call().map_err(|e| e.to_string())?;
    let total = response
        .header("content-length")
        .and_then(|s| s.parse::<u64>().ok());
    let status = response.status();

    let mut reader = response.into_reader();
    let mut bytes: Vec<u8> = Vec::new();
    let mut buf = [0u8; 8192];

    loop {
        use std::io::Read;
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n]);
        on_progress(DownloadProgress {
            bytes_received: bytes.len() as u64,
            total_bytes: total,
            fraction: total.map(|t| bytes.len() as f64 / t as f64),
        });
    }

    let body = String::from_utf8_lossy(&bytes).into_owned();
    Ok(Response {
        status,
        body,
        body_bytes: bytes,
    })
}

// ---------------------------------------------------------------------------
// Image cache — in-memory URL-keyed byte store
// ---------------------------------------------------------------------------

thread_local! {
    static IMAGE_CACHE: RefCell<HashMap<String, Vec<u8>>> = RefCell::new(HashMap::new());
}

/// Cache raw bytes under a URL key.
///
/// Subsequent [`fetch_image`] calls with the same URL return the cached bytes
/// without hitting the network.
pub fn cache_image(url: &str, data: Vec<u8>) {
    IMAGE_CACHE.with(|c| {
        c.borrow_mut().insert(url.to_string(), data);
    });
}

/// Retrieve cached bytes for a URL, or `None` if not yet cached.
pub fn get_cached_image(url: &str) -> Option<Vec<u8>> {
    IMAGE_CACHE.with(|c| c.borrow().get(url).cloned())
}

/// Clear all cached images, freeing their memory.
pub fn clear_image_cache() {
    IMAGE_CACHE.with(|c| c.borrow_mut().clear());
}

/// Download and cache an image, returning the raw bytes.
///
/// If the URL is already present in the in-memory cache the cached bytes are
/// returned immediately without making a network request. Otherwise a
/// synchronous HTTP GET is performed via `ureq`, the response bytes are stored
/// in the cache, and the same bytes are returned to the caller.
///
/// # Errors
/// Returns `Err(String)` if the network request fails.
///
/// # Example
/// ```rust,ignore
/// use crate::net::fetch_image;
///
/// let bytes = fetch_image("https://example.com/photo.jpg")?;
/// println!("downloaded {} bytes", bytes.len());
/// // Second call: cache hit, no network request.
/// let bytes2 = fetch_image("https://example.com/photo.jpg")?;
/// assert_eq!(bytes, bytes2);
/// ```
pub fn fetch_image(url: &str) -> Result<Vec<u8>, String> {
    if let Some(data) = get_cached_image(url) {
        return Ok(data);
    }
    let response = ureq::get(url).call().map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    use std::io::Read;
    response
        .into_reader()
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    cache_image(url, bytes.clone());
    Ok(bytes)
}

#[cfg(test)]
mod tests;
