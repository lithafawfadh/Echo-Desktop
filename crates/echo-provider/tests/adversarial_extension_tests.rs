//! Adversarial Verification Test Suite for Milestone 1:
//! Probing `EchoExtensionProvider` and `MockExtensionDaemon` for:
//! 1. Malformed JSON responses, unexpected HTTP status codes (500, 503, 301, 403, 408, 504),
//!    truncated response bodies, and empty JSON objects.
//! 2. High concurrency, thread safety, and socket deadlock freedom.
//! 3. Panic freedom under corrupted server responses, raw wire dropouts, and invalid UTF-8.

mod common;
use common::mock_daemon::{MockExtensionDaemon, MockResponse};

use echo_core::{AudioCodec, AudioFidelity, Track};
use echo_provider::error::ProviderError;
use echo_provider::extension::EchoExtensionProvider;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

// =========================================================================
// Category 1: Malformed JSON Responses & Corrupted Syntaxes
// =========================================================================

#[tokio::test]
async fn test_adv_health_malformed_json_syntax() {
    let daemon = MockExtensionDaemon::builder()
        .with_route(
            "/v1/health",
            MockResponse::ok_json(r#"{"status": "healthy", "broken_json": [1, 2, "#),
        )
        .start()
        .await;

    let provider = EchoExtensionProvider::new(daemon.base_url());
    let err = provider
        .check_health()
        .await
        .expect_err("Malformed JSON syntax on /health must return error");

    match err {
        ProviderError::JsonParse(e) => {
            assert!(e.is_eof() || e.is_syntax(), "Must be a serde_json syntax/eof error: {}", e);
        }
        other => panic!("Expected ProviderError::JsonParse, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_adv_search_malformed_json_syntax() {
    let daemon = MockExtensionDaemon::builder()
        .with_route(
            "/v1/search",
            MockResponse::ok_json(r#"<!DOCTYPE html><html><body>502 Bad Gateway</body></html>"#),
        )
        .start()
        .await;

    let provider = EchoExtensionProvider::new(daemon.base_url());
    let err = provider
        .search("Daft Punk")
        .await
        .expect_err("HTML body on /search 200 OK must fail parsing gracefully");

    match err {
        ProviderError::JsonParse(_) => {}
        other => panic!("Expected ProviderError::JsonParse, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_adv_resolve_isrc_malformed_json_syntax() {
    let daemon = MockExtensionDaemon::builder()
        .with_route(
            "/v1/resolve/isrc/USQX91300105",
            MockResponse::ok_json(r#"{"url": "https://cdn.lossless.test/stream.flac", "bitDepth": "NOT_AN_INT"}"#),
        )
        .start()
        .await;

    let provider = EchoExtensionProvider::new(daemon.base_url());
    let err = provider
        .resolve_isrc("USQX91300105")
        .await
        .expect_err("Type mismatch in JSON on /resolve/isrc must fail parsing");

    match err {
        ProviderError::JsonParse(e) => {
            assert!(e.is_data(), "Must be a serde_json data/type error: {}", e);
        }
        other => panic!("Expected ProviderError::JsonParse, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_adv_stream_malformed_json_syntax() {
    let daemon = MockExtensionDaemon::builder()
        .with_route(
            "/v1/stream",
            MockResponse::ok_json(r#"{"url": "#),
        )
        .start()
        .await;

    let provider = EchoExtensionProvider::new(daemon.base_url());
    let track = Track {
        id: "ext_test".into(),
        title: "Test".into(),
        artist: "Test".into(),
        album: None,
        duration_seconds: 120,
        thumbnail_url: None,
        isrc: None,
    };

    let err = provider
        .resolve_stream(&track)
        .await
        .expect_err("Truncated JSON on /stream must fail gracefully");

    match err {
        ProviderError::JsonParse(_) => {}
        other => panic!("Expected ProviderError::JsonParse, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_adv_empty_string_responses() {
    let daemon = MockExtensionDaemon::builder()
        .with_route("/v1/health", MockResponse::ok_json(""))
        .with_route("/v1/search", MockResponse::ok_json(""))
        .with_route("/v1/stream", MockResponse::ok_json(""))
        .start()
        .await;

    let provider = EchoExtensionProvider::new(daemon.base_url());

    // 1. Health with empty string
    let err_health = provider.check_health().await.unwrap_err();
    assert!(matches!(err_health, ProviderError::JsonParse(_)));

    // 2. Search with empty string
    let err_search = provider.search("test").await.unwrap_err();
    assert!(matches!(err_search, ProviderError::JsonParse(_)));

    // 3. Stream with empty string
    let track = Track {
        id: "empty_test".into(),
        title: "Test".into(),
        artist: "Test".into(),
        album: None,
        duration_seconds: 60,
        thumbnail_url: None,
        isrc: None,
    };
    let err_stream = provider.resolve_stream(&track).await.unwrap_err();
    assert!(matches!(err_stream, ProviderError::JsonParse(_)));
}

// =========================================================================
// Category 2: Empty JSON Objects & Schema Mismatches
// =========================================================================

#[tokio::test]
async fn test_adv_health_empty_json_object() {
    let daemon = MockExtensionDaemon::builder()
        .with_route("/v1/health", MockResponse::ok_json("{}"))
        .start()
        .await;

    let provider = EchoExtensionProvider::new(daemon.base_url());
    let err = provider
        .check_health()
        .await
        .expect_err("Empty JSON object on /health must fail because 'status' is required");

    match err {
        ProviderError::JsonParse(e) => {
            assert!(e.to_string().contains("missing field `status`"));
        }
        other => panic!("Expected ProviderError::JsonParse, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_adv_health_minimal_object_defaults() {
    // Only 'status' is provided; extension, name, version, capabilities omitted
    let daemon = MockExtensionDaemon::builder()
        .with_route(
            "/v1/health",
            MockResponse::ok_json(r#"{"status": "healthy"}"#),
        )
        .start()
        .await;

    let provider = EchoExtensionProvider::new(daemon.base_url());
    let health = provider
        .check_health()
        .await
        .expect("Minimal health JSON must parse with defaults");

    assert_eq!(health.status, "healthy");
    assert!(health.is_healthy());
    assert_eq!(health.name, "");
    assert_eq!(health.version, "");
    assert_eq!(health.tier, "");
    assert_eq!(health.capabilities.max_bit_depth, None);
    assert_eq!(health.capabilities.max_sample_rate, None);
    assert!(!health.capabilities.supports_isrc_lookup);
    assert!(!health.capabilities.supports_direct_streaming);
    assert!(health.capabilities.formats.is_empty());
}

#[tokio::test]
async fn test_adv_search_empty_json_object() {
    let daemon = MockExtensionDaemon::builder()
        .with_route("/v1/search", MockResponse::ok_json("{}"))
        .start()
        .await;

    let provider = EchoExtensionProvider::new(daemon.base_url());
    let results = provider
        .search("nothing")
        .await
        .expect("Empty JSON object on /search should default results to empty vec");

    assert!(results.is_empty(), "Results must default to empty vec");
}

#[tokio::test]
async fn test_adv_search_missing_fields_in_item() {
    // Result item is missing required fields like id, title, artist
    let daemon = MockExtensionDaemon::builder()
        .with_route(
            "/v1/search",
            MockResponse::ok_json(r#"{"results": [{"duration": 180}]}"#),
        )
        .start()
        .await;

    let provider = EchoExtensionProvider::new(daemon.base_url());
    let err = provider
        .search("bad item")
        .await
        .expect_err("Result item missing required fields must fail gracefully");

    match err {
        ProviderError::JsonParse(e) => {
            assert!(e.to_string().contains("missing field"));
        }
        other => panic!("Expected ProviderError::JsonParse, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_adv_stream_empty_json_object() {
    let daemon = MockExtensionDaemon::builder()
        .with_route("/v1/stream", MockResponse::ok_json("{}"))
        .start()
        .await;

    let provider = EchoExtensionProvider::new(daemon.base_url());
    let track = Track {
        id: "empty_obj_test".into(),
        title: "Test".into(),
        artist: "Test".into(),
        album: None,
        duration_seconds: 60,
        thumbnail_url: None,
        isrc: None,
    };

    let err = provider
        .resolve_stream(&track)
        .await
        .expect_err("Empty JSON on /stream must fail because 'url' is required");

    match err {
        ProviderError::JsonParse(e) => {
            assert!(e.to_string().contains("missing field `url`"));
        }
        other => panic!("Expected ProviderError::JsonParse, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_adv_stream_minimal_valid_fields_defaults() {
    // Only 'url' provided, everything else None / omitted
    let daemon = MockExtensionDaemon::builder()
        .with_route(
            "/v1/stream",
            MockResponse::ok_json(r#"{"url": "https://stream.internal/audio"}"#),
        )
        .start()
        .await;

    let provider = EchoExtensionProvider::new(daemon.base_url());
    let track = Track {
        id: "minimal_test".into(),
        title: "Test".into(),
        artist: "Test".into(),
        album: None,
        duration_seconds: 60,
        thumbnail_url: None,
        isrc: None,
    };

    let stream = provider
        .resolve_stream(&track)
        .await
        .expect("Minimal stream JSON must parse cleanly without panic");

    assert_eq!(stream.url, "https://stream.internal/audio");
    assert_eq!(stream.codec, AudioCodec::Unknown);
    assert!(!stream.is_lossless);
    assert_eq!(stream.bitrate_kbps, 320); // Default non-lossless bitrate
    assert_eq!(stream.sample_rate_hz, 44100);
    assert_eq!(stream.bit_depth, None);
    assert_eq!(stream.fidelity, Some(AudioFidelity::Lossy320));
    assert_eq!(stream.content_length_bytes, None);
    assert!(stream.http_headers.is_empty());
}

#[tokio::test]
async fn test_adv_stream_unknown_codec_fallback() {
    let daemon = MockExtensionDaemon::builder()
        .with_route(
            "/v1/stream",
            MockResponse::ok_json(
                r#"{"url": "https://stream.internal/audio", "format": "dsd_direct_stream"}"#,
            ),
        )
        .start()
        .await;

    let provider = EchoExtensionProvider::new(daemon.base_url());
    let track = Track {
        id: "unknown_codec_test".into(),
        title: "Test".into(),
        artist: "Test".into(),
        album: None,
        duration_seconds: 60,
        thumbnail_url: None,
        isrc: None,
    };

    let stream = provider
        .resolve_stream(&track)
        .await
        .expect("Unknown format must resolve to AudioCodec::Unknown");

    assert_eq!(stream.codec, AudioCodec::Unknown);
    assert!(!stream.is_lossless);
}

// =========================================================================
// Category 3: Unexpected HTTP Status Codes (500, 503, 301, 403, 408, 504)
// =========================================================================

#[tokio::test]
async fn test_adv_http_500_internal_server_error_html() {
    let daemon = MockExtensionDaemon::builder()
        .with_route(
            "/v1/health",
            MockResponse::new(
                500,
                "Internal Server Error",
                "text/html",
                "<html><body><h1>500 Internal Server Error</h1></body></html>",
            ),
        )
        .start()
        .await;

    let provider = EchoExtensionProvider::new(daemon.base_url());
    let err = provider.check_health().await.unwrap_err();

    match err {
        ProviderError::ApiError { code, message } => {
            assert_eq!(code, 500);
            assert!(message.contains("500"));
        }
        other => panic!("Expected ProviderError::ApiError, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_adv_http_503_service_unavailable_plaintext() {
    let daemon = MockExtensionDaemon::builder()
        .with_route(
            "/v1/search",
            MockResponse::new(
                503,
                "Service Unavailable",
                "text/plain",
                "Upstream audio cluster down for maintenance",
            ),
        )
        .start()
        .await;

    let provider = EchoExtensionProvider::new(daemon.base_url());
    let err = provider.search("query").await.unwrap_err();

    match err {
        ProviderError::ApiError { code, message } => {
            assert_eq!(code, 503);
            assert!(message.contains("503"));
        }
        other => panic!("Expected ProviderError::ApiError, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_adv_http_301_moved_permanently_no_location() {
    let daemon = MockExtensionDaemon::builder()
        .with_route(
            "/v1/health",
            MockResponse::new(
                301,
                "Moved Permanently",
                "text/html",
                "<html><body>301 Moved Permanently</body></html>",
            ),
        )
        .start()
        .await;

    let provider = EchoExtensionProvider::new(daemon.base_url());
    let err = provider.check_health().await.unwrap_err();

    match err {
        ProviderError::ApiError { code, message } => {
            assert_eq!(code, 301);
            assert!(message.contains("301"));
        }
        other => panic!("Expected ProviderError::ApiError for 301, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_adv_http_403_forbidden() {
    let daemon = MockExtensionDaemon::builder()
        .with_route(
            "/v1/stream",
            MockResponse::new(403, "Forbidden", "application/json", r#"{"error":"Forbidden"}"#),
        )
        .start()
        .await;

    let provider = EchoExtensionProvider::new(daemon.base_url());
    let track = Track {
        id: "forbidden_test".into(),
        title: "Test".into(),
        artist: "Test".into(),
        album: None,
        duration_seconds: 60,
        thumbnail_url: None,
        isrc: None,
    };

    let err = provider.resolve_stream(&track).await.unwrap_err();
    match err {
        ProviderError::ApiError { code, .. } => {
            assert_eq!(code, 403);
        }
        other => panic!("Expected ProviderError::ApiError(403), got: {:?}", other),
    }
}

#[tokio::test]
async fn test_adv_http_408_and_504_timeouts() {
    let daemon = MockExtensionDaemon::builder()
        .with_route(
            "/v1/resolve/isrc/TEST408",
            MockResponse::new(408, "Request Timeout", "text/plain", "Timeout"),
        )
        .with_route(
            "/v1/resolve/isrc/TEST504",
            MockResponse::new(504, "Gateway Timeout", "text/plain", "Gateway Timeout"),
        )
        .start()
        .await;

    let provider = EchoExtensionProvider::new(daemon.base_url());

    // 408
    let err408 = provider.resolve_isrc("TEST408").await.unwrap_err();
    match err408 {
        ProviderError::ApiError { code, .. } => assert_eq!(code, 408),
        other => panic!("Expected ApiError(408), got: {:?}", other),
    }

    // 504
    let err504 = provider.resolve_isrc("TEST504").await.unwrap_err();
    match err504 {
        ProviderError::ApiError { code, .. } => assert_eq!(code, 504),
        other => panic!("Expected ApiError(504), got: {:?}", other),
    }
}

#[tokio::test]
async fn test_adv_http_500_with_structured_extension_error() {
    let daemon = MockExtensionDaemon::builder()
        .with_route(
            "/v1/health",
            MockResponse::new(
                500,
                "Internal Server Error",
                "application/json",
                serde_json::json!({
                    "error": {
                        "code": "INTERNAL_DAEMON_CRASH",
                        "message": "Database thread pool exhausted",
                        "upstreamStatus": 500,
                        "retryable": true
                    }
                })
                .to_string(),
            ),
        )
        .start()
        .await;

    let provider = EchoExtensionProvider::new(daemon.base_url());
    let err = provider.check_health().await.unwrap_err();

    match err {
        ProviderError::ExtensionError {
            code,
            message,
            upstream_status,
            retryable,
        } => {
            assert_eq!(code, "INTERNAL_DAEMON_CRASH");
            assert_eq!(message, "Database thread pool exhausted");
            assert_eq!(upstream_status, Some(500));
            assert!(retryable);
        }
        other => panic!("Expected ProviderError::ExtensionError, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_adv_standard_errors_with_empty_bodies() {
    // When the server returns standard error status codes but NO JSON body,
    // client must fall back to default typed error variants without panic
    // 400 empty body
    let daemon_400 = MockExtensionDaemon::builder()
        .with_route("/v1/search", MockResponse::new(400, "Bad Request", "text/plain", ""))
        .start()
        .await;
    let provider_400 = EchoExtensionProvider::new(daemon_400.base_url());
    let err_empty_400 = provider_400.search("test").await.unwrap_err();
    assert!(matches!(err_empty_400, ProviderError::InvalidParameters(_)));

    // Check resolve_stream on 404 path
    let daemon2 = MockExtensionDaemon::builder()
        .with_route("/v1/stream", MockResponse::new(404, "Not Found", "text/plain", ""))
        .start()
        .await;
    let provider2 = EchoExtensionProvider::new(daemon2.base_url());
    let track = Track {
        id: "t404".into(),
        title: "T".into(),
        artist: "A".into(),
        album: None,
        duration_seconds: 10,
        thumbnail_url: None,
        isrc: None,
    };
    let err_empty_404 = provider2.resolve_stream(&track).await.unwrap_err();
    assert!(err_empty_404.is_not_found());
    assert!(err_empty_404.should_fallback());

    // 416 empty body
    let daemon3 = MockExtensionDaemon::builder()
        .with_route("/v1/stream", MockResponse::new(416, "Range Not Satisfiable", "text/plain", ""))
        .start()
        .await;
    let provider3 = EchoExtensionProvider::new(daemon3.base_url());
    let err_empty_416 = provider3.resolve_stream(&track).await.unwrap_err();
    assert!(err_empty_416.is_quality_unavailable());
    assert!(err_empty_416.should_fallback());

    // 429 empty body
    let daemon4 = MockExtensionDaemon::builder()
        .with_route("/v1/stream", MockResponse::new(429, "Too Many Requests", "text/plain", ""))
        .start()
        .await;
    let provider4 = EchoExtensionProvider::new(daemon4.base_url());
    let err_empty_429 = provider4.resolve_stream(&track).await.unwrap_err();
    assert!(matches!(err_empty_429, ProviderError::RateLimited));

    // 502 empty body
    let daemon5 = MockExtensionDaemon::builder()
        .with_route("/v1/stream", MockResponse::new(502, "Bad Gateway", "text/plain", ""))
        .start()
        .await;
    let provider5 = EchoExtensionProvider::new(daemon5.base_url());
    let err_empty_502 = provider5.resolve_stream(&track).await.unwrap_err();
    assert!(matches!(err_empty_502, ProviderError::UpstreamFailed(_)));
    assert!(err_empty_502.should_fallback());
}

// =========================================================================
// Category 4: Truncated Bodies, Non-UTF8 Bytes, and Wire Dropouts
// =========================================================================

#[tokio::test]
async fn test_adv_truncated_content_length_connection_drop() {
    // Raw TCP listener: sends HTTP header with Content-Length: 10000,
    // but sends only 25 bytes of body and abruptly closes the socket.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        if let Ok((mut socket, _)) = listener.accept().await {
            let mut req_buf = [0u8; 1024];
            let _ = socket.read(&mut req_buf).await;

            let response = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 10000\r\nConnection: close\r\n\r\n{\"status\": \"healt";
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.flush().await;
            // Immediate socket drop!
            drop(socket);
        }
    });

    let base_url = format!("http://127.0.0.1:{}/v1", port);
    let provider = EchoExtensionProvider::new(base_url);
    let err = provider
        .check_health()
        .await
        .expect_err("Truncated HTTP body before Content-Length satisfied must return Err");

    match err {
        ProviderError::Network(e) => {
            assert!(e.is_body() || e.is_decode() || e.to_string().contains("unexpected EOF") || e.to_string().contains("error reading a body"),
                "Expected network/body error, got: {}", e);
        }
        other => panic!("Expected ProviderError::Network, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_adv_immediate_tcp_rst_or_zero_bytes() {
    // Listener accepts connection and closes socket immediately with 0 bytes
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        if let Ok((socket, _)) = listener.accept().await {
            // Immediate drop without writing any response
            drop(socket);
        }
    });

    let base_url = format!("http://127.0.0.1:{}/v1", port);
    let provider = EchoExtensionProvider::new(base_url);
    let err = provider
        .check_health()
        .await
        .expect_err("Immediate TCP closure with 0 bytes must fail gracefully");

    assert!(err.is_connection_failure() || matches!(err, ProviderError::Network(_)));
}

#[tokio::test]
async fn test_adv_non_utf8_binary_body() {
    // Listener returns 200 OK but with non-UTF-8 binary bytes
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        if let Ok((mut socket, _)) = listener.accept().await {
            let mut req_buf = [0u8; 1024];
            let _ = socket.read(&mut req_buf).await;

            let header = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 4\r\nConnection: close\r\n\r\n";
            let _ = socket.write_all(header.as_bytes()).await;
            // Send invalid UTF-8 bytes: [0xFF, 0xFE, 0xFD, 0xFC]
            let _ = socket.write_all(&[0xFF, 0xFE, 0xFD, 0xFC]).await;
            let _ = socket.flush().await;
        }
    });

    let base_url = format!("http://127.0.0.1:{}/v1", port);
    let provider = EchoExtensionProvider::new(base_url);
    let err = provider
        .check_health()
        .await
        .expect_err("Non-UTF8 binary body on JSON endpoint must fail gracefully");

    // reqwest text() fails with decode error, or serde_json fails with parse error
    match err {
        ProviderError::Network(e) => {
            assert!(e.is_decode(), "Must be a decode error: {}", e);
        }
        ProviderError::JsonParse(_) => {}
        other => panic!("Expected Network(decode) or JsonParse, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_adv_large_payload_1mb() {
    // Generate a 1MB search response with hundreds of tracks
    let mut large_results = Vec::new();
    for i in 0..1000 {
        large_results.push(serde_json::json!({
            "id": format!("track_{}", i),
            "isrc": format!("USQX924{:05}", i),
            "title": format!("Large Payload Track Title Number {:05}", i),
            "artist": "Stress Test Symphony Orchestra",
            "album": "Stress Test Symphony Vol 1",
            "duration": 300,
            "releaseYear": 2024,
            "availableQualities": [
                {
                    "format": "flac",
                    "bitDepth": 24,
                    "sampleRate": 192000,
                    "bitrate": 9216,
                    "isLossless": true
                }
            ]
        }));
    }

    let payload = serde_json::json!({ "results": large_results }).to_string();
    assert!(payload.len() > 250_000, "Payload should be substantial");

    let daemon = MockExtensionDaemon::builder()
        .with_search_ok(payload)
        .start()
        .await;

    let provider = EchoExtensionProvider::new(daemon.base_url());
    let tracks = provider
        .search("symphony")
        .await
        .expect("1MB search payload must parse cleanly without memory fault");

    assert_eq!(tracks.len(), 1000);
    assert_eq!(tracks[0].id, "track_0");
    assert_eq!(tracks[999].id, "track_999");
}

// =========================================================================
// Category 5: Concurrency, Thread Safety, and Deadlock Stress Testing
// =========================================================================

#[tokio::test]
async fn test_adv_concurrency_50_parallel_health_checks() {
    let daemon = Arc::new(
        MockExtensionDaemon::builder()
            .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
            .start()
            .await,
    );

    let provider = Arc::new(EchoExtensionProvider::new(daemon.base_url()));
    let concurrency = 50;
    let mut tasks = Vec::with_capacity(concurrency);

    for i in 0..concurrency {
        let p = Arc::clone(&provider);
        tasks.push(tokio::spawn(async move {
            let health = p.check_health().await.unwrap_or_else(|e| {
                panic!("Task {} failed check_health: {:?}", i, e);
            });
            assert!(health.is_healthy());
            assert_eq!(health.name, "echo-flac-lossless-extension");
        }));
    }

    for task in tasks {
        task.await.expect("Task must not panic");
    }

    let recorded = daemon.recorded_requests().await;
    assert_eq!(
        recorded.len(),
        concurrency,
        "Mock daemon must record exactly 50 requests without drops"
    );
}

#[tokio::test]
async fn test_adv_concurrency_mixed_traffic_100_requests() {
    let daemon = Arc::new(
        MockExtensionDaemon::builder()
            .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
            .with_search_ok(MockExtensionDaemon::fixture_search_getlucky())
            .with_stream_ok(MockExtensionDaemon::fixture_stream_getlucky_flac())
            .with_isrc_ok(
                "USQX91300105",
                MockExtensionDaemon::fixture_stream_getlucky_flac(),
            )
            .start()
            .await,
    );

    let provider = Arc::new(EchoExtensionProvider::new(daemon.base_url()));
    let concurrency = 100;
    let mut tasks = Vec::with_capacity(concurrency);

    for i in 0..concurrency {
        let p = Arc::clone(&provider);
        tasks.push(tokio::spawn(async move {
            match i % 4 {
                0 => {
                    let health = p.check_health().await.unwrap();
                    assert!(health.is_healthy());
                }
                1 => {
                    let tracks = p.search(&format!("query_{}", i)).await.unwrap();
                    assert_eq!(tracks.len(), 1);
                }
                2 => {
                    let stream = p.resolve_isrc("USQX91300105").await.unwrap();
                    assert_eq!(stream.codec, AudioCodec::Flac);
                }
                3 => {
                    let track = Track {
                        id: format!("track_{}", i),
                        title: "Title".into(),
                        artist: "Artist".into(),
                        album: None,
                        duration_seconds: 200,
                        thumbnail_url: None,
                        isrc: Some("USQX91300105".into()),
                    };
                    let stream = p.resolve_stream(&track).await.unwrap();
                    assert_eq!(stream.codec, AudioCodec::Flac);
                }
                _ => unreachable!(),
            }
        }));
    }

    for task in tasks {
        task.await.expect("Concurrent task must complete cleanly");
    }

    let recorded = daemon.recorded_requests().await;
    assert_eq!(
        recorded.len(),
        concurrency,
        "All 100 concurrent requests must be recorded by MockExtensionDaemon"
    );
}

#[tokio::test]
async fn test_adv_parallel_daemon_instances_no_collisions() {
    // Spawn 5 independent mock daemons simultaneously
    let mut daemons = Vec::new();
    for _ in 0..5 {
        let d = MockExtensionDaemon::builder()
            .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
            .start()
            .await;
        daemons.push(d);
    }

    // Verify all ports are distinct
    let ports: std::collections::HashSet<u16> = daemons.iter().map(|d| d.port()).collect();
    assert_eq!(ports.len(), 5, "All 5 mock daemon ports must be distinct");

    // Query each daemon concurrently
    let mut tasks = Vec::new();
    for d in daemons {
        tasks.push(tokio::spawn(async move {
            let provider = EchoExtensionProvider::new(d.base_url());
            let health = provider.check_health().await.unwrap();
            assert!(health.is_healthy());
            let rec = d.recorded_requests().await;
            assert_eq!(rec.len(), 1);
        }));
    }

    for task in tasks {
        task.await.unwrap();
    }
}
