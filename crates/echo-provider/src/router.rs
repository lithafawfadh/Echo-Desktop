//! Multi-tiered dynamic stream fallback orchestrator.
//!
//! Orchestrates audio stream resolution across fidelity tiers:
//! - Tier 1: `lossless-extension` (FLAC 24-bit/192kHz or 16-bit/44.1kHz CD)
//! - Tier 2: `320kbs-extension` (MP3 / AAC 320kbps CBR)
//! - Tier 3: `YouTubeMusicClient` (InnerTube AAC 128kbps itag 140 / Opus 160kbps)
//!
//! Provides dynamic health bypass for offline or degraded microservices,
//! in-flight error cascading, and universal catalog search delegation.

use async_trait::async_trait;
use echo_core::{AudioFidelity, AudioStream, Track};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, instrument, warn};

use crate::error::ProviderError;
use crate::extension::EchoExtensionProvider;
use crate::innertube::YouTubeMusicClient;
use crate::MusicProvider;

/// Canonical default URL for Tier 1 lossless extension daemon.
pub const DEFAULT_LOSSLESS_URL: &str = "http://127.0.0.1:45300/v1";

/// Canonical default URL for Tier 2 320kbps high-bitrate extension daemon.
pub const DEFAULT_HIGHBITRATE_URL: &str = "http://127.0.0.1:45301/v1";

/// Default timeout for proactive health check discovery probes.
pub const DEFAULT_HEALTH_TIMEOUT: Duration = Duration::from_millis(500);

/// Health report snapshot across all configured router tiers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouterHealthReport {
    pub tier1_lossless_configured: bool,
    pub tier1_lossless_healthy: bool,
    pub tier2_highbitrate_configured: bool,
    pub tier2_highbitrate_healthy: bool,
    pub tier3_innertube_available: bool,
}

/// Multi-tier fallback router managing lossless, high-bitrate, and baseline stream providers.
#[derive(Clone)]
pub struct DynamicFallbackRouter {
    tier1_lossless: Option<Arc<EchoExtensionProvider>>,
    tier2_highbitrate: Option<Arc<EchoExtensionProvider>>,
    tier3_innertube: Arc<dyn MusicProvider>,
    health_timeout: Duration,
}

impl DynamicFallbackRouter {
    /// Constructs a router with explicitly configured provider tiers.
    pub fn new(
        tier1_lossless: Option<Arc<EchoExtensionProvider>>,
        tier2_highbitrate: Option<Arc<EchoExtensionProvider>>,
        tier3_innertube: Arc<dyn MusicProvider>,
    ) -> Self {
        Self {
            tier1_lossless,
            tier2_highbitrate,
            tier3_innertube,
            health_timeout: DEFAULT_HEALTH_TIMEOUT,
        }
    }

    /// Sets a custom timeout for proactive health check probes.
    pub fn with_health_timeout(mut self, timeout: Duration) -> Self {
        self.health_timeout = timeout;
        self
    }

    /// Constructs a router with production defaults:
    /// - Tier 1: `http://127.0.0.1:45300/v1`
    /// - Tier 2: `http://127.0.0.1:45301/v1`
    /// - Tier 3: Default `YouTubeMusicClient`
    pub fn with_defaults() -> Result<Self, ProviderError> {
        let tier1 = Some(Arc::new(EchoExtensionProvider::new(DEFAULT_LOSSLESS_URL)));
        let tier2 = Some(Arc::new(EchoExtensionProvider::new(DEFAULT_HIGHBITRATE_URL)));
        let tier3 = Arc::new(YouTubeMusicClient::new()?);

        Ok(Self::new(tier1, tier2, tier3))
    }

    /// Constructs a router with custom extension base URLs, useful for in-process testing.
    pub fn with_custom_urls(
        tier1_url: Option<impl Into<String>>,
        tier2_url: Option<impl Into<String>>,
        tier3_innertube: Arc<dyn MusicProvider>,
    ) -> Self {
        let tier1 = tier1_url.map(|u| Arc::new(EchoExtensionProvider::new(u)));
        let tier2 = tier2_url.map(|u| Arc::new(EchoExtensionProvider::new(u)));
        Self::new(tier1, tier2, tier3_innertube)
    }

    /// Returns a reference to the Tier 1 lossless extension provider, if configured.
    pub fn tier1_lossless(&self) -> Option<&Arc<EchoExtensionProvider>> {
        self.tier1_lossless.as_ref()
    }

