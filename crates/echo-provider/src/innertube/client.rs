use super::context::{build_client_context, create_innertube_http_client, INNERTUBE_BASE_URL};
use super::models::InnerTubePlayerResponse;
use super::player::{resolve_candidate_to_stream, select_itag_140_stream, RawStreamCandidate};
use super::search::{parse_search_response, SONG_FILTER_PARAMS};
use crate::cipher::{parse_player_script, YouTubeCipherEngine, DEFAULT_SIGNATURE_TIMESTAMP};
use crate::error::ProviderError;
use crate::MusicProvider;
use async_trait::async_trait;
use echo_core::{AudioFidelity, AudioStream, Track};
use regex::Regex;
use serde_json::json;
use tracing::debug;

/// High-level client for interacting with YouTube Music InnerTube endpoints.
pub struct YouTubeMusicClient {
    http_client: reqwest::Client,
    cipher_engine: YouTubeCipherEngine,
    base_url: String,
}

impl Default for YouTubeMusicClient {
    fn default() -> Self {
        Self {
            http_client: create_innertube_http_client().unwrap_or_default(),
            cipher_engine: YouTubeCipherEngine::new(),
            base_url: INNERTUBE_BASE_URL.to_string(),
        }
    }
}

impl YouTubeMusicClient {
    /// Creates a new `YouTubeMusicClient` initialized with default `WEB_REMIX` headers.
    pub fn new() -> Result<Self, ProviderError> {
        let http_client = create_innertube_http_client()?;
        Ok(Self {
            http_client,
            cipher_engine: YouTubeCipherEngine::new(),
            base_url: INNERTUBE_BASE_URL.to_string(),
        })
    }

    /// Creates a `YouTubeMusicClient` with an explicit `YouTubeCipherEngine`.
    pub fn with_cipher_engine(cipher_engine: YouTubeCipherEngine) -> Result<Self, ProviderError> {
        let http_client = create_innertube_http_client()?;
        Ok(Self {
            http_client,
            cipher_engine,
            base_url: INNERTUBE_BASE_URL.to_string(),
        })
    }

    /// Returns a reference to the underlying cipher engine.
    pub fn cipher_engine(&self) -> &YouTubeCipherEngine {
        &self.cipher_engine
    }

    /// Searches YouTube Music for songs matching the given query string.
    pub async fn search_tracks(&self, query: &str) -> Result<Vec<Track>, ProviderError> {
        let url = format!("{}/search", self.base_url);
        let payload = json!({
            "context": build_client_context(),
            "query": query,
            "params": SONG_FILTER_PARAMS,
        });

        debug!(query = %query, "Dispatching InnerTube search request");
        let resp = self.http_client.post(&url).json(&payload).send().await?;

        let status = resp.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(ProviderError::RateLimited);
        } else if !status.is_success() {
            let error_body = resp.text().await.unwrap_or_default();
            return Err(ProviderError::ApiError {
                code: status.as_u16(),
                message: error_body,
            });
        }

        let body_json: serde_json::Value = resp.json().await?;
        parse_search_response(&body_json)
    }

    /// Resolves a direct or cipher-deobfuscated playable audio stream for a given YouTube video ID.
    pub async fn resolve_stream_url(&self, video_id: &str) -> Result<AudioStream, ProviderError> {
        let sts = match self.cipher_engine.get_state().await {
            Some(state) => state.signature_timestamp,
            None => DEFAULT_SIGNATURE_TIMESTAMP,
        };

        let url = format!("{}/player", self.base_url);
        let payload = json!({
            "context": build_client_context(),
            "videoId": video_id,
            "playbackContext": {
                "contentPlaybackContext": {
                    "signatureTimestamp": sts
                }
            }
        });

        debug!(video_id = %video_id, sts = sts, "Dispatching InnerTube player request");
        let resp = self.http_client.post(&url).json(&payload).send().await?;

        let status = resp.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(ProviderError::RateLimited);
        } else if status == reqwest::StatusCode::NOT_FOUND {
            return Err(ProviderError::TrackNotFound(video_id.to_string()));
        } else if !status.is_success() {
            let error_body = resp.text().await.unwrap_or_default();
            return Err(ProviderError::ApiError {
                code: status.as_u16(),
                message: error_body,
            });
        }

        let player_resp: InnerTubePlayerResponse = resp.json().await?;
        let candidate = select_itag_140_stream(&player_resp, video_id)?;

        let mut cipher_state = self.cipher_engine.get_state().await;
        if cipher_state.is_none() && matches!(candidate, RawStreamCandidate::Cipher { .. }) {
            debug!("Cipher operations missing on cold client; lazily loading base.js");
            self.refresh_player_cipher().await?;
            cipher_state = self.cipher_engine.get_state().await;
        }

        let cipher_ops = cipher_state.as_ref().map(|s| s.cipher_ops.as_slice());

        resolve_candidate_to_stream(candidate, cipher_ops)
    }

    /// Refreshes the active YouTube player JavaScript bundle (`base.js`) and parses cipher operations.
    pub async fn refresh_player_cipher(&self) -> Result<(), ProviderError> {
        let landing_resp = self.http_client.get("https://music.youtube.com/").send().await?;
        let html = landing_resp.text().await?;

        // Locate base.js player script URL
        let script_re = Regex::new(r#""jsUrl"\s*:\s*"([^"]+base\.js)""#)
            .map_err(|e| ProviderError::CipherError(format!("Invalid base.js regex: {}", e)))?;

        let script_path = script_re
            .captures(&html)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str())
            .ok_or_else(|| {
                ProviderError::CipherError("Could not locate base.js script URL in landing page".into())
            })?;

        let script_url = if script_path.starts_with("http") {
            script_path.to_string()
        } else {
            format!("https://music.youtube.com{script_path}")
        };

        debug!(script_url = %script_url, "Fetching active base.js player bundle");
        let script_resp = self.http_client.get(&script_url).send().await?;
        let script_content = script_resp.text().await?;

        let state = parse_player_script(&script_content)?;
        self.cipher_engine.set_state(state).await;
        Ok(())
    }
}

#[async_trait]
impl MusicProvider for YouTubeMusicClient {
    fn name(&self) -> &'static str {
        "YouTubeMusicInnerTube"
    }

    fn supported_fidelity(&self) -> AudioFidelity {
        AudioFidelity::LossyHigh
    }

    async fn search(&self, query: &str) -> Result<Vec<Track>, ProviderError> {
        self.search_tracks(query).await
    }

    async fn resolve_stream(&self, track: &Track) -> Result<AudioStream, ProviderError> {
        self.resolve_stream_url(&track.id).await
    }
}
