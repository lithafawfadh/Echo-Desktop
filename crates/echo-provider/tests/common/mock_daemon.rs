#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{oneshot, Mutex, RwLock};
use tokio::task::JoinHandle;

/// HTTP Request captured by the mock server for client assertion testing.
#[derive(Debug, Clone)]
pub struct RecordedRequest {
    pub method: String,
    pub path: String,
    pub raw_query: Option<String>,
    pub query_params: HashMap<String, String>,
    pub headers: HashMap<String, String>,
}

/// HTTP Response configuration for mock routes.
#[derive(Debug, Clone)]
pub struct MockResponse {
    pub status: u16,
    pub status_text: &'static str,
    pub content_type: &'static str,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub delay: Option<Duration>,
}

impl MockResponse {
    pub fn new(
        status: u16,
        status_text: &'static str,
        content_type: &'static str,
        body: impl Into<String>,
    ) -> Self {
        Self {
            status,
            status_text,
            content_type,
            headers: Vec::new(),
            body: body.into(),
            delay: None,
        }
    }

    /// Creates an HTTP 200 OK JSON response.
    pub fn ok_json(body: impl Into<String>) -> Self {
        Self::new(200, "OK", "application/json", body)
    }

    /// Creates a standardized error response conforming to SPEC.md §4.6.
    pub fn error_json(
        status: u16,
        code: &str,
        message: &str,
        upstream_status: Option<u16>,
        retryable: bool,
    ) -> Self {
        let status_text = match status {
            400 => "Bad Request",
            404 => "Not Found",
            416 => "Range Not Satisfiable",
            429 => "Too Many Requests",
            500 => "Internal Server Error",
            502 => "Bad Gateway",
            503 => "Service Unavailable",
            _ => "Error",
        };

        let body = serde_json::json!({
            "error": {
                "code": code,
                "message": message,
                "upstreamStatus": upstream_status.unwrap_or(status),
                "retryable": retryable
            }
        })
        .to_string();

        Self::new(status, status_text, "application/json", body)
    }

    /// Adds an artificial response delay to simulate network latency or timeouts.
    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = Some(delay);
        self
    }

    /// Adds a custom HTTP header to the mock response.
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((key.into(), value.into()));
        self
    }
}

/// Route lookup table supporting exact path matching and prefix matching.
#[derive(Default)]
pub struct RouteTable {
    pub exact_routes: HashMap<String, MockResponse>,
    pub prefix_routes: Vec<(String, MockResponse)>,
}

/// Fluent builder for constructing and starting a `MockExtensionDaemon`.
pub struct MockDaemonBuilder {
    routes: RouteTable,
}

impl MockDaemonBuilder {
    pub fn new() -> Self {
        Self {
            routes: RouteTable::default(),
        }
    }

    /// Maps an exact path (e.g. `"/v1/health"`) to a mock response.
    pub fn with_route(mut self, path: impl Into<String>, response: MockResponse) -> Self {
        self.routes.exact_routes.insert(path.into(), response);
        self
    }

    /// Maps a path prefix (e.g. `"/v1/search"` or `"/v1/resolve/isrc/"`) to a mock response.
    pub fn with_prefix_route(mut self, prefix: impl Into<String>, response: MockResponse) -> Self {
        self.routes.prefix_routes.push((prefix.into(), response));
        self
    }

    /// Helper: registers a 200 OK response on `GET /v1/health`.
    pub fn with_health_ok(self, body: impl Into<String>) -> Self {
        self.with_route("/v1/health", MockResponse::ok_json(body))
    }

    /// Helper: registers a 200 OK response on `GET /v1/search`.
    pub fn with_search_ok(self, body: impl Into<String>) -> Self {
        self.with_prefix_route("/v1/search", MockResponse::ok_json(body))
    }

    /// Helper: registers a 200 OK response on `GET /v1/resolve/isrc/{isrc}`.
    pub fn with_isrc_ok(self, isrc: &str, body: impl Into<String>) -> Self {
        self.with_route(format!("/v1/resolve/isrc/{}", isrc), MockResponse::ok_json(body))
    }

