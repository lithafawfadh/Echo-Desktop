//! Milestone 2 Integration Test Suite: Multi-Tier Fallback Router
//!
//! Tests the complete dynamic fallback cascade across fidelity tiers:
//! - Tier 1: Lossless Extension (FLAC 24-bit/192kHz or 16-bit/44.1kHz) at ephemeral port A
//! - Tier 2: 320kbs-extension (MP3/AAC 320kbps) at ephemeral port B
//! - Tier 3/4: YouTube Music InnerTube (AAC 128kbps / Opus 160kbps) via in-process mock
//!
//! 100% offline, zero external daemon processes, zero race conditions.

mod common;
use common::mock_daemon::{MockExtensionDaemon, MockResponse};

use echo_core::{AudioCodec, AudioFidelity, AudioStream, Track};
use echo_provider::error::ProviderError;
use echo_provider::extension::EchoExtensionProvider;
use echo_provider::router::DynamicFallbackRouter;
use echo_provider::MusicProvider;

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// =========================================================================
// Canonical Test Fixtures
// =========================================================================

/// 24-bit / 192kHz Studio Master FLAC fixture (LosslessHiRes tier)
const FIXTURE_STREAM_24_192_FLAC: &str = r#"{
  "url": "https://cdn.lossless.echomusic.internal/streams/dp_getlucky_24_192.flac?token=exp17412000_sig99281a",
  "format": "flac",
  "bitDepth": 24,
  "sampleRate": 192000,
  "bitrate": 9216,
  "isLossless": true,
  "fileSizeBytes": 253800000,
  "durationSeconds": 369,
  "expiresAt": 1741200000,
  "headers": {
    "User-Agent": "EchoDesktop/1.0",
    "Accept-Ranges": "bytes"
  }
}"#;

/// 16-bit / 44.1kHz Standard CD FLAC fixture (LosslessCD tier)
const FIXTURE_STREAM_16_44_FLAC: &str = r#"{
  "url": "https://cdn.lossless.echomusic.internal/streams/dp_getlucky_16_44.flac",
  "format": "flac",
  "bitDepth": 16,
  "sampleRate": 44100,
  "bitrate": 1411,
  "isLossless": true,
  "fileSizeBytes": 38900000,
  "durationSeconds": 369,
  "expiresAt": 1741200000,
  "headers": {
    "User-Agent": "EchoDesktop/1.0",
    "Accept-Ranges": "bytes"
  }
}"#;

// =========================================================================
// In-Process Mock InnerTube Provider for Tier 3 / Tier 4
// =========================================================================

/// Thread-safe in-process mock of `MusicProvider` for Tier 3 / Tier 4.
///
/// Records search queries and stream resolution requests to assert that
/// the router queries Tier 3 only when higher tiers fail or for catalog searches.
#[derive(Clone)]
pub struct MockInnerTubeProvider {
    search_handler: Arc<dyn Fn(&str) -> Result<Vec<Track>, ProviderError> + Send + Sync>,
    stream_handler: Arc<dyn Fn(&Track) -> Result<AudioStream, ProviderError> + Send + Sync>,
    search_queries: Arc<Mutex<Vec<String>>>,
    resolve_requests: Arc<Mutex<Vec<Track>>>,
}

impl Default for MockInnerTubeProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MockInnerTubeProvider {
    pub fn new() -> Self {
        Self {
            search_handler: Arc::new(|query| {
                Ok(vec![Track::new(
                    format!("yt_{query}_01"),
                    format!("Title for {query}"),
                    "InnerTube Artist",
                    Some("InnerTube Album".into()),
                    210,
                    None,
                )])
            }),
            stream_handler: Arc::new(|track| {
                Ok(AudioStream {
                    url: format!("https://rr1---sn-xxx.googlevideo.com/videoplayback?itag=140&id={}", track.id),
                    codec: AudioCodec::Aac,
                    bitrate_kbps: 128,
                    sample_rate_hz: 44100,
                    is_lossless: false,
                    bit_depth: None,
                    fidelity: Some(AudioFidelity::LossyStandard),
                    content_length_bytes: Some(3_500_000),
                    http_headers: HashMap::new(),
                })
            }),
            search_queries: Arc::new(Mutex::new(Vec::new())),
            resolve_requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Configures the mock to return fixed search results.
    pub fn with_search_results(mut self, tracks: Vec<Track>) -> Self {
        self.search_handler = Arc::new(move |_| Ok(tracks.clone()));
        self
    }

    /// Configures the mock to return a specific AudioStream on stream resolution.
    pub fn with_stream(mut self, stream: AudioStream) -> Self {
        self.stream_handler = Arc::new(move |_| Ok(stream.clone()));
        self
    }

    /// Configures the mock to return `NoStreamsAvailable` on stream resolution.
    pub fn with_no_streams(mut self) -> Self {
        self.stream_handler = Arc::new(move |track| {
            Err(ProviderError::NoStreamsAvailable(track.id.clone()))
        });
        self
    }

    /// Configures custom stream resolution handler.
    pub fn with_stream_handler<F>(mut self, handler: F) -> Self
    where
        F: Fn(&Track) -> Result<AudioStream, ProviderError> + Send + Sync + 'static,
    {
        self.stream_handler = Arc::new(handler);
        self
    }

    /// Number of times `resolve_stream` was called.
    pub fn resolve_count(&self) -> usize {
        self.resolve_requests.lock().unwrap().len()
    }

    /// Number of times `search` was called.
    pub fn search_count(&self) -> usize {
        self.search_queries.lock().unwrap().len()
    }

    /// Snapshot of search query strings received.
    pub fn recorded_search_queries(&self) -> Vec<String> {
        self.search_queries.lock().unwrap().clone()
    }
}

#[async_trait]
impl MusicProvider for MockInnerTubeProvider {
    fn name(&self) -> &'static str {
        "MockInnerTubeProvider"
    }

    fn supported_fidelity(&self) -> AudioFidelity {
        AudioFidelity::LossyStandard
    }

    async fn search(&self, query: &str) -> Result<Vec<Track>, ProviderError> {
        self.search_queries.lock().unwrap().push(query.to_string());
        (self.search_handler)(query)
    }

    async fn resolve_stream(&self, track: &Track) -> Result<AudioStream, ProviderError> {
        self.resolve_requests.lock().unwrap().push(track.clone());
        (self.stream_handler)(track)
    }
}

// =========================================================================
// Integration Test Cases (Milestone 2 Specification)
// =========================================================================

/// 1. `test_router_tier1_lossless_success`
/// Verifies: Tier 1 is online and returns 24-bit/192kHz FLAC.
/// Router returns Tier 1 stream immediately without querying Tier 2 or Tier 3.
#[tokio::test]
async fn test_router_tier1_lossless_success() {
    let daemon1 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_stream_ok(FIXTURE_STREAM_24_192_FLAC)
        .start()
        .await;

    let daemon2 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_stream_ok(MockExtensionDaemon::fixture_stream_320k_mp3())
        .start()
        .await;

    let mock_tier3 = MockInnerTubeProvider::new();

    let router = DynamicFallbackRouter::new(
        Some(Arc::new(EchoExtensionProvider::new(daemon1.base_url()))),
        Some(Arc::new(EchoExtensionProvider::new(daemon2.base_url()))),
        Arc::new(mock_tier3.clone()),
    );

    let track = Track {
        id: "ext_dp_getlucky".into(),
        title: "Get Lucky".into(),
        artist: "Daft Punk".into(),
        album: Some("Random Access Memories".into()),
        duration_seconds: 369,
        thumbnail_url: None,
        isrc: None,
    };

    let stream = router
        .resolve_best_stream(&track)
        .await
        .expect("Tier 1 lossless stream should resolve successfully");

    assert_eq!(stream.codec, AudioCodec::Flac);
    assert_eq!(stream.sample_rate_hz, 192000);
    assert_eq!(stream.bit_depth, Some(24));
    assert_eq!(stream.bitrate_kbps, 9216);
    assert!(stream.is_lossless);
    assert_eq!(stream.fidelity, Some(AudioFidelity::LosslessHiRes));
    assert!(stream.url.contains("dp_getlucky_24_192.flac"));

    // Short-circuit assertions: Tier 2 and Tier 3 must NOT have been queried for stream resolution
    let daemon2_stream_requests: Vec<_> = daemon2
        .recorded_requests()
        .await
        .into_iter()
        .filter(|r| r.path.starts_with("/v1/stream") || r.path.starts_with("/v1/resolve"))
        .collect();
    assert_eq!(
        daemon2_stream_requests.len(),
        0,
        "Tier 2 should NOT be queried when Tier 1 succeeds"
    );
    assert_eq!(
        mock_tier3.resolve_count(),
        0,
        "Tier 3 InnerTube should NOT be queried when Tier 1 succeeds"
    );
}

/// 2. `test_router_tier1_404_falls_back_to_tier2`
/// Verifies: Tier 1 returns 404 Not Found -> Router cascades to Tier 2 ->
/// Tier 2 returns 320kbps MP3 -> Router returns Tier 2 without querying Tier 3.
#[tokio::test]
async fn test_router_tier1_404_falls_back_to_tier2() {
    let daemon1 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_error("/v1/stream", 404, "TRACK_NOT_FOUND", "Track not in lossless catalog")
        .start()
        .await;

    let daemon2 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_stream_ok(MockExtensionDaemon::fixture_stream_320k_mp3())
        .start()
        .await;

    let mock_tier3 = MockInnerTubeProvider::new();

    let router = DynamicFallbackRouter::new(
        Some(Arc::new(EchoExtensionProvider::new(daemon1.base_url()))),
        Some(Arc::new(EchoExtensionProvider::new(daemon2.base_url()))),
        Arc::new(mock_tier3.clone()),
    );

    let track = Track::new("track_missing_in_tier1", "Get Lucky", "Daft Punk", None, 369, None);

    let stream = router
        .resolve_best_stream(&track)
        .await
        .expect("Router should fall back to Tier 2 when Tier 1 returns 404");

    assert_eq!(stream.codec, AudioCodec::Mp3);
    assert_eq!(stream.bitrate_kbps, 320);
    assert_eq!(stream.sample_rate_hz, 44100);
    assert!(!stream.is_lossless);
    assert_eq!(stream.fidelity, Some(AudioFidelity::Lossy320));
    assert!(stream.url.contains("dp_getlucky_320.mp3"));

    let daemon1_requests = daemon1.recorded_requests().await;
    assert!(
        daemon1_requests.iter().any(|r| r.path.starts_with("/v1/stream")),
        "Tier 1 stream endpoint must have been attempted before fallback"
    );
    assert_eq!(
        mock_tier3.resolve_count(),
        0,
        "Tier 3 InnerTube should NOT be queried when Tier 2 succeeds"
    );
}

/// 3. `test_router_tier1_unhealthy_skips_to_tier2`
/// Verifies: Tier 1 health check returns status: degraded.
/// Router proactively bypasses Tier 1 directly to Tier 2 without calling Tier 1 stream endpoint.
#[tokio::test]
async fn test_router_tier1_unhealthy_skips_to_tier2() {
    let daemon1 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_degraded())
        .with_error("/v1/stream", 500, "SHOULD_NOT_BE_CALLED", "Tier 1 was not bypassed")
        .start()
        .await;

