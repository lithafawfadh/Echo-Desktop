pub mod cipher;
pub mod error;
pub mod extension;
pub mod innertube;
pub mod router;

use async_trait::async_trait;
use echo_core::{AudioFidelity, AudioStream, Track};

pub use cipher::{
    deobfuscate_stream_url, descramble_signature, execute_cipher_ops, execute_cipher_ops_in_place,
    execute_n_ops, execute_n_ops_in_place, parse_player_script, transform_n_token,
    transform_stream_url_n, transform_stream_url_n_with_ops, CipherOp, NOp, PlayerCipherState,
    YouTubeCipherEngine, DEFAULT_SIGNATURE_TIMESTAMP,
};
pub use error::ProviderError;
pub use extension::{
    EchoExtensionProvider, ExtensionCapabilities, ExtensionErrorDetail, ExtensionErrorResponse,
    ExtensionHealth, ExtensionHealthResponse, ExtensionInfo, ExtensionQuality,
    ExtensionSearchResponse, ExtensionSearchResult, ExtensionStreamResponse,
};
pub use innertube::{
    build_client_context, create_innertube_http_client, parse_duration, parse_player_response,
    parse_search_response, parse_search_response_typed, resolve_candidate_to_stream,
    select_itag_140_stream, RawStreamCandidate, YouTubeMusicClient, INNERTUBE_BASE_URL,
    SONG_FILTER_PARAMS,
};
pub use router::{
    DynamicFallbackRouter, RouterHealthReport, DEFAULT_HIGHBITRATE_URL, DEFAULT_LOSSLESS_URL,
};

/// Trait abstracting music catalog search and audio stream resolution.
#[async_trait]
pub trait MusicProvider: Send + Sync {
    /// Returns the human-readable display name of this provider.
    fn name(&self) -> &'static str {
        "MusicProvider"
    }

    /// Returns the maximum audio fidelity supported by this provider.
    fn supported_fidelity(&self) -> AudioFidelity {
        AudioFidelity::LossyStandard
    }

    /// Searches tracks by a text query string.
    async fn search(&self, query: &str) -> Result<Vec<Track>, ProviderError>;

    /// Resolves a playable audio stream for a given track.
    async fn resolve_stream(&self, track: &Track) -> Result<AudioStream, ProviderError>;
}

/// Multi-provider router with fallback: checks lossless extensions first,
/// then falls back to secondary / standard stream provider.
pub struct FallbackRouter<P1: MusicProvider, P2: MusicProvider> {
    primary: Option<P1>,
    fallback: P2,
}

impl<P1: MusicProvider, P2: MusicProvider> FallbackRouter<P1, P2> {
    pub fn new(primary: Option<P1>, fallback: P2) -> Self {
        Self { primary, fallback }
    }
}

#[async_trait]
impl<P1: MusicProvider, P2: MusicProvider> MusicProvider for FallbackRouter<P1, P2> {
    fn name(&self) -> &'static str {
        "FallbackRouter"
    }
    async fn search(&self, query: &str) -> Result<Vec<Track>, ProviderError> {
        if let Some(ref primary) = self.primary {
            if let Ok(results) = primary.search(query).await {
                if !results.is_empty() {
                    return Ok(results);
                }
            }
        }
        self.fallback.search(query).await
    }

    async fn resolve_stream(&self, track: &Track) -> Result<AudioStream, ProviderError> {
        if let Some(ref primary) = self.primary {
            tracing::debug!("Attempting high-fidelity stream resolution for track {}", track.id);
            if let Ok(stream) = primary.resolve_stream(track).await {
                tracing::info!("High-fidelity stream resolved via extension ({} kbps)", stream.bitrate_kbps);
                return Ok(stream);
            }
        }

        tracing::debug!("Falling back to standard stream provider for {}", track.id);
        self.fallback.resolve_stream(track).await
    }
}
