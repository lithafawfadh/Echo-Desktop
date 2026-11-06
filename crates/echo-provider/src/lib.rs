pub mod cipher;
pub mod error;
pub mod innertube;

use async_trait::async_trait;
use echo_core::{AudioCodec, AudioStream, Track};
use serde::Deserialize;
use tracing::{debug, info};

pub use cipher::{
    deobfuscate_stream_url, descramble_signature, execute_cipher_ops, execute_cipher_ops_in_place,
    execute_n_ops, execute_n_ops_in_place, parse_player_script, transform_n_token,
    transform_stream_url_n, transform_stream_url_n_with_ops, CipherOp, NOp, PlayerCipherState,
    YouTubeCipherEngine, DEFAULT_SIGNATURE_TIMESTAMP,
};
pub use error::ProviderError;
pub use innertube::{
    build_client_context, create_innertube_http_client, parse_duration, parse_player_response,
    parse_search_response, parse_search_response_typed, resolve_candidate_to_stream,
    select_itag_140_stream, RawStreamCandidate, YouTubeMusicClient, INNERTUBE_BASE_URL,
    SONG_FILTER_PARAMS,
};

/// Trait abstracting music catalog search and audio stream resolution.
#[async_trait]
pub trait MusicProvider: Send + Sync {
    /// Searches tracks by a text query string.
    async fn search(&self, query: &str) -> Result<Vec<Track>, ProviderError>;

    /// Resolves a playable audio stream for a given track.
    async fn resolve_stream(&self, track: &Track) -> Result<AudioStream, ProviderError>;
}

/// Client for Echo Music's official lossless-extension and 320kbs-extension (legacy router support).
pub struct EchoExtensionProvider {
    client: reqwest::Client,
    endpoint_url: String,
}

#[derive(Deserialize)]
struct ExtensionSearchItem {
    id: String,
    title: String,
    artist: String,
    album: Option<String>,
    duration: Option<u32>,
    thumbnail: Option<String>,
}

#[derive(Deserialize)]
struct ExtensionStreamResponse {
    url: String,
    format: Option<String>,
    bitrate: Option<u32>,
    #[serde(rename = "sampleRate")]
    sample_rate: Option<u32>,
    #[serde(rename = "isLossless")]
    is_lossless: Option<bool>,
}

impl EchoExtensionProvider {
    pub fn new(endpoint_url: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
            endpoint_url: endpoint_url.into().trim_end_matches('/').to_string(),
        }
    }
}

#[async_trait]
impl MusicProvider for EchoExtensionProvider {
    async fn search(&self, query: &str) -> Result<Vec<Track>, ProviderError> {
        let url = format!("{}/search", self.endpoint_url);
        let resp = self.client.get(&url).query(&[("q", query)]).send().await?;

        let status = resp.status();
        if !status.is_success() {
            return Err(ProviderError::ApiError {
                code: status.as_u16(),
                message: format!("Status {}", status),
            });
        }

        let items: Vec<ExtensionSearchItem> = resp.json().await?;
        let tracks = items
            .into_iter()
            .map(|item| Track {
                id: item.id,
                title: item.title,
                artist: item.artist,
                album: item.album,
                duration_seconds: item.duration.unwrap_or(0),
                thumbnail_url: item.thumbnail,
            })
            .collect();

        Ok(tracks)
    }

    async fn resolve_stream(&self, track: &Track) -> Result<AudioStream, ProviderError> {
        let url = format!("{}/stream", self.endpoint_url);
        let resp = self
            .client
            .get(&url)
            .query(&[("id", &track.id), ("title", &track.title), ("artist", &track.artist)])
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            return Err(ProviderError::NoStreamsAvailable(track.id.clone()));
        }

        let data: ExtensionStreamResponse = resp.json().await?;
        let codec = match data.format.as_deref() {
            Some("flac") | Some("alac") => AudioCodec::Flac,
            Some("mp3") => AudioCodec::Mp3,
            Some("aac") | Some("m4a") => AudioCodec::Aac,
            Some("opus") => AudioCodec::Opus,
            _ => AudioCodec::Unknown,
        };

        Ok(AudioStream {
            url: data.url,
            codec,
            bitrate_kbps: data.bitrate.unwrap_or(data.is_lossless.map_or(320, |_| 1411)),
            sample_rate_hz: data.sample_rate.unwrap_or(44100),
            is_lossless: data.is_lossless.unwrap_or(codec == AudioCodec::Flac),
        })
    }
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
            debug!("Attempting high-fidelity stream resolution for track {}", track.id);
            if let Ok(stream) = primary.resolve_stream(track).await {
                info!("High-fidelity stream resolved via extension ({} kbps)", stream.bitrate_kbps);
                return Ok(stream);
            }
        }

        debug!("Falling back to standard stream provider for {}", track.id);
        self.fallback.resolve_stream(track).await
    }
}