    let daemon2 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_stream_ok(MockExtensionDaemon::fixture_stream_320k_mp3())
        .start()
        .await;

    let mock_tier3 = MockInnerTubeProvider::new();

    let router = DynamicFallbackRouter::new(
        Some(Arc::new(EchoExtensionProvider::new(daemon1.base_url()))),
        Some(Arc::new(EchoExtensionProvider::new(daemon2.base_url()))),
        Arc::new(mock_tier3.clone()),
    );

    let track = Track::new("trk_unhealthy_test", "Get Lucky", "Daft Punk", None, 369, None);

    let stream = router
        .resolve_best_stream(&track)
        .await
        .expect("Router should bypass unhealthy Tier 1 and resolve Tier 2");

    assert_eq!(stream.codec, AudioCodec::Mp3);
    assert_eq!(stream.fidelity, Some(AudioFidelity::Lossy320));

    // CRITICAL: Verify Tier 1 health check was queried, but stream endpoint was NEVER called
    let daemon1_requests = daemon1.recorded_requests().await;
    assert!(
        daemon1_requests.iter().any(|r| r.path == "/v1/health"),
        "Tier 1 health check must be queried"
    );
    let daemon1_stream_calls: Vec<_> = daemon1_requests
        .into_iter()
        .filter(|r| r.path.starts_with("/v1/stream") || r.path.starts_with("/v1/resolve"))
        .collect();
    assert_eq!(
        daemon1_stream_calls.len(),
        0,
        "Degraded Tier 1 must be bypassed without any stream resolution requests"
    );
}

/// 4. `test_router_both_extensions_offline_falls_back_to_innertube`
/// Verifies: Tier 1 and Tier 2 point to closed loopback ports.
/// Router cascades cleanly past both connection failures to Tier 3 InnerTube.
#[tokio::test]
async fn test_router_both_extensions_offline_falls_back_to_innertube() {
    let offline_tier1_url = MockExtensionDaemon::unused_base_url().await;
    let offline_tier2_url = MockExtensionDaemon::unused_base_url().await;

    let mock_tier3 = MockInnerTubeProvider::new().with_stream(AudioStream {
        url: "https://rr1---sn-xxx.googlevideo.com/videoplayback?itag=140&ratebypass=yes".into(),
        codec: AudioCodec::Aac,
        bitrate_kbps: 128,
        sample_rate_hz: 44100,
        is_lossless: false,
        bit_depth: None,
        fidelity: Some(AudioFidelity::LossyStandard),
        content_length_bytes: Some(3800000),
        http_headers: HashMap::new(),
    });

    let router = DynamicFallbackRouter::new(
        Some(Arc::new(EchoExtensionProvider::new(offline_tier1_url))),
        Some(Arc::new(EchoExtensionProvider::new(offline_tier2_url))),
        Arc::new(mock_tier3.clone()),
    );

    let track = Track::new("yt_vid_offline_test", "Get Lucky", "Daft Punk", None, 369, None);

    let stream = router
        .resolve_best_stream(&track)
        .await
        .expect("Router must fall back cleanly to Tier 3 when both extension daemons are offline");

    assert_eq!(stream.codec, AudioCodec::Aac);
    assert_eq!(stream.bitrate_kbps, 128);
    assert_eq!(stream.sample_rate_hz, 44100);
    assert!(!stream.is_lossless);
    assert_eq!(stream.fidelity, Some(AudioFidelity::LossyStandard));
    assert!(stream.url.contains("googlevideo.com"));
    assert_eq!(mock_tier3.resolve_count(), 1);
}

/// 5. `test_router_timeout_resilience`
/// Verifies: Tier 1 mock daemon delays stream response beyond client timeout.
/// Router catches timeout error and cascades to Tier 2 without hanging.
#[tokio::test]
async fn test_router_timeout_resilience() {
    let daemon1 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_route(
            "/v1/stream",
            MockResponse::ok_json(FIXTURE_STREAM_24_192_FLAC)
                .with_delay(Duration::from_millis(800)),
        )
        .start()
        .await;

    let fast_timeout_client = reqwest::Client::builder()
        .timeout(Duration::from_millis(200))
        .build()
        .unwrap();
    let tier1_provider = EchoExtensionProvider::with_client(fast_timeout_client, daemon1.base_url());

    let daemon2 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_stream_ok(MockExtensionDaemon::fixture_stream_320k_mp3())
        .start()
        .await;

    let mock_tier3 = MockInnerTubeProvider::new();

    let router = DynamicFallbackRouter::new(
        Some(Arc::new(tier1_provider)),
        Some(Arc::new(EchoExtensionProvider::new(daemon2.base_url()))),
        Arc::new(mock_tier3.clone()),
    );

    let track = Track::new("trk_timeout_test", "Get Lucky", "Daft Punk", None, 369, None);

    let start = Instant::now();
    let stream = router
        .resolve_best_stream(&track)
        .await
        .expect("Router should cascade to Tier 2 on Tier 1 timeout");
    let elapsed = start.elapsed();

    assert_eq!(stream.codec, AudioCodec::Mp3);
    assert_eq!(stream.fidelity, Some(AudioFidelity::Lossy320));

    assert!(
        elapsed >= Duration::from_millis(150),
        "Expected elapsed time >= 150ms due to timeout, got {:?}",
        elapsed
    );
    assert!(
        elapsed < Duration::from_millis(700),
        "Router took too long to cascade on timeout: {:?}",
        elapsed
    );
}

/// 6. `test_router_isrc_exact_match`
/// Verifies: Track with ISRC queries Tier 1 `/v1/resolve/isrc/{isrc}` directly.
#[tokio::test]
async fn test_router_isrc_exact_match() {
    let isrc = "USQX91300105";

    let daemon1 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_isrc_ok(isrc, FIXTURE_STREAM_24_192_FLAC)
        .with_error("/v1/stream", 400, "SHOULD_USE_ISRC", "Expected ISRC lookup")
        .start()
        .await;

    let daemon2 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_stream_ok(MockExtensionDaemon::fixture_stream_320k_mp3())
        .start()
        .await;

    let mock_tier3 = MockInnerTubeProvider::new();

    let router = DynamicFallbackRouter::new(
        Some(Arc::new(EchoExtensionProvider::new(daemon1.base_url()))),
        Some(Arc::new(EchoExtensionProvider::new(daemon2.base_url()))),
        Arc::new(mock_tier3.clone()),
    );

    let track = Track {
        id: "any_arbitrary_id".into(),
        title: "Get Lucky".into(),
        artist: "Daft Punk".into(),
        album: Some("Random Access Memories".into()),
        duration_seconds: 369,
        thumbnail_url: None,
        isrc: Some(isrc.to_string()),
    };

    let stream = router
        .resolve_best_stream(&track)
        .await
        .expect("Direct ISRC resolution should succeed");

    assert_eq!(stream.codec, AudioCodec::Flac);
    assert_eq!(stream.sample_rate_hz, 192000);
    assert_eq!(stream.bit_depth, Some(24));
    assert_eq!(stream.fidelity, Some(AudioFidelity::LosslessHiRes));

    let daemon1_requests = daemon1.recorded_requests().await;
    let expected_isrc_path = format!("/v1/resolve/isrc/{isrc}");
    assert!(
        daemon1_requests.iter().any(|r| r.path == expected_isrc_path),
        "Expected request to '{}', got requests: {:?}",
        expected_isrc_path,
        daemon1_requests
    );
}

/// 7. `test_router_all_tiers_exhausted`
/// Verifies: When Tier 1, Tier 2, and Tier 3 all return not found / errors,
/// the router returns `ProviderError::NoStreamsAvailable`.
#[tokio::test]
async fn test_router_all_tiers_exhausted() {
    let daemon1 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_error("/v1/stream", 404, "TRACK_NOT_FOUND", "Not in Tier 1")
        .start()
        .await;

    let daemon2 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_error("/v1/stream", 404, "TRACK_NOT_FOUND", "Not in Tier 2")
        .start()
        .await;

    let mock_tier3 = MockInnerTubeProvider::new().with_no_streams();

    let router = DynamicFallbackRouter::new(
        Some(Arc::new(EchoExtensionProvider::new(daemon1.base_url()))),
        Some(Arc::new(EchoExtensionProvider::new(daemon2.base_url()))),
        Arc::new(mock_tier3.clone()),
    );

    let track = Track::new("trk_completely_missing", "Ghost Track", "Nobody", None, 180, None);

    let err = router
        .resolve_best_stream(&track)
        .await
        .expect_err("Router must return an error when all tiers are exhausted");

    match &err {
        ProviderError::NoStreamsAvailable(msg) => {
            assert!(
                msg.contains("trk_completely_missing") || msg.contains("Ghost Track") || msg.contains("exhausted"),
                "Error message should mention track or exhaustion: {msg}"
            );
        }
        ProviderError::TrackNotFound(msg) => {
            assert!(msg.contains("trk_completely_missing"));
        }
        other => panic!("Expected NoStreamsAvailable or TrackNotFound, got: {:?}", other),
    }

    assert_eq!(mock_tier3.resolve_count(), 1);
}