    /// Helper: registers a 200 OK response on `GET /v1/stream`.
    pub fn with_stream_ok(self, body: impl Into<String>) -> Self {
        self.with_prefix_route("/v1/stream", MockResponse::ok_json(body))
    }

    /// Helper: registers a standardized error on a given path.
    pub fn with_error(
        self,
        path: impl Into<String>,
        status: u16,
        code: &str,
        message: &str,
    ) -> Self {
        self.with_route(
            path,
            MockResponse::error_json(status, code, message, Some(status), false),
        )
    }

    /// Starts the background mock server on an ephemeral OS loopback port.
    pub async fn start(self) -> MockExtensionDaemon {
        MockExtensionDaemon::start_with_routes(self.routes).await
    }
}

/// In-process HTTP mock server simulating local extension microservices.
pub struct MockExtensionDaemon {
    port: u16,
    base_url: String,
    routes: Arc<RwLock<RouteTable>>,
    recorded_requests: Arc<Mutex<Vec<RecordedRequest>>>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    server_handle: Option<JoinHandle<()>>,
}

impl MockExtensionDaemon {
    /// Creates a fluent builder to configure routes before server start.
    pub fn builder() -> MockDaemonBuilder {
        MockDaemonBuilder::new()
    }

    /// Starts an empty mock daemon.
    pub async fn start() -> Self {
        Self::start_with_routes(RouteTable::default()).await
    }

    /// Returns an unallocated / closed port to simulate offline daemon (Connection Refused).
    pub async fn unused_port() -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("Failed to bind ephemeral loopback port");
        let port = listener.local_addr().expect("Local addr").port();
        drop(listener);
        port
    }

    /// Returns an HTTP base URL targeting an offline daemon port (`http://127.0.0.1:{unused_port}/v1`).
    pub async fn unused_base_url() -> String {
        let port = Self::unused_port().await;
        format!("http://127.0.0.1:{}/v1", port)
    }

    /// Internal: binds `127.0.0.1:0` and spawns the connection listener loop.
    async fn start_with_routes(routes: RouteTable) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("Failed to bind ephemeral loopback port for mock daemon");
        let local_addr = listener.local_addr().expect("Local addr");
        let port = local_addr.port();
        let base_url = format!("http://127.0.0.1:{}/v1", port);

        let routes = Arc::new(RwLock::new(routes));
        let recorded_requests = Arc::new(Mutex::new(Vec::new()));
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();

        let routes_clone = Arc::clone(&routes);
        let records_clone = Arc::clone(&recorded_requests);

        let server_handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => {
                        break;
                    }
                    accept_res = listener.accept() => {
                        match accept_res {
                            Ok((mut socket, _peer_addr)) => {
                                let routes = Arc::clone(&routes_clone);
                                let records = Arc::clone(&records_clone);
                                tokio::spawn(async move {
                                    handle_connection(&mut socket, routes, records).await;
                                });
                            }
                            Err(_) => break,
                        }
                    }
                }
            }
        });

        Self {
            port,
            base_url,
            routes,
            recorded_requests,
            shutdown_tx: Some(shutdown_tx),
            server_handle: Some(server_handle),
        }
    }

    /// Returns the dynamically assigned port number.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Returns the base URL with `/v1` suffix (e.g. `"http://127.0.0.1:54321/v1"`).
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Adds or updates an exact route dynamically while the daemon is running.
    pub async fn set_route(&self, path: impl Into<String>, response: MockResponse) {
        let mut routes = self.routes.write().await;
        routes.exact_routes.insert(path.into(), response);
    }

    /// Adds a prefix route dynamically while the daemon is running.
    pub async fn set_prefix_route(&self, prefix: impl Into<String>, response: MockResponse) {
        let mut routes = self.routes.write().await;
        routes.prefix_routes.push((prefix.into(), response));
    }

    /// Returns a snapshot of all HTTP requests received by this daemon.
    pub async fn recorded_requests(&self) -> Vec<RecordedRequest> {
        self.recorded_requests.lock().await.clone()
    }
}