    /// Returns a reference to the Tier 2 high-bitrate extension provider, if configured.
    pub fn tier2_highbitrate(&self) -> Option<&Arc<EchoExtensionProvider>> {
        self.tier2_highbitrate.as_ref()
    }

    /// Returns a reference to the Tier 3 InnerTube provider.
    pub fn tier3_innertube(&self) -> &Arc<dyn MusicProvider> {
        &self.tier3_innertube
    }

    /// Returns the configured health timeout duration.
    pub fn health_timeout(&self) -> Duration {
        self.health_timeout
    }

    /// Alias for `tier1_lossless()`.
    pub fn tier1(&self) -> Option<&Arc<EchoExtensionProvider>> {
        self.tier1_lossless.as_ref()
    }

    /// Alias for `tier2_highbitrate()`.
    pub fn tier2(&self) -> Option<&Arc<EchoExtensionProvider>> {
        self.tier2_highbitrate.as_ref()
    }

    /// Alias for `tier3_innertube()`.
    pub fn tier3(&self) -> &Arc<dyn MusicProvider> {
        &self.tier3_innertube
    }

    /// Checks whether an extension daemon is healthy and available within the health timeout.
    pub async fn is_tier_healthy(&self, provider: &EchoExtensionProvider, tier_name: &str) -> bool {
        match tokio::time::timeout(self.health_timeout, provider.check_health()).await {
            Ok(Ok(health)) => {
                if health.is_healthy() {
                    debug!(
                        tier = tier_name,
                        url = %provider.base_url(),
                        status = %health.status,
                        name = %health.name,
                        "Extension daemon verified healthy"
                    );
                    true
                } else {
                    warn!(
                        tier = tier_name,
                        url = %provider.base_url(),
                        status = %health.status,
                        name = %health.name,
                        "Extension daemon reported non-healthy status; bypassing tier"
                    );
                    false
                }
            }
            Ok(Err(err)) => {
                warn!(
                    tier = tier_name,
                    url = %provider.base_url(),
                    error = %err,
                    "Extension daemon health check failed (offline/unreachable); bypassing tier"
                );
                false
            }
            Err(_) => {
                warn!(
                    tier = tier_name,
                    url = %provider.base_url(),
                    timeout_ms = self.health_timeout.as_millis(),
                    "Extension daemon health check timed out; bypassing tier"
                );
                false
            }
        }
    }

    /// Queries health status across all configured extension tiers.
    pub async fn check_health(&self) -> RouterHealthReport {
        let tier1_healthy = match self.tier1_lossless {
            Some(ref ext) => self.is_tier_healthy(ext, "Tier 1 (Lossless)").await,
            None => false,
        };

        let tier2_healthy = match self.tier2_highbitrate {
            Some(ref ext) => self.is_tier_healthy(ext, "Tier 2 (320kbps)").await,
            None => false,
        };

        RouterHealthReport {
            tier1_lossless_configured: self.tier1_lossless.is_some(),
            tier1_lossless_healthy: tier1_healthy,
            tier2_highbitrate_configured: self.tier2_highbitrate.is_some(),
            tier2_highbitrate_healthy: tier2_healthy,
            tier3_innertube_available: true,
        }
    }

