mod common;
use common::mock_daemon::{MockExtensionDaemon, MockResponse};

use echo_core::{AudioCodec, AudioFidelity, Track};
use echo_provider::error::ProviderError;
use echo_provider::extension::EchoExtensionProvider;
use std::time::Duration;

/// Verifies that `GET /v1/health` correctly parses capability discovery metadata.
#[tokio::test]
async fn test_extension_health_check_success() {
    let daemon = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .start()
        .await;

    let provider = EchoExtensionProvider::new(daemon.base_url());
    let health = provider
        .check_health()
        .await
        .expect("Health check should succeed against mock daemon");

    assert_eq!(health.status, "healthy");
    assert!(health.is_healthy());
    assert_eq!(health.name, "echo-flac-lossless-extension");
    assert_eq!(health.version, "1.4.2");
    assert_eq!(health.tier, "lossless_master");
    assert_eq!(health.capabilities.max_bit_depth, Some(24));
    assert_eq!(health.capabilities.max_sample_rate, Some(192000));
    assert!(health.capabilities.supports_isrc_lookup);
    assert!(health.capabilities.supports_direct_streaming);
    assert!(health.capabilities.formats.contains(&"flac".to_string()));
    assert!(health.capabilities.formats.contains(&"alac".to_string()));
    assert!(health.capabilities.formats.contains(&"wav".to_string()));
}

/// Verifies that degraded daemon status correctly reports `is_healthy() == false`.
#[tokio::test]
async fn test_extension_health_check_degraded() {
    let daemon = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_degraded())
        .start()
        .await;

    let provider = EchoExtensionProvider::new(daemon.base_url());
    let health = provider
        .check_health()
        .await
        .expect("Health check network call should succeed");

    assert_eq!(health.status, "degraded");
    assert!(
        !health.is_healthy(),
        "Degraded status must evaluate is_healthy() == false to bypass tier"
    );
    assert_eq!(health.capabilities.max_bit_depth, Some(16));
    assert_eq!(health.capabilities.max_sample_rate, Some(44100));
    assert!(!health.capabilities.supports_isrc_lookup);
}

/// Verifies that `GET /v1/search` correctly parses catalog search results and query parameters.
#[tokio::test]
async fn test_extension_search_tracks() {
    let daemon = MockExtensionDaemon::builder()
        .with_search_ok(MockExtensionDaemon::fixture_search_getlucky())
        .start()
        .await;

    let provider = EchoExtensionProvider::new(daemon.base_url());
    let tracks = provider
        .search("Daft Punk")
        .await
        .expect("Search request should succeed");

    assert_eq!(tracks.len(), 1);
    let track = &tracks[0];
    assert_eq!(track.id, "ext_dp_getlucky_flac");
    assert_eq!(track.title, "Get Lucky (feat. Pharrell Williams & Nile Rodgers)");
    assert_eq!(track.artist, "Daft Punk");
    assert_eq!(track.album, Some("Random Access Memories".to_string()));
    assert_eq!(track.duration_seconds, 369);
    assert_eq!(track.isrc, Some("USQX91300105".to_string()));
    assert_eq!(
        track.thumbnail_url,
        Some("https://img.provider.net/covers/ram_1400.jpg".to_string())
    );

    // Verify request parameters received by mock daemon
    let recorded = daemon.recorded_requests().await;
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].path, "/v1/search");
    assert_eq!(
        recorded[0].query_params.get("q").map(|s| s.as_str()),
        Some("Daft Punk")
    );
    assert_eq!(
        recorded[0].query_params.get("type").map(|s| s.as_str()),
        Some("track")
    );
    assert_eq!(
        recorded[0].query_params.get("limit").map(|s| s.as_str()),
        Some("10")
    );
}

/// Verifies that `GET /v1/resolve/isrc/{isrc}` performs unambiguous exact ISRC resolution.
#[tokio::test]
async fn test_extension_resolve_isrc_success() {
    let daemon = MockExtensionDaemon::builder()
        .with_isrc_ok(
            "USQX91300105",
            MockExtensionDaemon::fixture_stream_getlucky_flac(),
        )
        .start()
        .await;

    let provider = EchoExtensionProvider::new(daemon.base_url());
    let stream = provider
        .resolve_isrc("USQX91300105")
        .await
        .expect("ISRC resolution should succeed");

    assert_eq!(
        stream.url,
        "https://cdn.lossless.echomusic.internal/streams/dp_getlucky_24_88200.flac?token=exp17412000_sig99281a"
    );
    assert_eq!(stream.codec, AudioCodec::Flac);
    assert_eq!(stream.bitrate_kbps, 2822);
    assert_eq!(stream.sample_rate_hz, 88200);
    assert!(stream.is_lossless);
    assert_eq!(stream.bit_depth, Some(24));
    assert_eq!(stream.fidelity, Some(AudioFidelity::LosslessHiRes));
    assert_eq!(stream.content_length_bytes, Some(128450120));
    assert_eq!(
        stream.http_headers.get("User-Agent"),
        Some(&"EchoDesktop/1.0".to_string())
    );
    assert_eq!(
        stream.http_headers.get("Accept-Ranges"),
        Some(&"bytes".to_string())
    );
}