impl Drop for MockExtensionDaemon {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.server_handle.take() {
            handle.abort();
        }
    }
}

/// Handles an individual incoming TCP connection, parses HTTP/1.1 request, and sends response.
async fn handle_connection(
    socket: &mut TcpStream,
    routes: Arc<RwLock<RouteTable>>,
    records: Arc<Mutex<Vec<RecordedRequest>>>,
) {
    let mut buf = Vec::with_capacity(2048);
    let mut chunk = [0u8; 1024];

    loop {
        let n = match socket.read(&mut chunk).await {
            Ok(0) => return,
            Ok(n) => n,
            Err(_) => return,
        };
        buf.extend_from_slice(&chunk[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") || buf.len() >= 16384 {
            break;
        }
    }

    let req_str = String::from_utf8_lossy(&buf);
    let mut lines = req_str.lines();
    let request_line = match lines.next() {
        Some(l) if !l.is_empty() => l,
        _ => return,
    };

    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 2 {
        return;
    }

    let method = parts[0].to_string();
    let full_uri = parts[1];

    let (path, raw_query) = match full_uri.find('?') {
        Some(idx) => (&full_uri[..idx], Some(&full_uri[idx + 1..])),
        None => (full_uri, None),
    };

    let mut query_params = HashMap::new();
    if let Some(q) = raw_query {
        for pair in q.split('&') {
            let mut kv = pair.splitn(2, '=');
            if let (Some(k), Some(v)) = (kv.next(), kv.next()) {
                let decoded_k = percent_encoding::percent_decode_str(k)
                    .decode_utf8_lossy()
                    .replace('+', " ");
                let decoded_v = percent_encoding::percent_decode_str(v)
                    .decode_utf8_lossy()
                    .replace('+', " ");
                query_params.insert(decoded_k, decoded_v);
            }
        }
    }

    let mut headers = HashMap::new();
    for line in lines {
        if line.is_empty() || line == "\r" {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }

    let recorded = RecordedRequest {
        method,
        path: path.to_string(),
        raw_query: raw_query.map(|s| s.to_string()),
        query_params,
        headers,
    };
    records.lock().await.push(recorded);

    // Route matching priority: 1. Exact path match -> 2. Prefix match -> 3. 404 Default
    let response = {
        let table = routes.read().await;
        if let Some(resp) = table.exact_routes.get(path) {
            Some(resp.clone())
        } else {
            table
                .prefix_routes
                .iter()
                .find(|(prefix, _)| path.starts_with(prefix))
                .map(|(_, resp)| resp.clone())
        }
    };

    let final_response = response.unwrap_or_else(|| {
        MockResponse::error_json(
            404,
            "TRACK_NOT_FOUND",
            &format!("Mock route '{}' not found", path),
            Some(404),
            false,
        )
    });

    // Handle artificial latency / timeout simulation
    if let Some(delay) = final_response.delay {
        tokio::time::sleep(delay).await;
    }

    let body_bytes = final_response.body.as_bytes();
    let mut header_buf = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n",
        final_response.status,
        final_response.status_text,
        final_response.content_type,
        body_bytes.len()
    );

    for (k, v) in &final_response.headers {
        header_buf.push_str(&format!("{}: {}\r\n", k, v));
    }
    header_buf.push_str("\r\n");

    let _ = socket.write_all(header_buf.as_bytes()).await;
    let _ = socket.write_all(body_bytes).await;
    let _ = socket.flush().await;
}

// =========================================================================
// Standardized Canonical JSON Response Fixtures (Conforming to SPEC.md §4)
// =========================================================================

impl MockExtensionDaemon {
    /// Canonical healthy response for `GET /v1/health` (SPEC.md §4.2).
    pub fn fixture_health_healthy() -> &'static str {
        r#"{
  "status": "healthy",
  "extension": {
    "name": "echo-flac-lossless-extension",
    "version": "1.4.2",
    "tier": "lossless_master",
    "capabilities": {
      "formats": ["flac", "alac", "wav"],
      "maxBitDepth": 24,
      "maxSampleRate": 192000,
      "supportsIsrcLookup": true,
      "supportsDirectStreaming": true
    }
  }
}"#
    }

    /// Degraded response for `GET /v1/health` (SPEC.md §4.2).
    pub fn fixture_health_degraded() -> &'static str {
        r#"{
  "status": "degraded",
  "extension": {
    "name": "echo-flac-lossless-extension",
    "version": "1.4.2",
    "tier": "lossless_master",
    "capabilities": {
      "formats": ["flac"],
      "maxBitDepth": 16,
      "maxSampleRate": 44100,
      "supportsIsrcLookup": false,
      "supportsDirectStreaming": true
    }
  }
}"#
    }

    /// Canonical search response for `GET /v1/search` (SPEC.md §4.3).
    pub fn fixture_search_getlucky() -> &'static str {
        r#"{
  "results": [
    {
      "id": "ext_dp_getlucky_flac",
      "isrc": "USQX91300105",
      "title": "Get Lucky (feat. Pharrell Williams & Nile Rodgers)",
      "artist": "Daft Punk",
      "album": "Random Access Memories",
      "duration": 369,
      "releaseYear": 2013,
      "thumbnail": "https://img.provider.net/covers/ram_1400.jpg",
      "availableQualities": [
        {
          "format": "flac",
          "bitDepth": 24,
          "sampleRate": 88200,
          "bitrate": 2822,
          "isLossless": true
        },
        {
          "format": "flac",
          "bitDepth": 16,
          "sampleRate": 44100,
          "bitrate": 1411,
          "isLossless": true
        }
      ]
    }
  ]
}"#
    }

    /// Canonical stream response for `GET /v1/resolve/isrc/{isrc}` and `GET /v1/stream` (SPEC.md §4.4, §4.5).
    pub fn fixture_stream_getlucky_flac() -> &'static str {
        r#"{
  "url": "https://cdn.lossless.echomusic.internal/streams/dp_getlucky_24_88200.flac?token=exp17412000_sig99281a",
  "format": "flac",
  "bitDepth": 24,
  "sampleRate": 88200,
  "bitrate": 2822,
  "isLossless": true,
  "fileSizeBytes": 128450120,
  "durationSeconds": 369,
  "expiresAt": 1741200000,
  "headers": {
    "User-Agent": "EchoDesktop/1.0",
    "Accept-Ranges": "bytes"
  }
}"#
    }

    /// Canonical 320kbps MP3 stream response for Tier 2 extension.
    pub fn fixture_stream_320k_mp3() -> &'static str {
        r#"{
  "url": "https://cdn.320k.echomusic.internal/streams/dp_getlucky_320.mp3",
  "format": "mp3",
  "sampleRate": 44100,
  "bitrate": 320,
  "isLossless": false,
  "fileSizeBytes": 14760000,
  "durationSeconds": 369,
  "expiresAt": 1741200000,
  "headers": {
    "User-Agent": "EchoDesktop/1.0",
    "Accept-Ranges": "bytes"
  }
}"#
    }

    /// Canonical error 404 response (SPEC.md §4.6).
    pub fn fixture_error_404_not_found() -> &'static str {
        r#"{
  "error": {
    "code": "TRACK_NOT_FOUND",
    "message": "No lossless stream matching ISRC USQX99999999 found in extension catalog.",
    "upstreamStatus": 404,
    "retryable": false
  }
}"#
    }

    /// Canonical error 416 response (SPEC.md §4.6).
    pub fn fixture_error_416_quality_unavailable() -> &'static str {
        r#"{
  "error": {
    "code": "QUALITY_UNAVAILABLE",
    "message": "Requested quality tier 'lossless_max' unavailable for track.",
    "upstreamStatus": 416,
    "retryable": false
  }
}"#
    }

    /// Canonical error 502 response (SPEC.md §4.6).
    pub fn fixture_error_502_upstream_failed() -> &'static str {
        r#"{
  "error": {
    "code": "UPSTREAM_FAILED",
    "message": "Extension upstream CDN unreachable.",
    "upstreamStatus": 502,
    "retryable": true
  }
}"#
    }
}