/// 8. `test_router_search_delegates_to_innertube`
/// Verifies: `router.search()` queries Tier 3 InnerTube catalog for universal completeness,
/// without querying extension daemons.
#[tokio::test]
async fn test_router_search_delegates_to_innertube() {
    let daemon1 = MockExtensionDaemon::start().await;
    let daemon2 = MockExtensionDaemon::start().await;

    let search_tracks = vec![
        Track::new("yt_01", "Instant Crush", "Daft Punk", Some("RAM".into()), 337, None),
        Track::new("yt_02", "Lose Yourself to Dance", "Daft Punk", Some("RAM".into()), 353, None),
    ];

    let mock_tier3 = MockInnerTubeProvider::new().with_search_results(search_tracks.clone());

    let router = DynamicFallbackRouter::new(
        Some(Arc::new(EchoExtensionProvider::new(daemon1.base_url()))),
        Some(Arc::new(EchoExtensionProvider::new(daemon2.base_url()))),
        Arc::new(mock_tier3.clone()),
    );

    let results = router
        .search("Daft Punk")
        .await
        .expect("Search should delegate to Tier 3 InnerTube successfully");

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].id, "yt_01");
    assert_eq!(results[0].title, "Instant Crush");
    assert_eq!(results[1].id, "yt_02");
    assert_eq!(results[1].title, "Lose Yourself to Dance");

    assert_eq!(mock_tier3.search_count(), 1);
    assert_eq!(mock_tier3.recorded_search_queries(), vec!["Daft Punk".to_string()]);

    let daemon1_search = daemon1
        .recorded_requests()
        .await
        .into_iter()
        .filter(|r| r.path.contains("/search"))
        .count();
    let daemon2_search = daemon2
        .recorded_requests()
        .await
        .into_iter()
        .filter(|r| r.path.contains("/search"))
        .count();
    assert_eq!(daemon1_search, 0, "Tier 1 should not receive catalog search");
    assert_eq!(daemon2_search, 0, "Tier 2 should not receive catalog search");
}

/// 9. `test_router_tier1_502_upstream_failure_falls_back_to_tier2`
/// Verifies: Tier 1 returns HTTP 502 UPSTREAM_FAILED -> router cascades cleanly to Tier 2.
#[tokio::test]
async fn test_router_tier1_502_upstream_failure_falls_back_to_tier2() {
    let daemon1 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_error("/v1/stream", 502, "UPSTREAM_FAILED", "Lossless upstream CDN unreachable")
        .start()
        .await;

    let daemon2 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_stream_ok(MockExtensionDaemon::fixture_stream_320k_mp3())
        .start()
        .await;

    let mock_tier3 = MockInnerTubeProvider::new();

    let router = DynamicFallbackRouter::new(
        Some(Arc::new(EchoExtensionProvider::new(daemon1.base_url()))),
        Some(Arc::new(EchoExtensionProvider::new(daemon2.base_url()))),
        Arc::new(mock_tier3.clone()),
    );

    let track = Track::new("trk_502_test", "Get Lucky", "Daft Punk", None, 369, None);

    let stream = router
        .resolve_best_stream(&track)
        .await
        .expect("Router should cascade past 502 to Tier 2");

    assert_eq!(stream.codec, AudioCodec::Mp3);
    assert_eq!(stream.fidelity, Some(AudioFidelity::Lossy320));
    assert_eq!(mock_tier3.resolve_count(), 0);
}

/// 10. `test_router_tier1_416_quality_unavailable_falls_back_to_tier2`
/// Verifies: Tier 1 returns HTTP 416 QUALITY_UNAVAILABLE -> router cascades to Tier 2.
#[tokio::test]
async fn test_router_tier1_416_quality_unavailable_falls_back_to_tier2() {
    let daemon1 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_error("/v1/stream", 416, "QUALITY_UNAVAILABLE", "Lossless master not available")
        .start()
        .await;

    let daemon2 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_stream_ok(MockExtensionDaemon::fixture_stream_320k_mp3())
        .start()
        .await;

    let mock_tier3 = MockInnerTubeProvider::new();

    let router = DynamicFallbackRouter::new(
        Some(Arc::new(EchoExtensionProvider::new(daemon1.base_url()))),
        Some(Arc::new(EchoExtensionProvider::new(daemon2.base_url()))),
        Arc::new(mock_tier3.clone()),
    );

    let track = Track::new("trk_416_test", "Get Lucky", "Daft Punk", None, 369, None);

    let stream = router
        .resolve_best_stream(&track)
        .await
        .expect("Router should cascade past 416 to Tier 2");

    assert_eq!(stream.codec, AudioCodec::Mp3);
    assert_eq!(stream.fidelity, Some(AudioFidelity::Lossy320));
}

/// 11. `test_router_tier1_isrc_404_fallback_to_stream`
/// Verifies: Track has ISRC, but `/v1/resolve/isrc/{isrc}` returns 404.
/// Router falls back to Tier 1 `/v1/stream?quality=lossless_max` and succeeds without cascading to Tier 2.
#[tokio::test]
async fn test_router_tier1_isrc_404_fallback_to_stream() {
    let isrc = "USQX91300105";

    let daemon1 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_error(format!("/v1/resolve/isrc/{}", isrc), 404, "TRACK_NOT_FOUND", "ISRC not matched")
        .with_stream_ok(FIXTURE_STREAM_24_192_FLAC)
        .start()
        .await;

    let daemon2 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_stream_ok(MockExtensionDaemon::fixture_stream_320k_mp3())
        .start()
        .await;

    let mock_tier3 = MockInnerTubeProvider::new();

    let router = DynamicFallbackRouter::new(
        Some(Arc::new(EchoExtensionProvider::new(daemon1.base_url()))),
        Some(Arc::new(EchoExtensionProvider::new(daemon2.base_url()))),
        Arc::new(mock_tier3.clone()),
    );

    let track = Track {
        id: "trk_fallback_stream".into(),
        title: "Get Lucky".into(),
        artist: "Daft Punk".into(),
        album: Some("Random Access Memories".into()),
        duration_seconds: 369,
        thumbnail_url: None,
        isrc: Some(isrc.to_string()),
    };

    let stream = router
        .resolve_best_stream(&track)
        .await
        .expect("Router should fall back to /v1/stream on Tier 1 when ISRC returns 404");

    assert_eq!(stream.codec, AudioCodec::Flac);
    assert_eq!(stream.fidelity, Some(AudioFidelity::LosslessHiRes));

    // Verify Tier 2 was NOT queried
    let daemon2_stream_requests: Vec<_> = daemon2
        .recorded_requests()
        .await
        .into_iter()
        .filter(|r| r.path.starts_with("/v1/stream"))
        .collect();
    assert_eq!(daemon2_stream_requests.len(), 0, "Tier 2 should NOT be called");
}

/// 12. `test_cascade_tier2_verifies_quality_high_320`
/// Verifies: When Tier 1 fails and router cascades to Tier 2, the HTTP request
/// explicitly passes `quality=high_320` in query parameters.
#[tokio::test]
async fn test_cascade_tier2_verifies_quality_high_320() {
    let daemon1 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_error("/v1/stream", 404, "TRACK_NOT_FOUND", "Missing in Tier 1")
        .start()
        .await;

    let daemon2 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_stream_ok(MockExtensionDaemon::fixture_stream_320k_mp3())
        .start()
        .await;

    let mock_tier3 = MockInnerTubeProvider::new();

    let router = DynamicFallbackRouter::new(
        Some(Arc::new(EchoExtensionProvider::new(daemon1.base_url()))),
        Some(Arc::new(EchoExtensionProvider::new(daemon2.base_url()))),
        Arc::new(mock_tier3.clone()),
    );

    let track = Track::new("trk_tier2_quality_test", "Get Lucky", "Daft Punk", None, 369, None);

    let stream = router
        .resolve_best_stream(&track)
        .await
        .expect("Tier 2 resolution should succeed");

    assert_eq!(stream.codec, AudioCodec::Mp3);
    assert_eq!(stream.fidelity, Some(AudioFidelity::Lossy320));

    // Inspect recorded requests on daemon 2
    let daemon2_requests = daemon2.recorded_requests().await;
    let stream_req = daemon2_requests
        .iter()
        .find(|r| r.path.starts_with("/v1/stream"))
        .expect("Daemon 2 must have received a /v1/stream request");

    assert_eq!(
        stream_req.query_params.get("quality").map(|s| s.as_str()),
        Some("high_320"),
        "Tier 2 request must explicitly carry quality=high_320"
    );
}

/// 13. `test_router_none_extensions_direct_to_tier3`
/// Verifies: When router is initialized with `tier1: None` and `tier2: None`,
/// stream resolution immediately routes to Tier 3 without attempting any HTTP calls.
#[tokio::test]
async fn test_router_none_extensions_direct_to_tier3() {
    let mock_tier3 = MockInnerTubeProvider::new();

    let router = DynamicFallbackRouter::new(
        None,
        None,
        Arc::new(mock_tier3.clone()),
    );

    let track = Track::new("trk_none_test", "Get Lucky", "Daft Punk", None, 369, None);

    let stream = router
        .resolve_best_stream(&track)
        .await
        .expect("Router should resolve via Tier 3 when extensions are None");

    assert_eq!(stream.codec, AudioCodec::Aac);
    assert_eq!(stream.fidelity, Some(AudioFidelity::LossyStandard));
    assert_eq!(mock_tier3.resolve_count(), 1);
}

/// 14. `test_router_music_provider_trait_impl`
/// Verifies: `DynamicFallbackRouter` correctly implements `MusicProvider` trait:
/// `resolve_stream` delegates to `resolve_best_stream`, and metadata methods reflect expected values.
#[tokio::test]
async fn test_router_music_provider_trait_impl() {
    let daemon1 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_stream_ok(FIXTURE_STREAM_16_44_FLAC)
        .start()
        .await;

    let mock_tier3 = MockInnerTubeProvider::new();

    let router = DynamicFallbackRouter::new(
        Some(Arc::new(EchoExtensionProvider::new(daemon1.base_url()))),
        None,
        Arc::new(mock_tier3),
    );

    // Verify trait dispatch via `&dyn MusicProvider`
    let provider: &dyn MusicProvider = &router;
    assert_eq!(provider.name(), "EchoDynamicFallbackRouter");
    assert_eq!(provider.supported_fidelity(), AudioFidelity::LosslessHiRes);

    let track = Track::new("trk_trait_test", "Get Lucky", "Daft Punk", None, 369, None);
    let stream = provider
        .resolve_stream(&track)
        .await
        .expect("MusicProvider::resolve_stream should succeed");

    assert_eq!(stream.codec, AudioCodec::Flac);
    assert_eq!(stream.sample_rate_hz, 44100);
    assert_eq!(stream.bit_depth, Some(16));
    assert_eq!(stream.fidelity, Some(AudioFidelity::LosslessCD));
}