    /// Primary resolution engine: Attempts highest fidelity resolution first,
    /// dynamically bypassing offline or unhealthy daemons, and cascading to
    /// lower fidelity tiers upon failure.
    #[instrument(skip(self), fields(track_id = %track.id, track_title = %track.title))]
    pub async fn resolve_best_stream(&self, track: &Track) -> Result<AudioStream, ProviderError> {
        // -----------------------------------------------------------------
        // Tier 1: Lossless Extension (FLAC 24-bit/192kHz or 16-bit/44.1kHz CD)
        // -----------------------------------------------------------------
        if let Some(ref tier1) = self.tier1_lossless {
            debug!(
                track_id = %track.id,
                title = %track.title,
                isrc = ?track.isrc,
                "Evaluating Tier 1 [Lossless Extension]"
            );

            if self.is_tier_healthy(tier1, "Tier 1 (Lossless)").await {
                let mut tier1_stream: Option<AudioStream> = None;

                // Step 1A: ISRC Direct Resolution (if ISRC present)
                if let Some(ref isrc) = track.isrc {
                    let trimmed_isrc = isrc.trim();
                    if !trimmed_isrc.is_empty() {
                        debug!(isrc = %trimmed_isrc, "Attempting Tier 1 ISRC direct resolution");
                        match tier1.resolve_isrc(trimmed_isrc).await {
                            Ok(stream) => {
                                info!(
                                    isrc = %trimmed_isrc,
                                    codec = ?stream.codec,
                                    sample_rate_hz = stream.sample_rate_hz,
                                    bit_depth = ?stream.bit_depth,
                                    bitrate_kbps = stream.bitrate_kbps,
                                    "Tier 1 matched via ISRC direct lookup"
                                );
                                tier1_stream = Some(stream);
                            }
                            Err(err) => {
                                if err.is_not_found() {
                                    debug!(
                                        isrc = %trimmed_isrc,
                                        "Tier 1 ISRC lookup returned 404; falling back to /v1/stream metadata query"
                                    );
                                } else if err.is_connection_failure() || matches!(err, ProviderError::RateLimited) {
                                    warn!(
                                        error = %err,
                                        "Tier 1 fatal network/rate-limit error on ISRC lookup; cascading to Tier 2"
                                    );
                                    // Daemon is unreachable or rate-limited; skip /v1/stream
                                } else {
                                    warn!(
                                        error = %err,
                                        "Tier 1 ISRC lookup failed; attempting /v1/stream metadata query"
                                    );
                                }
                            }
                        }
                    }
                }

                // Step 1B: Fallback to /v1/stream with quality=lossless_max
                if tier1_stream.is_none() {
                    debug!("Attempting Tier 1 /v1/stream resolution (quality=lossless_max)");
                    match tier1.resolve_stream_with_quality(track, "lossless_max").await {
                        Ok(stream) => {
                            info!(
                                codec = ?stream.codec,
                                sample_rate_hz = stream.sample_rate_hz,
                                bit_depth = ?stream.bit_depth,
                                bitrate_kbps = stream.bitrate_kbps,
                                "Tier 1 [Lossless] stream resolved successfully"
                            );
                            tier1_stream = Some(stream);
                        }
                        Err(err) => {
                            warn!(
                                error = %err,
                                track_id = %track.id,
                                "Tier 1 stream resolution failed; cascading to Tier 2"
                            );
                        }
                    }
                }

                if let Some(mut stream) = tier1_stream {
                    // Ensure fidelity classification
                    if stream.fidelity.is_none() {
                        let fidelity = if stream.is_lossless {
                            if stream.bit_depth.unwrap_or(16) > 16 || stream.sample_rate_hz > 48000 {
                                AudioFidelity::LosslessHiRes
                            } else {
                                AudioFidelity::LosslessCD
                            }
                        } else {
                            AudioFidelity::Lossy320
                        };
                        stream.fidelity = Some(fidelity);
                    }
                    return Ok(stream);
                }
            } else {
                warn!(
                    track_id = %track.id,
                    "Tier 1 extension offline or unhealthy; bypassing directly to Tier 2"
                );
            }
        }

        // -----------------------------------------------------------------
        // Tier 2: 320kbps High-Bitrate Extension (MP3 / AAC 320kbps CBR)
        // -----------------------------------------------------------------
        if let Some(ref tier2) = self.tier2_highbitrate {
            debug!(
                track_id = %track.id,
                title = %track.title,
                "Evaluating Tier 2 [320kbs-extension]"
            );

            if self.is_tier_healthy(tier2, "Tier 2 (320kbps)").await {
                debug!("Attempting Tier 2 resolution (quality=high_320)");
                match tier2.resolve_stream_with_quality(track, "high_320").await {
                    Ok(mut stream) => {
                        info!(
                            codec = ?stream.codec,
                            bitrate_kbps = stream.bitrate_kbps,
                            sample_rate_hz = stream.sample_rate_hz,
                            "Tier 2 [320kbps] stream resolved successfully"
                        );
                        if stream.fidelity.is_none() {
                            stream.fidelity = Some(AudioFidelity::Lossy320);
                        }
                        return Ok(stream);
                    }
                    Err(err) => {
                        warn!(
                            error = %err,
                            track_id = %track.id,
                            "Tier 2 stream resolution failed; cascading to Tier 3/4 InnerTube"
                        );
                    }
                }
            } else {
                warn!(
                    track_id = %track.id,
                    "Tier 2 extension offline or unhealthy; bypassing directly to Tier 3"
                );
            }
        }

        // -----------------------------------------------------------------
        // Tier 3/4: YouTube Music InnerTube (Opus 160kbps / AAC 128kbps)
        // -----------------------------------------------------------------
        debug!(
            track_id = %track.id,
            title = %track.title,
            "Evaluating Tier 3/4 [YouTube Music InnerTube]"
        );
        match self.tier3_innertube.resolve_stream(track).await {
            Ok(mut stream) => {
                info!(
                    codec = ?stream.codec,
                    bitrate_kbps = stream.bitrate_kbps,
                    sample_rate_hz = stream.sample_rate_hz,
                    "Tier 3/4 [InnerTube] baseline stream resolved successfully"
                );
                if stream.fidelity.is_none() {
                    stream.fidelity = Some(AudioFidelity::LossyStandard);
                }
                Ok(stream)
            }
            Err(err) => {
                error!(
                    error = %err,
                    track_id = %track.id,
                    "All stream resolution tiers exhausted"
                );
                Err(ProviderError::NoStreamsAvailable(format!(
                    "All stream resolution tiers exhausted for track '{}' (id: {})",
                    track.title, track.id
                )))
            }
        }
    }
}