/// Verifies that `GET /v1/stream` resolves playable streams with quality parameters.
#[tokio::test]
async fn test_extension_resolve_stream_fallback() {
    let daemon = MockExtensionDaemon::builder()
        .with_stream_ok(MockExtensionDaemon::fixture_stream_getlucky_flac())
        .start()
        .await;

    let provider = EchoExtensionProvider::new(daemon.base_url());
    let track = Track {
        id: "ext_dp_getlucky_flac".into(),
        title: "Get Lucky".into(),
        artist: "Daft Punk".into(),
        album: Some("Random Access Memories".into()),
        duration_seconds: 369,
        thumbnail_url: None,
        isrc: Some("USQX91300105".into()),
    };

    let stream = provider
        .resolve_stream(&track)
        .await
        .expect("Stream resolve should succeed");

    assert_eq!(stream.codec, AudioCodec::Flac);
    assert!(stream.is_lossless);
    assert_eq!(stream.bit_depth, Some(24));
    assert_eq!(stream.sample_rate_hz, 88200);
    assert_eq!(stream.fidelity, Some(AudioFidelity::LosslessHiRes));

    // Verify query parameters sent to /v1/stream
    let recorded = daemon.recorded_requests().await;
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].path, "/v1/stream");
    assert_eq!(
        recorded[0].query_params.get("id").map(|s| s.as_str()),
        Some("ext_dp_getlucky_flac")
    );
    assert_eq!(
        recorded[0].query_params.get("isrc").map(|s| s.as_str()),
        Some("USQX91300105")
    );
    assert_eq!(
        recorded[0].query_params.get("quality").map(|s| s.as_str()),
        Some("lossless_max")
    );
}

/// Verifies that HTTP 404 responses map to `ProviderError::TrackNotFound` with fallback predicate true.
#[tokio::test]
async fn test_extension_error_404_track_not_found() {
    let daemon = MockExtensionDaemon::builder()
        .with_route(
            "/v1/resolve/isrc/UNKNOWN_ISRC",
            MockResponse::new(
                404,
                "Not Found",
                "application/json",
                MockExtensionDaemon::fixture_error_404_not_found(),
            ),
        )
        .start()
        .await;

    let provider = EchoExtensionProvider::new(daemon.base_url());
    let err = provider
        .resolve_isrc("UNKNOWN_ISRC")
        .await
        .expect_err("Must return Err on 404");

    assert!(err.is_not_found(), "Error must be recognized as is_not_found()");
    assert!(err.should_fallback(), "Error must indicate router should fallback");
    match err {
        ProviderError::TrackNotFound(msg) => {
            assert!(msg.contains("No lossless stream matching ISRC"));
        }
        other => panic!("Expected ProviderError::TrackNotFound, got: {:?}", other),
    }
}

/// Verifies that HTTP 416 responses map to `ProviderError::QualityUnavailable` with fallback predicate true.
#[tokio::test]
async fn test_extension_error_416_quality_unavailable() {
    let daemon = MockExtensionDaemon::builder()
        .with_route(
            "/v1/stream",
            MockResponse::new(
                416,
                "Range Not Satisfiable",
                "application/json",
                MockExtensionDaemon::fixture_error_416_quality_unavailable(),
            ),
        )
        .start()
        .await;

    let provider = EchoExtensionProvider::new(daemon.base_url());
    let track = Track {
        id: "unavailable_quality_track".into(),
        title: "Test Track".into(),
        artist: "Test Artist".into(),
        album: None,
        duration_seconds: 200,
        thumbnail_url: None,
        isrc: None,
    };

    let err = provider
        .resolve_stream(&track)
        .await
        .expect_err("Must return Err on 416");

    assert!(
        err.is_quality_unavailable(),
        "Error must be recognized as is_quality_unavailable()"
    );
    assert!(err.should_fallback(), "Error must indicate router should fallback");
    match err {
        ProviderError::QualityUnavailable(msg) => {
            assert!(msg.contains("Requested quality tier 'lossless_max' unavailable"));
        }
        other => panic!("Expected ProviderError::QualityUnavailable, got: {:?}", other),
    }
}