/// 15. `test_router_tier1_429_rate_limited_falls_back_to_tier2`
/// Verifies: Tier 1 returns HTTP 429 RATE_LIMITED -> router cascades cleanly to Tier 2.
#[tokio::test]
async fn test_router_tier1_429_rate_limited_falls_back_to_tier2() {
    let daemon1 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_error("/v1/stream", 429, "RATE_LIMITED", "Too many requests to lossless daemon")
        .start()
        .await;

    let daemon2 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_stream_ok(MockExtensionDaemon::fixture_stream_320k_mp3())
        .start()
        .await;

    let mock_tier3 = MockInnerTubeProvider::new();

    let router = DynamicFallbackRouter::new(
        Some(Arc::new(EchoExtensionProvider::new(daemon1.base_url()))),
        Some(Arc::new(EchoExtensionProvider::new(daemon2.base_url()))),
        Arc::new(mock_tier3.clone()),
    );

    let track = Track::new("trk_429_test", "Get Lucky", "Daft Punk", None, 369, None);

    let stream = router
        .resolve_best_stream(&track)
        .await
        .expect("Router should cascade past 429 to Tier 2");

    assert_eq!(stream.codec, AudioCodec::Mp3);
    assert_eq!(stream.fidelity, Some(AudioFidelity::Lossy320));
    assert_eq!(mock_tier3.resolve_count(), 0);
}

/// 16. `test_router_health_timeout_bypass`
/// Verifies: Tier 1 health check probe delays beyond configured `health_timeout`.
/// Router proactively bypasses Tier 1 directly to Tier 2 without waiting indefinitely.
#[tokio::test]
async fn test_router_health_timeout_bypass() {
    let daemon1 = MockExtensionDaemon::builder()
        .with_route(
            "/v1/health",
            MockResponse::ok_json(MockExtensionDaemon::fixture_health_healthy())
                .with_delay(Duration::from_millis(800)),
        )
        .with_error("/v1/stream", 500, "SHOULD_NOT_BE_CALLED", "Tier 1 was not bypassed")
        .start()
        .await;

    let daemon2 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_stream_ok(MockExtensionDaemon::fixture_stream_320k_mp3())
        .start()
        .await;

    let mock_tier3 = MockInnerTubeProvider::new();

    let router = DynamicFallbackRouter::new(
        Some(Arc::new(EchoExtensionProvider::new(daemon1.base_url()))),
        Some(Arc::new(EchoExtensionProvider::new(daemon2.base_url()))),
        Arc::new(mock_tier3.clone()),
    )
    .with_health_timeout(Duration::from_millis(150));

    let track = Track::new("trk_health_timeout_test", "Get Lucky", "Daft Punk", None, 369, None);

    let start = Instant::now();
    let stream = router
        .resolve_best_stream(&track)
        .await
        .expect("Router should bypass Tier 1 on health check timeout and resolve Tier 2");
    let elapsed = start.elapsed();

    assert_eq!(stream.codec, AudioCodec::Mp3);
    assert_eq!(stream.fidelity, Some(AudioFidelity::Lossy320));

    assert!(
        elapsed >= Duration::from_millis(100),
        "Expected elapsed >= 100ms, got {:?}",
        elapsed
    );
    assert!(
        elapsed < Duration::from_millis(700),
        "Router took too long to bypass on health timeout: {:?}",
        elapsed
    );

    // Verify stream on Tier 1 was NEVER requested
    let daemon1_stream_calls: Vec<_> = daemon1
        .recorded_requests()
        .await
        .into_iter()
        .filter(|r| r.path.starts_with("/v1/stream"))
        .collect();
    assert_eq!(daemon1_stream_calls.len(), 0);
}

/// 17. `test_router_check_health_report`
/// Verifies: `check_health()` returns comprehensive report across all configured tiers.
#[tokio::test]
async fn test_router_check_health_report() {
    let daemon1 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .start()
        .await;

    let offline_tier2_url = MockExtensionDaemon::unused_base_url().await;

    let mock_tier3 = MockInnerTubeProvider::new();

    let router = DynamicFallbackRouter::new(
        Some(Arc::new(EchoExtensionProvider::new(daemon1.base_url()))),
        Some(Arc::new(EchoExtensionProvider::new(offline_tier2_url))),
        Arc::new(mock_tier3),
    )
    .with_health_timeout(Duration::from_millis(200));

    let report = router.check_health().await;

    assert!(report.tier1_lossless_configured);
    assert!(report.tier1_lossless_healthy);
    assert!(report.tier2_highbitrate_configured);
    assert!(!report.tier2_highbitrate_healthy);
    assert!(report.tier3_innertube_available);
}


// =========================================================================
// ADVERSARIAL CHALLENGER VERIFICATION SUITE (challenger_m2_1)
// =========================================================================

/// Adv 1: Short-Circuit Proof — Tier 1 ISRC Resolution Success
/// Verifies: When Tier 1 resolves via ISRC direct lookup:
/// - Tier 1 /v1/stream receives 0 calls
/// - Tier 2 receives EXACTLY 0 calls (/v1/health, /v1/stream, /v1/resolve)
/// - Tier 3 InnerTube resolve_count is EXACTLY 0 and search_count is 0
#[tokio::test]
async fn test_adversarial_short_circuit_tier1_isrc_success() {
    let isrc = "USQX91300105";

    let daemon1 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_isrc_ok(isrc, FIXTURE_STREAM_24_192_FLAC)
        .with_error("/v1/stream", 500, "FATAL_UNEXPECTED", "Tier 1 /v1/stream should not be called")
        .start()
        .await;

    let daemon2 = MockExtensionDaemon::builder()
        .with_error("/v1/health", 500, "FATAL_UNEXPECTED", "Tier 2 should not be touched")
        .with_error("/v1/stream", 500, "FATAL_UNEXPECTED", "Tier 2 should not be touched")
        .start()
        .await;

    let mock_tier3 = MockInnerTubeProvider::new();

    let router = DynamicFallbackRouter::new(
        Some(Arc::new(EchoExtensionProvider::new(daemon1.base_url()))),
        Some(Arc::new(EchoExtensionProvider::new(daemon2.base_url()))),
        Arc::new(mock_tier3.clone()),
    );

    let track = Track {
        id: "adv_trk_isrc".into(),
        title: "Get Lucky".into(),
        artist: "Daft Punk".into(),
        album: Some("Random Access Memories".into()),
        duration_seconds: 369,
        thumbnail_url: None,
        isrc: Some(isrc.to_string()),
    };

    let stream = router
        .resolve_best_stream(&track)
        .await
        .expect("Tier 1 ISRC resolution must succeed");

    assert_eq!(stream.codec, AudioCodec::Flac);
    assert_eq!(stream.fidelity, Some(AudioFidelity::LosslessHiRes));

    // Assert Tier 1 calls: exactly 1 health, 1 isrc, ZERO /v1/stream
    let d1_reqs = daemon1.recorded_requests().await;
    assert_eq!(
        d1_reqs.iter().filter(|r| r.path.starts_with("/v1/stream")).count(),
        0,
        "Tier 1 /v1/stream MUST NOT be called when ISRC resolves"
    );

    // Assert Tier 2 ZERO calls of any kind
    let d2_reqs = daemon2.recorded_requests().await;
    assert_eq!(
        d2_reqs.len(),
        0,
        "Tier 2 MUST receive exactly 0 requests when Tier 1 succeeds via ISRC"
    );

    // Assert Tier 3 ZERO calls
    assert_eq!(
        mock_tier3.resolve_count(),
        0,
        "Tier 3 InnerTube MUST receive exactly 0 stream calls when Tier 1 succeeds"
    );
    assert_eq!(
        mock_tier3.search_count(),
        0,
        "Tier 3 InnerTube MUST receive exactly 0 search calls during stream resolution"
    );
}

/// Adv 2: Short-Circuit Proof — Tier 1 Stream Success (No ISRC)
/// Verifies: When Tier 1 resolves via /v1/stream?quality=lossless_max:
/// - Tier 2 receives EXACTLY 0 requests
/// - Tier 3 receives EXACTLY 0 requests
#[tokio::test]
async fn test_adversarial_short_circuit_tier1_stream_success() {
    let daemon1 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_stream_ok(FIXTURE_STREAM_24_192_FLAC)
        .start()
        .await;

    let daemon2 = MockExtensionDaemon::builder()
        .with_error("/v1/health", 500, "FATAL_UNEXPECTED", "Tier 2 should not be touched")
        .with_error("/v1/stream", 500, "FATAL_UNEXPECTED", "Tier 2 should not be touched")
        .start()
        .await;

    let mock_tier3 = MockInnerTubeProvider::new();

    let router = DynamicFallbackRouter::new(
        Some(Arc::new(EchoExtensionProvider::new(daemon1.base_url()))),
        Some(Arc::new(EchoExtensionProvider::new(daemon2.base_url()))),
        Arc::new(mock_tier3.clone()),
    );

    let track = Track::new("adv_trk_no_isrc", "Get Lucky", "Daft Punk", None, 369, None);

    let stream = router
        .resolve_best_stream(&track)
        .await
        .expect("Tier 1 stream resolution must succeed");

    assert_eq!(stream.codec, AudioCodec::Flac);
    assert_eq!(stream.fidelity, Some(AudioFidelity::LosslessHiRes));

    // Verify Tier 1 sent quality=lossless_max
    let d1_reqs = daemon1.recorded_requests().await;
    let stream_req = d1_reqs
        .iter()
        .find(|r| r.path.starts_with("/v1/stream"))
        .expect("Tier 1 /v1/stream was not requested");
    assert_eq!(
        stream_req.query_params.get("quality").map(|s| s.as_str()),
        Some("lossless_max"),
        "Tier 1 MUST request quality=lossless_max"
    );

    // Verify Tier 2 received 0 requests
    let d2_reqs = daemon2.recorded_requests().await;
    assert_eq!(
        d2_reqs.len(),
        0,
        "Tier 2 MUST receive 0 requests when Tier 1 succeeds"
    );

    // Verify Tier 3 received 0 requests
    assert_eq!(
        mock_tier3.resolve_count(),
        0,
        "Tier 3 InnerTube MUST receive 0 requests when Tier 1 succeeds"
    );
}