impl Default for DynamicFallbackRouter {
    fn default() -> Self {
        let tier1 = Some(Arc::new(EchoExtensionProvider::new(DEFAULT_LOSSLESS_URL)));
        let tier2 = Some(Arc::new(EchoExtensionProvider::new(DEFAULT_HIGHBITRATE_URL)));
        let tier3 = Arc::new(YouTubeMusicClient::default());

        Self::new(tier1, tier2, tier3)
    }
}

#[async_trait]
impl MusicProvider for DynamicFallbackRouter {
    fn name(&self) -> &'static str {
        "EchoDynamicFallbackRouter"
    }

    fn supported_fidelity(&self) -> AudioFidelity {
        AudioFidelity::LosslessHiRes
    }

    /// Delegates search strictly to InnerTube for universal, exhaustive catalog completeness.
    async fn search(&self, query: &str) -> Result<Vec<Track>, ProviderError> {
        self.tier3_innertube.search(query).await
    }

    /// Resolves best available stream using multi-tier dynamic fallback cascade.
    async fn resolve_stream(&self, track: &Track) -> Result<AudioStream, ProviderError> {
        self.resolve_best_stream(track).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use echo_core::AudioCodec;

    struct DummyProvider {
        search_results: Vec<Track>,
        stream: Option<AudioStream>,
    }

    #[async_trait]
    impl MusicProvider for DummyProvider {
        fn name(&self) -> &'static str {
            "DummyProvider"
        }

        fn supported_fidelity(&self) -> AudioFidelity {
            AudioFidelity::LossyStandard
        }

        async fn search(&self, _query: &str) -> Result<Vec<Track>, ProviderError> {
            Ok(self.search_results.clone())
        }

        async fn resolve_stream(&self, track: &Track) -> Result<AudioStream, ProviderError> {
            self.stream
                .clone()
                .ok_or_else(|| ProviderError::TrackNotFound(track.id.clone()))
        }
    }

    #[test]
    fn test_router_metadata_and_fidelity() {
        let dummy = Arc::new(DummyProvider {
            search_results: Vec::new(),
            stream: None,
        });

        let router = DynamicFallbackRouter::new(None, None, dummy);
        assert_eq!(router.name(), "EchoDynamicFallbackRouter");
        assert_eq!(router.supported_fidelity(), AudioFidelity::LosslessHiRes);
        assert!(router.tier1_lossless().is_none());
        assert!(router.tier2_highbitrate().is_none());
    }

    #[tokio::test]
    async fn test_router_search_delegation() {
        let expected_track = Track::new("trk_1", "Solar", "Artist", None, 180, None);
        let dummy = Arc::new(DummyProvider {
            search_results: vec![expected_track.clone()],
            stream: None,
        });

        let router = DynamicFallbackRouter::new(None, None, dummy);
        let results = router.search("Solar").await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "trk_1");
    }

    #[tokio::test]
    async fn test_router_tier3_direct_fallback_when_extensions_none() {
        let stream = AudioStream {
            url: "https://googlevideo.com/stream.mp4".into(),
            codec: AudioCodec::Aac,
            bitrate_kbps: 128,
            sample_rate_hz: 44100,
            is_lossless: false,
            bit_depth: None,
            fidelity: Some(AudioFidelity::LossyStandard),
            content_length_bytes: None,
            http_headers: std::collections::HashMap::new(),
        };

        let dummy = Arc::new(DummyProvider {
            search_results: Vec::new(),
            stream: Some(stream),
        });

        let router = DynamicFallbackRouter::new(None, None, dummy);
        let track = Track::new("vid_1", "Solar", "Artist", None, 180, None);
        let resolved = router.resolve_stream(&track).await.unwrap();

        assert_eq!(resolved.fidelity, Some(AudioFidelity::LossyStandard));
        assert_eq!(resolved.bitrate_kbps, 128);
    }
}