/// Verifies that HTTP 502 responses map to `ProviderError::UpstreamFailed` with fallback predicate true.
#[tokio::test]
async fn test_extension_error_502_upstream_failed() {
    let daemon = MockExtensionDaemon::builder()
        .with_route(
            "/v1/stream",
            MockResponse::new(
                502,
                "Bad Gateway",
                "application/json",
                MockExtensionDaemon::fixture_error_502_upstream_failed(),
            ),
        )
        .start()
        .await;

    let provider = EchoExtensionProvider::new(daemon.base_url());
    let track = Track {
        id: "upstream_failing_track".into(),
        title: "Test Track".into(),
        artist: "Test Artist".into(),
        album: None,
        duration_seconds: 200,
        thumbnail_url: None,
        isrc: None,
    };

    let err = provider
        .resolve_stream(&track)
        .await
        .expect_err("Must return Err on 502");

    assert!(err.should_fallback(), "502 upstream failure must trigger tier fallback");
    match err {
        ProviderError::UpstreamFailed(msg) => {
            assert!(msg.contains("Extension upstream CDN unreachable"));
        }
        other => panic!("Expected ProviderError::UpstreamFailed, got: {:?}", other),
    }
}

/// Verifies that an offline daemon (connection refused) returns `ProviderError::ConnectionRefused` without panic.
#[tokio::test]
async fn test_extension_connection_refused_offline() {
    let offline_base_url = MockExtensionDaemon::unused_base_url().await;
    let provider = EchoExtensionProvider::new(offline_base_url);

    let err = provider
        .check_health()
        .await
        .expect_err("Offline daemon must fail health check gracefully");

    assert!(err.is_connection_failure(), "Refused port must be connection failure");
    assert!(err.should_fallback(), "Offline daemon must trigger tier fallback");
    match err {
        ProviderError::ConnectionRefused(_) | ProviderError::Network(_) => {}
        other => panic!("Expected ConnectionRefused or Network error, got: {:?}", other),
    }
}

/// Verifies that client connection timeouts are gracefully mapped to `ProviderError::Timeout`.
#[tokio::test]
async fn test_extension_connection_timeout_handling() {
    // Mock daemon delays response by 500ms
    let daemon = MockExtensionDaemon::builder()
        .with_route(
            "/v1/health",
            MockResponse::ok_json(MockExtensionDaemon::fixture_health_healthy())
                .with_delay(Duration::from_millis(500)),
        )
        .start()
        .await;

    // Client configured with strict 100ms timeout
    let fast_client = reqwest::Client::builder()
        .timeout(Duration::from_millis(100))
        .build()
        .unwrap();

    let provider = EchoExtensionProvider::with_client(fast_client, daemon.base_url());
    let err = provider
        .check_health()
        .await
        .expect_err("Hanging daemon must trigger timeout error");

    assert!(
        err.is_connection_failure(),
        "Timeout must be classified as connection failure"
    );
    assert!(err.should_fallback(), "Timeout must trigger tier fallback");
    match err {
        ProviderError::Timeout(_) | ProviderError::Network(_) => {}
        other => panic!("Expected Timeout or Network error, got: {:?}", other),
    }
}

/// Verifies that HTTP 400 responses map to `ProviderError::InvalidParameters`.
#[tokio::test]
async fn test_extension_error_400_invalid_parameters() {
    let daemon = MockExtensionDaemon::builder()
        .with_route(
            "/v1/search",
            MockResponse::error_json(
                400,
                "INVALID_PARAMETERS",
                "Missing required query parameter 'q'",
                Some(400),
                false,
            ),
        )
        .start()
        .await;

    let provider = EchoExtensionProvider::new(daemon.base_url());
    let err = provider
        .search("")
        .await
        .expect_err("Must return Err on 400");

    match err {
        ProviderError::InvalidParameters(msg) => {
            assert!(msg.contains("Missing required query parameter"));
        }
        other => panic!("Expected ProviderError::InvalidParameters, got: {:?}", other),
    }
}

/// Verifies that HTTP 429 rate limit responses map to `ProviderError::RateLimited`.
#[tokio::test]
async fn test_extension_error_429_rate_limited() {
    let daemon = MockExtensionDaemon::builder()
        .with_route(
            "/v1/stream",
            MockResponse::error_json(
                429,
                "RATE_LIMITED",
                "Rate limit exceeded",
                Some(429),
                true,
            ),
        )
        .start()
        .await;

    let provider = EchoExtensionProvider::new(daemon.base_url());
    let track = Track {
        id: "rate_limited_track".into(),
        title: "Test".into(),
        artist: "Test".into(),
        album: None,
        duration_seconds: 100,
        thumbnail_url: None,
        isrc: None,
    };

    let err = provider
        .resolve_stream(&track)
        .await
        .expect_err("Must return Err on 429");

    match err {
        ProviderError::RateLimited => {}
        other => panic!("Expected ProviderError::RateLimited, got: {:?}", other),
    }
}