/// Adv 3: Degraded Health Bypass Proof — Track WITH ISRC
/// Verifies: When Tier 1 is degraded, the router queries /v1/health once and:
/// - NEVER queries /v1/resolve/isrc/{isrc}
/// - NEVER queries /v1/stream
/// - Seamlessly cascades to Tier 2
#[tokio::test]
async fn test_adversarial_degraded_health_bypass_with_isrc() {
    let isrc = "USQX91300105";

    let daemon1 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_degraded())
        .with_error(format!("/v1/resolve/isrc/{isrc}"), 500, "FATAL_BYPASS_VIOLATION", "Should not call ISRC on degraded daemon")
        .with_error("/v1/stream", 500, "FATAL_BYPASS_VIOLATION", "Should not call stream on degraded daemon")
        .start()
        .await;

    let daemon2 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_stream_ok(MockExtensionDaemon::fixture_stream_320k_mp3())
        .start()
        .await;

    let mock_tier3 = MockInnerTubeProvider::new();

    let router = DynamicFallbackRouter::new(
        Some(Arc::new(EchoExtensionProvider::new(daemon1.base_url()))),
        Some(Arc::new(EchoExtensionProvider::new(daemon2.base_url()))),
        Arc::new(mock_tier3.clone()),
    );

    let track = Track {
        id: "adv_trk_degraded_isrc".into(),
        title: "Get Lucky".into(),
        artist: "Daft Punk".into(),
        album: Some("Random Access Memories".into()),
        duration_seconds: 369,
        thumbnail_url: None,
        isrc: Some(isrc.to_string()),
    };

    let stream = router
        .resolve_best_stream(&track)
        .await
        .expect("Should bypass degraded Tier 1 and resolve via Tier 2");

    assert_eq!(stream.codec, AudioCodec::Mp3);
    assert_eq!(stream.fidelity, Some(AudioFidelity::Lossy320));

    // Verify Tier 1 calls
    let d1_reqs = daemon1.recorded_requests().await;
    assert_eq!(
        d1_reqs.len(),
        1,
        "Tier 1 must receive EXACTLY 1 request (/v1/health)"
    );
    assert_eq!(d1_reqs[0].path, "/v1/health");

    // Tier 3 was not needed
    assert_eq!(mock_tier3.resolve_count(), 0);
}

/// Adv 4: Degraded Health Bypass on BOTH Tier 1 and Tier 2 -> Direct to Tier 3
/// Verifies: When BOTH Tier 1 and Tier 2 report degraded health:
/// - Both are probed for /v1/health exactly once
/// - Neither receives any stream or resolution calls
/// - Resolution cascades cleanly to Tier 3 InnerTube
#[tokio::test]
async fn test_adversarial_degraded_health_bypass_both_tiers() {
    let daemon1 = MockExtensionDaemon::builder()
        .with_health_ok(r#"{"status": "maintenance", "extension": {"name": "lossless", "version": "1.0", "tier": "lossless", "capabilities": {"formats": ["flac"], "maxBitDepth": 16, "maxSampleRate": 44100, "supportsIsrcLookup": false, "supportsDirectStreaming": false}}}"#)
        .with_error("/v1/stream", 500, "FATAL_BYPASS_VIOLATION", "Should not call Tier 1 stream")
        .start()
        .await;

    let daemon2 = MockExtensionDaemon::builder()
        .with_health_ok(r#"{"status": "unhealthy", "extension": {"name": "320k", "version": "1.0", "tier": "high_bitrate", "capabilities": {"formats": ["mp3"], "maxBitDepth": 16, "maxSampleRate": 44100, "supportsIsrcLookup": false, "supportsDirectStreaming": false}}}"#)
        .with_error("/v1/stream", 500, "FATAL_BYPASS_VIOLATION", "Should not call Tier 2 stream")
        .start()
        .await;

    let mock_tier3 = MockInnerTubeProvider::new();

    let router = DynamicFallbackRouter::new(
        Some(Arc::new(EchoExtensionProvider::new(daemon1.base_url()))),
        Some(Arc::new(EchoExtensionProvider::new(daemon2.base_url()))),
        Arc::new(mock_tier3.clone()),
    );

    let track = Track {
        id: "adv_trk_both_degraded".into(),
        title: "Get Lucky".into(),
        artist: "Daft Punk".into(),
        album: None,
        duration_seconds: 369,
        thumbnail_url: None,
        isrc: Some("USQX91300105".into()),
    };

    let stream = router
        .resolve_best_stream(&track)
        .await
        .expect("Should bypass both degraded tiers and resolve via Tier 3");

    assert_eq!(stream.codec, AudioCodec::Aac);
    assert_eq!(stream.fidelity, Some(AudioFidelity::LossyStandard));
    assert_eq!(mock_tier3.resolve_count(), 1);

    // Assert Tier 1 and Tier 2 received only health checks
    let d1_reqs = daemon1.recorded_requests().await;
    assert_eq!(d1_reqs.len(), 1);
    assert_eq!(d1_reqs[0].path, "/v1/health");

    let d2_reqs = daemon2.recorded_requests().await;
    assert_eq!(d2_reqs.len(), 1);
    assert_eq!(d2_reqs[0].path, "/v1/health");
}

/// Adv 5: Quality Parameter Enforcement — Tier 2 Rejects lossless_max
/// Verifies:
/// - Tier 1 fails (404)
/// - Tier 2 STRICTLY queries quality=high_320 and NOT lossless_max
/// - Daemon asserts that receiving quality=lossless_max on Tier 2 causes 416
#[tokio::test]
async fn test_adversarial_quality_parameter_enforcement_high_320() {
    let daemon1 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_error("/v1/stream", 404, "TRACK_NOT_FOUND", "Missing")
        .start()
        .await;

    let daemon2 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_stream_ok(MockExtensionDaemon::fixture_stream_320k_mp3())
        .start()
        .await;

    let mock_tier3 = MockInnerTubeProvider::new();

    let router = DynamicFallbackRouter::new(
        Some(Arc::new(EchoExtensionProvider::new(daemon1.base_url()))),
        Some(Arc::new(EchoExtensionProvider::new(daemon2.base_url()))),
        Arc::new(mock_tier3.clone()),
    );

    let track = Track::new("adv_trk_quality_check", "Get Lucky", "Daft Punk", None, 369, None);

    let stream = router
        .resolve_best_stream(&track)
        .await
        .expect("Tier 2 resolution must succeed");

    assert_eq!(stream.fidelity, Some(AudioFidelity::Lossy320));

    // Inspect Tier 1 request query
    let d1_reqs = daemon1.recorded_requests().await;
    let d1_stream_req = d1_reqs.iter().find(|r| r.path.starts_with("/v1/stream")).unwrap();
    assert_eq!(
        d1_stream_req.query_params.get("quality").map(|s| s.as_str()),
        Some("lossless_max"),
        "Tier 1 must query quality=lossless_max"
    );

    // Inspect Tier 2 request query
    let d2_reqs = daemon2.recorded_requests().await;
    let d2_stream_req = d2_reqs.iter().find(|r| r.path.starts_with("/v1/stream")).unwrap();
    assert_eq!(
        d2_stream_req.query_params.get("quality").map(|s| s.as_str()),
        Some("high_320"),
        "Tier 2 must strictly query quality=high_320"
    );
    assert_ne!(
        d2_stream_req.query_params.get("quality").map(|s| s.as_str()),
        Some("lossless_max"),
        "Tier 2 MUST NEVER query quality=lossless_max"
    );
}

/// Adv 6: Complete Failure Mode Permutation Matrix
/// Proves that multi-tier cascading works under all individual and compound failure modes:
/// 6A: Tier 1 404 -> Tier 2 416 -> Tier 3 succeeds
/// 6B: Tier 1 416 -> Tier 2 429 -> Tier 3 succeeds
/// 6C: Tier 1 429 -> Tier 2 502 -> Tier 3 succeeds
/// 6D: Tier 1 502 -> Tier 2 404 -> Tier 3 succeeds
/// 6E: Tier 1 Refused -> Tier 2 Refused -> Tier 3 succeeds
/// 6F: Tier 1 Hanging Health -> Tier 2 Hanging Stream -> Tier 3 succeeds
#[tokio::test]
async fn test_adversarial_cascade_failure_mode_matrix() {
    // 6A: Tier 1 404 -> Tier 2 416 -> Tier 3 succeeds
    {
        let d1 = MockExtensionDaemon::builder()
            .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
            .with_error("/v1/stream", 404, "TRACK_NOT_FOUND", "404")
            .start().await;
        let d2 = MockExtensionDaemon::builder()
            .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
            .with_error("/v1/stream", 416, "QUALITY_UNAVAILABLE", "416")
            .start().await;
        let t3 = MockInnerTubeProvider::new();
        let router = DynamicFallbackRouter::new(
            Some(Arc::new(EchoExtensionProvider::new(d1.base_url()))),
            Some(Arc::new(EchoExtensionProvider::new(d2.base_url()))),
            Arc::new(t3.clone()),
        );
        let track = Track::new("trk_6a", "Title", "Artist", None, 180, None);
        let s = router.resolve_best_stream(&track).await.unwrap();
        assert_eq!(s.fidelity, Some(AudioFidelity::LossyStandard));
        assert_eq!(t3.resolve_count(), 1);
    }

    // 6B: Tier 1 416 -> Tier 2 429 -> Tier 3 succeeds
    {
        let d1 = MockExtensionDaemon::builder()
            .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
            .with_error("/v1/stream", 416, "QUALITY_UNAVAILABLE", "416")
            .start().await;
        let d2 = MockExtensionDaemon::builder()
            .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
            .with_error("/v1/stream", 429, "RATE_LIMITED", "429")
            .start().await;
        let t3 = MockInnerTubeProvider::new();
        let router = DynamicFallbackRouter::new(
            Some(Arc::new(EchoExtensionProvider::new(d1.base_url()))),
            Some(Arc::new(EchoExtensionProvider::new(d2.base_url()))),
            Arc::new(t3.clone()),
        );
        let track = Track::new("trk_6b", "Title", "Artist", None, 180, None);
        let s = router.resolve_best_stream(&track).await.unwrap();
        assert_eq!(s.fidelity, Some(AudioFidelity::LossyStandard));
        assert_eq!(t3.resolve_count(), 1);
    }

    // 6C: Tier 1 429 -> Tier 2 502 -> Tier 3 succeeds
    {
        let d1 = MockExtensionDaemon::builder()
            .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
            .with_error("/v1/stream", 429, "RATE_LIMITED", "429")
            .start().await;
        let d2 = MockExtensionDaemon::builder()
            .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
            .with_error("/v1/stream", 502, "UPSTREAM_FAILED", "502")
            .start().await;
        let t3 = MockInnerTubeProvider::new();
        let router = DynamicFallbackRouter::new(
            Some(Arc::new(EchoExtensionProvider::new(d1.base_url()))),
            Some(Arc::new(EchoExtensionProvider::new(d2.base_url()))),
            Arc::new(t3.clone()),
        );
        let track = Track::new("trk_6c", "Title", "Artist", None, 180, None);
        let s = router.resolve_best_stream(&track).await.unwrap();
        assert_eq!(s.fidelity, Some(AudioFidelity::LossyStandard));
        assert_eq!(t3.resolve_count(), 1);
    }

    // 6D: Tier 1 502 -> Tier 2 404 -> Tier 3 succeeds
    {
        let d1 = MockExtensionDaemon::builder()
            .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
            .with_error("/v1/stream", 502, "UPSTREAM_FAILED", "502")
            .start().await;
        let d2 = MockExtensionDaemon::builder()
            .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
            .with_error("/v1/stream", 404, "TRACK_NOT_FOUND", "404")
            .start().await;
        let t3 = MockInnerTubeProvider::new();
        let router = DynamicFallbackRouter::new(
            Some(Arc::new(EchoExtensionProvider::new(d1.base_url()))),
            Some(Arc::new(EchoExtensionProvider::new(d2.base_url()))),
            Arc::new(t3.clone()),
        );
        let track = Track::new("trk_6d", "Title", "Artist", None, 180, None);
        let s = router.resolve_best_stream(&track).await.unwrap();
        assert_eq!(s.fidelity, Some(AudioFidelity::LossyStandard));
        assert_eq!(t3.resolve_count(), 1);
    }

    // 6E: Tier 1 Refused (offline port) -> Tier 2 Refused -> Tier 3 succeeds
    {
        let offline_u1 = MockExtensionDaemon::unused_base_url().await;
        let offline_u2 = MockExtensionDaemon::unused_base_url().await;
        let t3 = MockInnerTubeProvider::new();
        let router = DynamicFallbackRouter::new(
            Some(Arc::new(EchoExtensionProvider::new(offline_u1))),
            Some(Arc::new(EchoExtensionProvider::new(offline_u2))),
            Arc::new(t3.clone()),
        );
        let track = Track::new("trk_6e", "Title", "Artist", None, 180, None);
        let s = router.resolve_best_stream(&track).await.unwrap();
        assert_eq!(s.fidelity, Some(AudioFidelity::LossyStandard));
        assert_eq!(t3.resolve_count(), 1);
    }

    // 6F: Tier 1 Hanging Health (>150ms probe timeout) -> Tier 2 Hanging Stream (>200ms client timeout) -> Tier 3 succeeds
    {
        let d1 = MockExtensionDaemon::builder()
            .with_route(
                "/v1/health",
                MockResponse::ok_json(MockExtensionDaemon::fixture_health_healthy())
                    .with_delay(Duration::from_millis(500)),
            )
            .start().await;

        let d2 = MockExtensionDaemon::builder()
            .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
            .with_route(
                "/v1/stream",
                MockResponse::ok_json(MockExtensionDaemon::fixture_stream_320k_mp3())
                    .with_delay(Duration::from_millis(600)),
            )
            .start().await;

        let fast_client = reqwest::Client::builder()
            .timeout(Duration::from_millis(150))
            .build().unwrap();
        let tier2_prov = EchoExtensionProvider::with_client(fast_client, d2.base_url());

        let t3 = MockInnerTubeProvider::new();
        let router = DynamicFallbackRouter::new(
            Some(Arc::new(EchoExtensionProvider::new(d1.base_url()))),
            Some(Arc::new(tier2_prov)),
            Arc::new(t3.clone()),
        ).with_health_timeout(Duration::from_millis(100));

        let track = Track::new("trk_6f", "Title", "Artist", None, 180, None);
        let s = router.resolve_best_stream(&track).await.unwrap();
        assert_eq!(s.fidelity, Some(AudioFidelity::LossyStandard));
        assert_eq!(t3.resolve_count(), 1);
    }
}

/// Adv 7: Corrupted Non-JSON Payload Resilience
/// Verifies: When Tier 1 returns HTTP 200 OK with unparseable corrupt payload,
/// the router does NOT panic, catches JsonParse error, and cascades cleanly to Tier 2.
#[tokio::test]
async fn test_adversarial_corrupted_json_payload_resilience() {
    let daemon1 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_route(
            "/v1/stream",
            MockResponse::new(200, "OK", "application/json", "<<<MALFORMED NOT JSON STRING>>>"),
        )
        .start()
        .await;

    let daemon2 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_stream_ok(MockExtensionDaemon::fixture_stream_320k_mp3())
        .start()
        .await;

    let mock_tier3 = MockInnerTubeProvider::new();

    let router = DynamicFallbackRouter::new(
        Some(Arc::new(EchoExtensionProvider::new(daemon1.base_url()))),
        Some(Arc::new(EchoExtensionProvider::new(daemon2.base_url()))),
        Arc::new(mock_tier3.clone()),
    );

    let track = Track::new("adv_trk_corrupt_json", "Get Lucky", "Daft Punk", None, 369, None);

    let stream = router
        .resolve_best_stream(&track)
        .await
        .expect("Router should survive malformed JSON from Tier 1 and cascade to Tier 2");

    assert_eq!(stream.codec, AudioCodec::Mp3);
    assert_eq!(stream.fidelity, Some(AudioFidelity::Lossy320));
    assert_eq!(mock_tier3.resolve_count(), 0);
}

/// Adv 8: Whitespace-only ISRC Bypasses ISRC Lookup
/// Verifies: Track with whitespace-only ISRC does NOT call /v1/resolve/isrc/
/// and directly queries /v1/stream.
#[tokio::test]
async fn test_adversarial_whitespace_isrc_bypasses_isrc_lookup() {
    let daemon1 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_error("/v1/resolve/isrc/", 500, "SHOULD_NOT_BE_CALLED", "Empty ISRC should not query endpoint")
        .with_stream_ok(FIXTURE_STREAM_24_192_FLAC)
        .start()
        .await;

    let mock_tier3 = MockInnerTubeProvider::new();

    let router = DynamicFallbackRouter::new(
        Some(Arc::new(EchoExtensionProvider::new(daemon1.base_url()))),
        None,
        Arc::new(mock_tier3),
    );

    let track = Track {
        id: "adv_trk_ws_isrc".into(),
        title: "Get Lucky".into(),
        artist: "Daft Punk".into(),
        album: None,
        duration_seconds: 369,
        thumbnail_url: None,
        isrc: Some("   \t\n  ".into()),
    };

    let stream = router
        .resolve_best_stream(&track)
        .await
        .expect("Whitespace ISRC should resolve via /v1/stream");

    assert_eq!(stream.fidelity, Some(AudioFidelity::LosslessHiRes));

    let d1_reqs = daemon1.recorded_requests().await;
    let isrc_calls = d1_reqs.iter().filter(|r| r.path.contains("/resolve/isrc")).count();
    assert_eq!(isrc_calls, 0, "ISRC endpoint must NOT be called for whitespace ISRC");
}

/// Adv 9: Full Multi-Step Fallback Cascade (ISRC 404 -> Tier 1 Stream 416 -> Tier 2 404 -> Tier 3 OK)
/// Verifies a complete 4-step cascading chain across all endpoints and tiers.
#[tokio::test]
async fn test_adversarial_full_four_step_cascade() {
    let isrc = "USQX91300105";

    let daemon1 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_error(format!("/v1/resolve/isrc/{isrc}"), 404, "TRACK_NOT_FOUND", "Step 1: ISRC 404")
        .with_error("/v1/stream", 416, "QUALITY_UNAVAILABLE", "Step 2: Stream 416")
        .start()
        .await;

    let daemon2 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_error("/v1/stream", 404, "TRACK_NOT_FOUND", "Step 3: Tier 2 404")
        .start()
        .await;

    let mock_tier3 = MockInnerTubeProvider::new();

    let router = DynamicFallbackRouter::new(
        Some(Arc::new(EchoExtensionProvider::new(daemon1.base_url()))),
        Some(Arc::new(EchoExtensionProvider::new(daemon2.base_url()))),
        Arc::new(mock_tier3.clone()),
    );

    let track = Track {
        id: "adv_four_step_trk".into(),
        title: "Get Lucky".into(),
        artist: "Daft Punk".into(),
        album: None,
        duration_seconds: 369,
        thumbnail_url: None,
        isrc: Some(isrc.into()),
    };

    let stream = router
        .resolve_best_stream(&track)
        .await
        .expect("Should cascade all 4 steps to Tier 3");

    assert_eq!(stream.fidelity, Some(AudioFidelity::LossyStandard));
    assert_eq!(mock_tier3.resolve_count(), 1);

    // Verify Step 1 & Step 2 were attempted on Tier 1
    let d1_reqs = daemon1.recorded_requests().await;
    assert!(d1_reqs.iter().any(|r| r.path.contains("/resolve/isrc")));
    assert!(d1_reqs.iter().any(|r| r.path.starts_with("/v1/stream")));

    // Verify Step 3 was attempted on Tier 2
    let d2_reqs = daemon2.recorded_requests().await;
    assert!(d2_reqs.iter().any(|r| r.path.starts_with("/v1/stream")));
}

/// Adv 10: Special Characters and Query Parameter Escaping
/// Verifies: Tracks with UTF-8, punctuation, and URL special characters in title/artist
/// do not corrupt the query strings when cascading to Tier 2.
#[tokio::test]
async fn test_adversarial_special_characters_url_encoding() {
    let daemon1 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_error("/v1/stream", 404, "TRACK_NOT_FOUND", "Missing")
        .start()
        .await;

    let daemon2 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_stream_ok(MockExtensionDaemon::fixture_stream_320k_mp3())
        .start()
        .await;

    let mock_tier3 = MockInnerTubeProvider::new();

    let router = DynamicFallbackRouter::new(
        Some(Arc::new(EchoExtensionProvider::new(daemon1.base_url()))),
        Some(Arc::new(EchoExtensionProvider::new(daemon2.base_url()))),
        Arc::new(mock_tier3),
    );

    let weird_title = "AC/DC - Back in Black & Blue? (Live 100% #1) / [Remastered] 🎵";
    let weird_artist = "AC/DC & Guns N' Roses";

    let track = Track::new("adv_trk_special_chars", weird_title, weird_artist, None, 255, None);

    let stream = router
        .resolve_best_stream(&track)
        .await
        .expect("Resolution with special characters should succeed");

    assert_eq!(stream.codec, AudioCodec::Mp3);
    assert_eq!(stream.fidelity, Some(AudioFidelity::Lossy320));

    let d2_reqs = daemon2.recorded_requests().await;
    let s_req = d2_reqs.iter().find(|r| r.path.starts_with("/v1/stream")).unwrap();
    assert_eq!(s_req.query_params.get("title").map(|s| s.as_str()), Some(weird_title));
    assert_eq!(s_req.query_params.get("artist").map(|s| s.as_str()), Some(weird_artist));
    assert_eq!(s_req.query_params.get("quality").map(|s| s.as_str()), Some("high_320"));
}

// =========================================================================
// CONCURRENCY, STRESS & BOUNDARY VERIFICATION SUITE (challenger_m2_2)
// =========================================================================

/// Stress 1: High Concurrency — 100 Parallel Tasks on Shared Arc<DynamicFallbackRouter>
/// 50 tasks resolve Tier 1 FLAC via ISRC, 50 tasks cascade to Tier 2 320kbps MP3.
/// Verifies no deadlocks, race conditions, socket dropouts, or cross-task response leaks.
#[tokio::test]
async fn test_stress_concurrency_100_parallel_stream_resolutions() {
    let mut d1_builder = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_error("/v1/stream", 404, "TRACK_NOT_FOUND", "Cascade to Tier 2");

    for i in 0..50 {
        d1_builder = d1_builder.with_isrc_ok(&format!("ISRC_T1_{:03}", i), FIXTURE_STREAM_24_192_FLAC);
    }

    let daemon1 = d1_builder.start().await;

    let daemon2 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_stream_ok(MockExtensionDaemon::fixture_stream_320k_mp3())
        .start()
        .await;

    let mock_tier3 = MockInnerTubeProvider::new();

    let router = Arc::new(DynamicFallbackRouter::new(
        Some(Arc::new(EchoExtensionProvider::new(daemon1.base_url()))),
        Some(Arc::new(EchoExtensionProvider::new(daemon2.base_url()))),
        Arc::new(mock_tier3.clone()),
    ));

    let mut tasks = Vec::with_capacity(100);

    for i in 0..100 {
        let r = Arc::clone(&router);
        let task = tokio::spawn(async move {
            if i % 2 == 0 {
                // Tier 1 resolution via ISRC
                let isrc_idx = i / 2;
                let track = Track {
                    id: format!("t1_trk_{:03}", isrc_idx),
                    title: format!("Tier 1 Track {:03}", isrc_idx),
                    artist: "Artist 1".into(),
                    album: None,
                    duration_seconds: 240,
                    thumbnail_url: None,
                    isrc: Some(format!("ISRC_T1_{:03}", isrc_idx)),
                };
                let stream = r.resolve_best_stream(&track).await.unwrap_or_else(|e| {
                    panic!("Task {} (Tier 1) failed: {:?}", i, e);
                });
                assert_eq!(stream.codec, AudioCodec::Flac);
                assert_eq!(stream.sample_rate_hz, 192000);
                assert_eq!(stream.fidelity, Some(AudioFidelity::LosslessHiRes));
            } else {
                // Tier 2 cascade (no ISRC, Tier 1 returns 404)
                let t2_idx = i / 2;
                let track = Track {
                    id: format!("t2_trk_{:03}", t2_idx),
                    title: format!("Tier 2 Track {:03}", t2_idx),
                    artist: "Artist 2".into(),
                    album: None,
                    duration_seconds: 180,
                    thumbnail_url: None,
                    isrc: None,
                };
                let stream = r.resolve_best_stream(&track).await.unwrap_or_else(|e| {
                    panic!("Task {} (Tier 2) failed: {:?}", i, e);
                });
                assert_eq!(stream.codec, AudioCodec::Mp3);
                assert_eq!(stream.bitrate_kbps, 320);
                assert_eq!(stream.fidelity, Some(AudioFidelity::Lossy320));
            }
        });
        tasks.push(task);
    }

    for task in tasks {
        task.await.expect("Concurrent resolution task must not panic");
    }

    assert_eq!(
        mock_tier3.resolve_count(),
        0,
        "Tier 3 InnerTube should never be called when Tier 1 and Tier 2 resolve all streams"
    );
}

/// Stress 2: Concurrency Contention — Parallel Mixed Operations (Search, Health, Stream Resolution)
/// 20 tasks running search(), 20 tasks running check_health(), 20 tasks running resolve_best_stream()
#[tokio::test]
async fn test_stress_concurrency_60_mixed_api_operations() {
    let daemon1 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_stream_ok(FIXTURE_STREAM_24_192_FLAC)
        .start()
        .await;

    let daemon2 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_stream_ok(MockExtensionDaemon::fixture_stream_320k_mp3())
        .start()
        .await;

    let search_tracks = vec![
        Track::new("yt_res_1", "Search Track 1", "Artist", None, 200, None),
        Track::new("yt_res_2", "Search Track 2", "Artist", None, 220, None),
    ];
    let mock_tier3 = MockInnerTubeProvider::new().with_search_results(search_tracks);

    let router = Arc::new(DynamicFallbackRouter::new(
        Some(Arc::new(EchoExtensionProvider::new(daemon1.base_url()))),
        Some(Arc::new(EchoExtensionProvider::new(daemon2.base_url()))),
        Arc::new(mock_tier3),
    ));

    let mut tasks = Vec::with_capacity(60);

    for i in 0..60 {
        let r = Arc::clone(&router);
        let task = tokio::spawn(async move {
            match i % 3 {
                0 => {
                    let report = r.check_health().await;
                    assert!(report.tier1_lossless_configured);
                    assert!(report.tier1_lossless_healthy);
                    assert!(report.tier2_highbitrate_configured);
                    assert!(report.tier2_highbitrate_healthy);
                    assert!(report.tier3_innertube_available);
                }
                1 => {
                    let results = r.search(&format!("query_{}", i)).await.unwrap();
                    assert_eq!(results.len(), 2);
                }
                2 => {
                    let track = Track::new(format!("trk_res_{}", i), "Title", "Artist", None, 180, None);
                    let stream = r.resolve_best_stream(&track).await.unwrap();
                    assert_eq!(stream.codec, AudioCodec::Flac);
                    assert_eq!(stream.fidelity, Some(AudioFidelity::LosslessHiRes));
                }
                _ => unreachable!(),
            }
        });
        tasks.push(task);
    }

    for task in tasks {
        task.await.expect("Concurrent task must complete cleanly");
    }
}

/// Boundary 1: Complete ISRC Matrix — None, Empty, Whitespace, Padded
/// Verifies: None, "", "   ", "\t\n\r" all bypass ISRC direct resolution and proceed to /v1/stream.
/// Padded ISRC ("  USQX91300105  ") trims whitespace and calls /v1/resolve/isrc/USQX91300105 directly.
#[tokio::test]
async fn test_boundary_isrc_matrix_empty_whitespace_and_trimming() {
    let valid_isrc = "USQX91300105";

    let daemon1 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_isrc_ok(valid_isrc, FIXTURE_STREAM_24_192_FLAC)
        .with_stream_ok(FIXTURE_STREAM_16_44_FLAC)
        .start()
        .await;

    let mock_tier3 = MockInnerTubeProvider::new();

    let router = DynamicFallbackRouter::new(
        Some(Arc::new(EchoExtensionProvider::new(daemon1.base_url()))),
        None,
        Arc::new(mock_tier3),
    );

    let test_cases = vec![
        (None, AudioFidelity::LosslessCD, false),
        (Some(""), AudioFidelity::LosslessCD, false),
        (Some("   "), AudioFidelity::LosslessCD, false),
        (Some("\t \n \r  "), AudioFidelity::LosslessCD, false),
        (Some("  USQX91300105  "), AudioFidelity::LosslessHiRes, true),
    ];

    for (isrc, expected_fidelity, used_isrc) in test_cases {
        let track = Track {
            id: "bnd_isrc_trk".into(),
            title: "Boundary Track".into(),
            artist: "Artist".into(),
            album: None,
            duration_seconds: 200,
            thumbnail_url: None,
            isrc: isrc.map(|s| s.to_string()),
        };

        let stream = router.resolve_best_stream(&track).await.unwrap();
        assert_eq!(stream.fidelity, Some(expected_fidelity));

        if used_isrc {
            assert_eq!(stream.sample_rate_hz, 192000);
            assert_eq!(stream.bit_depth, Some(24));
        } else {
            assert_eq!(stream.sample_rate_hz, 44100);
            assert_eq!(stream.bit_depth, Some(16));
        }
    }

    let reqs = daemon1.recorded_requests().await;
    let isrc_calls: Vec<_> = reqs.iter().filter(|r| r.path.starts_with("/v1/resolve/isrc/")).collect();
    assert_eq!(isrc_calls.len(), 1, "Only trimmed valid ISRC must call /v1/resolve/isrc/");
    assert_eq!(isrc_calls[0].path, format!("/v1/resolve/isrc/{}", valid_isrc));

    let stream_calls: Vec<_> = reqs.iter().filter(|r| r.path.starts_with("/v1/stream")).collect();
    assert_eq!(stream_calls.len(), 4, "The 4 empty/whitespace/None cases must each query /v1/stream");
}

/// Boundary 2: Extreme Length Strings (>1000 chars in Title, Artist, Album)
/// Verifies no buffer overrun, stack overflow, client crash, or URI encoding failures.
#[tokio::test]
async fn test_boundary_metadata_very_long_title_and_artist() {
    let daemon1 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_stream_ok(FIXTURE_STREAM_24_192_FLAC)
        .start()
        .await;

    let mock_tier3 = MockInnerTubeProvider::new();

    let router = DynamicFallbackRouter::new(
        Some(Arc::new(EchoExtensionProvider::new(daemon1.base_url()))),
        None,
        Arc::new(mock_tier3),
    );

    let long_title = "TitleSegment_".repeat(120); // 1560 characters
    let long_artist = "ArtistSegment_".repeat(60); // 840 characters
    let long_album = "AlbumSegment_".repeat(80);   // 1040 characters

    let track = Track {
        id: "adv_long_strings".into(),
        title: long_title.clone(),
        artist: long_artist.clone(),
        album: Some(long_album),
        duration_seconds: 500,
        thumbnail_url: None,
        isrc: None,
    };

    let stream = router
        .resolve_best_stream(&track)
        .await
        .expect("Long string track must resolve successfully");

    assert_eq!(stream.codec, AudioCodec::Flac);
    assert_eq!(stream.fidelity, Some(AudioFidelity::LosslessHiRes));

    let reqs = daemon1.recorded_requests().await;
    let s_req = reqs.iter().find(|r| r.path.starts_with("/v1/stream")).unwrap();
    assert_eq!(s_req.query_params.get("title").map(|s| s.as_str()), Some(long_title.as_str()));
    assert_eq!(s_req.query_params.get("artist").map(|s| s.as_str()), Some(long_artist.as_str()));
}

/// Boundary 3: Unicode, Multilingual Scripts & Emoji Preservation
/// Verifies multi-byte UTF-8 sequences (Japanese, Cyrillic, Arabic, Emoji) are properly preserved.
#[tokio::test]
async fn test_boundary_metadata_unicode_multilingual_and_emojis() {
    let daemon1 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_stream_ok(FIXTURE_STREAM_24_192_FLAC)
        .start()
        .await;

    let mock_tier3 = MockInnerTubeProvider::new();

    let router = DynamicFallbackRouter::new(
        Some(Arc::new(EchoExtensionProvider::new(daemon1.base_url()))),
        None,
        Arc::new(mock_tier3),
    );

    let uni_title = "🎵 夢のしずく (Drops of Dreams) - Сказка 🌟 العربية 中文 🎧";
    let uni_artist = "松たか子 & Чайковский & Fairouz 👑";
    let uni_album = "Global Harmony (世界調和) [Vol. 42] 🌍";

    let track = Track {
        id: "adv_uni_trk".into(),
        title: uni_title.into(),
        artist: uni_artist.into(),
        album: Some(uni_album.into()),
        duration_seconds: 320,
        thumbnail_url: None,
        isrc: None,
    };

    let stream = router
        .resolve_best_stream(&track)
        .await
        .expect("Multilingual unicode track must resolve cleanly");

    assert_eq!(stream.codec, AudioCodec::Flac);
    assert_eq!(stream.fidelity, Some(AudioFidelity::LosslessHiRes));

    let reqs = daemon1.recorded_requests().await;
    let s_req = reqs.iter().find(|r| r.path.starts_with("/v1/stream")).unwrap();
    assert_eq!(s_req.query_params.get("title").map(|s| s.as_str()), Some(uni_title));
    assert_eq!(s_req.query_params.get("artist").map(|s| s.as_str()), Some(uni_artist));
}

/// Boundary 4: Empty / Minimal Track Fields
/// Verifies that empty strings for title, artist, and None album/isrc do not cause errors.
#[tokio::test]
async fn test_boundary_metadata_empty_minimal_fields() {
    let daemon1 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_stream_ok(FIXTURE_STREAM_16_44_FLAC)
        .start()
        .await;

    let mock_tier3 = MockInnerTubeProvider::new();

    let router = DynamicFallbackRouter::new(
        Some(Arc::new(EchoExtensionProvider::new(daemon1.base_url()))),
        None,
        Arc::new(mock_tier3),
    );

    let track = Track {
        id: "trk_minimal".into(),
        title: "".into(),
        artist: "".into(),
        album: None,
        duration_seconds: 0,
        thumbnail_url: None,
        isrc: None,
    };

    let stream = router
        .resolve_best_stream(&track)
        .await
        .expect("Minimal empty track must resolve successfully");

    assert_eq!(stream.codec, AudioCodec::Flac);
    assert_eq!(stream.fidelity, Some(AudioFidelity::LosslessCD));

    let reqs = daemon1.recorded_requests().await;
    let s_req = reqs.iter().find(|r| r.path.starts_with("/v1/stream")).unwrap();
    assert_eq!(s_req.query_params.get("id").map(|s| s.as_str()), Some("trk_minimal"));
    assert_eq!(s_req.query_params.get("quality").map(|s| s.as_str()), Some("lossless_max"));
    assert!(!s_req.query_params.contains_key("title"), "Empty title must not be added to query params");
    assert!(!s_req.query_params.contains_key("artist"), "Empty artist must not be added to query params");
}

/// Boundary 5: Exotic ISRC with URI Special Characters
/// When an exotic ISRC like "US/123?45#67" causes 404 on /v1/resolve/isrc, router falls back to /v1/stream.
#[tokio::test]
async fn test_boundary_exotic_isrc_special_characters_resilience() {
    let exotic_isrc = "US/123?45#67";

    let daemon1 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_stream_ok(FIXTURE_STREAM_24_192_FLAC)
        .start()
        .await;

    let mock_tier3 = MockInnerTubeProvider::new();

    let router = DynamicFallbackRouter::new(
        Some(Arc::new(EchoExtensionProvider::new(daemon1.base_url()))),
        None,
        Arc::new(mock_tier3),
    );

    let track = Track {
        id: "trk_exotic_isrc".into(),
        title: "Exotic Track".into(),
        artist: "Exotic Artist".into(),
        album: None,
        duration_seconds: 210,
        thumbnail_url: None,
        isrc: Some(exotic_isrc.into()),
    };

    let stream = router
        .resolve_best_stream(&track)
        .await
        .expect("Router should fall back to /v1/stream and resolve successfully");

    assert_eq!(stream.codec, AudioCodec::Flac);
    assert_eq!(stream.fidelity, Some(AudioFidelity::LosslessHiRes));
}

/// Exhaustion 1: Deterministic Error Mapping When Extensions Are None
/// Tests when Tier 1 = None, Tier 2 = None, and Tier 3 returns various ProviderError variants.
/// DynamicFallbackRouter contract requires mapping to ProviderError::NoStreamsAvailable containing track details.
#[tokio::test]
async fn test_exhaustion_none_extensions_deterministic_error_mapping() {
    let error_scenarios: Vec<(&'static str, Box<dyn Fn(&Track) -> ProviderError + Send + Sync>)> = vec![
        (
            "NoStreamsAvailable",
            Box::new(|t| ProviderError::NoStreamsAvailable(t.id.clone())),
        ),
        (
            "TrackNotFound",
            Box::new(|t| ProviderError::TrackNotFound(t.id.clone())),
        ),
        (
            "Timeout",
            Box::new(|_| ProviderError::Timeout("InnerTube request timed out".into())),
        ),
        (
            "RateLimited",
            Box::new(|_| ProviderError::RateLimited),
        ),
        (
            "ConnectionRefused",
            Box::new(|_| ProviderError::ConnectionRefused("TCP reset from proxy".into())),
        ),
    ];

    for (name, error_gen) in error_scenarios {
        let mock_tier3 = MockInnerTubeProvider::new().with_stream_handler(move |t| Err(error_gen(t)));

        let router = DynamicFallbackRouter::new(None, None, Arc::new(mock_tier3));

        let track = Track::new("exh_trk_none", "Deterministic Test Song", "Artist", None, 180, None);

        let err = router
            .resolve_best_stream(&track)
            .await
            .expect_err("Scenario must return Err");

        match err {
            ProviderError::NoStreamsAvailable(msg) => {
                assert!(
                    msg.contains("Deterministic Test Song"),
                    "Scenario {} message must contain track title: {}",
                    name,
                    msg
                );
                assert!(
                    msg.contains("exh_trk_none"),
                    "Scenario {} message must contain track id: {}",
                    name,
                    msg
                );
                assert!(
                    msg.contains("exhausted"),
                    "Scenario {} message must state exhausted: {}",
                    name,
                    msg
                );
            }
            other => panic!("Scenario {} expected NoStreamsAvailable, got: {:?}", name, other),
        }
    }
}

/// Exhaustion 2: Multi-Tier Failure Matrix (502 -> 429 -> Connection Refused -> Deterministic NoStreamsAvailable)
#[tokio::test]
async fn test_exhaustion_full_tier_matrix_deterministic_error() {
    let daemon1 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_error("/v1/stream", 502, "UPSTREAM_FAILED", "Tier 1 CDN down")
        .start()
        .await;

    let daemon2 = MockExtensionDaemon::builder()
        .with_health_ok(MockExtensionDaemon::fixture_health_healthy())
        .with_error("/v1/stream", 429, "RATE_LIMITED", "Tier 2 rate limit hit")
        .start()
        .await;

    let mock_tier3 = MockInnerTubeProvider::new().with_stream_handler(|track| {
        Err(ProviderError::ConnectionRefused(format!("InnerTube unreachable for {}", track.id)))
    });

    let router = DynamicFallbackRouter::new(
        Some(Arc::new(EchoExtensionProvider::new(daemon1.base_url()))),
        Some(Arc::new(EchoExtensionProvider::new(daemon2.base_url()))),
        Arc::new(mock_tier3),
    );

    let track = Track::new("exh_matrix_id", "The Final Curtain", "Frank", None, 300, None);

    let err = router
        .resolve_best_stream(&track)
        .await
        .expect_err("Must return deterministic NoStreamsAvailable when all tiers fail");

    match err {
        ProviderError::NoStreamsAvailable(msg) => {
            assert!(msg.contains("The Final Curtain"));
            assert!(msg.contains("exh_matrix_id"));
            assert!(msg.contains("exhausted"));
        }
        other => panic!("Expected NoStreamsAvailable, got: {:?}", other),
    }
}

/// Exhaustion 3: Offline Loopback Daemons + InnerTube Failure
/// Both Tier 1 & 2 closed loopback ports -> bypassed -> Tier 3 fails -> NoStreamsAvailable
#[tokio::test]
async fn test_exhaustion_all_tiers_offline_deterministic_error() {
    let offline_t1 = MockExtensionDaemon::unused_base_url().await;
    let offline_t2 = MockExtensionDaemon::unused_base_url().await;

    let mock_tier3 = MockInnerTubeProvider::new().with_no_streams();

    let router = DynamicFallbackRouter::new(
        Some(Arc::new(EchoExtensionProvider::new(offline_t1))),
        Some(Arc::new(EchoExtensionProvider::new(offline_t2))),
        Arc::new(mock_tier3),
    );

    let track = Track::new("exh_offline_id", "Ghost Signal", "Lost", None, 150, None);

    let err = router
        .resolve_best_stream(&track)
        .await
        .expect_err("All offline daemons + failed Tier 3 must exhaust deterministically");

    match err {
        ProviderError::NoStreamsAvailable(msg) => {
            assert!(msg.contains("Ghost Signal"));
            assert!(msg.contains("exh_offline_id"));
        }
        other => panic!("Expected NoStreamsAvailable, got: {:?}", other),
    }
}
