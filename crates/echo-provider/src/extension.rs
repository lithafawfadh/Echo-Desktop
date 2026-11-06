use async_trait::async_trait;
use echo_core::{AudioCodec, AudioFidelity, AudioStream, Track};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tracing::debug;

use crate::error::ProviderError;
use crate::MusicProvider;

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ExtensionCapabilities {
    #[serde(default)]
    pub formats: Vec<String>,
    #[serde(rename = "maxBitDepth", default)]
    pub max_bit_depth: Option<u8>,
    #[serde(rename = "maxSampleRate", default)]
    pub max_sample_rate: Option<u32>,
    #[serde(rename = "supportsIsrcLookup", default)]
    pub supports_isrc_lookup: bool,
    #[serde(rename = "supportsDirectStreaming", default)]
    pub supports_direct_streaming: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionInfo {
    pub name: String,
    pub version: String,
    pub tier: String,
    pub capabilities: ExtensionCapabilities,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionHealthResponse {
    pub status: String,
    #[serde(default)]
    pub extension: Option<ExtensionInfo>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub tier: Option<String>,
    #[serde(default)]
    pub capabilities: Option<ExtensionCapabilities>,
}

impl ExtensionHealthResponse {
    pub fn into_health(self) -> ExtensionHealth {
        if let Some(info) = self.extension {
            ExtensionHealth {
                status: self.status,
                name: info.name,
                version: info.version,
                tier: info.tier,
                capabilities: info.capabilities,
            }
        } else {
            ExtensionHealth {
                status: self.status,
                name: self.name.unwrap_or_default(),
                version: self.version.unwrap_or_default(),
                tier: self.tier.unwrap_or_default(),
                capabilities: self.capabilities.unwrap_or_default(),
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionHealth {
    pub status: String,
    pub name: String,
    pub version: String,
    pub tier: String,
    pub capabilities: ExtensionCapabilities,
}

impl ExtensionHealth {
    pub fn is_healthy(&self) -> bool {
        self.status.eq_ignore_ascii_case("healthy") || self.status.eq_ignore_ascii_case("ok")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionQuality {
    pub format: String,
    #[serde(rename = "bitDepth", default)]
    pub bit_depth: Option<u8>,
    #[serde(rename = "sampleRate", default)]
    pub sample_rate: Option<u32>,
    #[serde(default)]
    pub bitrate: Option<u32>,
    #[serde(rename = "isLossless", default)]
    pub is_lossless: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionSearchResult {
    pub id: String,
    #[serde(default)]
    pub isrc: Option<String>,
    pub title: String,
    pub artist: String,
    #[serde(default)]
    pub album: Option<String>,
    #[serde(default)]
    pub duration: Option<u32>,
    #[serde(rename = "releaseYear", default)]
    pub release_year: Option<u32>,
    #[serde(default)]
    pub thumbnail: Option<String>,
    #[serde(rename = "availableQualities", default)]
    pub available_qualities: Vec<ExtensionQuality>,
}

impl From<ExtensionSearchResult> for Track {
    fn from(res: ExtensionSearchResult) -> Self {
        Track {
            id: res.id,
            title: res.title,
            artist: res.artist,
            album: res.album,
            duration_seconds: res.duration.unwrap_or(0),
            thumbnail_url: res.thumbnail,
            isrc: res.isrc,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionSearchResponse {
    #[serde(default)]
    pub results: Vec<ExtensionSearchResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionStreamResponse {
    pub url: String,
    #[serde(default)]
    pub format: Option<String>,
    #[serde(rename = "bitDepth", default)]
    pub bit_depth: Option<u8>,
    #[serde(rename = "sampleRate", default)]
    pub sample_rate: Option<u32>,
    #[serde(default)]
    pub bitrate: Option<u32>,
    #[serde(rename = "isLossless", default)]
    pub is_lossless: Option<bool>,
    #[serde(rename = "fileSizeBytes", default)]
    pub file_size_bytes: Option<u64>,
    #[serde(rename = "durationSeconds", default)]
    pub duration_seconds: Option<u32>,
    #[serde(rename = "expiresAt", default)]
    pub expires_at: Option<u64>,
    #[serde(default)]
    pub headers: Option<HashMap<String, String>>,
}

impl ExtensionStreamResponse {
    pub fn into_audio_stream(self) -> AudioStream {
        let codec = match self.format.as_deref().map(|s| s.to_ascii_lowercase()).as_deref() {
            Some("flac") => AudioCodec::Flac,
            Some("alac") => AudioCodec::Alac,
            Some("wav") => AudioCodec::Wav,
            Some("mp3") => AudioCodec::Mp3,
            Some("aac") | Some("m4a") => AudioCodec::Aac,
            Some("opus") | Some("webm") => AudioCodec::Opus,
            _ => AudioCodec::Unknown,
        };

        let is_lossless = self.is_lossless.unwrap_or(matches!(
            codec,
            AudioCodec::Flac | AudioCodec::Alac | AudioCodec::Wav
        ));

        let sample_rate_hz = self.sample_rate.unwrap_or(44100);
        let bit_depth = self.bit_depth.or(if is_lossless { Some(16) } else { None });
        let bitrate_kbps = self.bitrate.unwrap_or(if is_lossless { 1411 } else { 320 });

        let fidelity = if is_lossless {
            if bit_depth.unwrap_or(16) > 16 || sample_rate_hz > 48000 {
                AudioFidelity::LosslessHiRes
            } else {
                AudioFidelity::LosslessCD
            }
        } else if bitrate_kbps >= 320 {
            AudioFidelity::Lossy320
        } else if bitrate_kbps >= 160 {
            AudioFidelity::LossyHigh
        } else {
            AudioFidelity::LossyStandard
        };

        AudioStream {
            url: self.url,
            codec,
            bitrate_kbps,
            sample_rate_hz,
            is_lossless,
            bit_depth,
            fidelity: Some(fidelity),
            content_length_bytes: self.file_size_bytes,
            http_headers: self.headers.unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionErrorDetail {
    pub code: String,
    pub message: String,
    #[serde(rename = "upstreamStatus", default)]
    pub upstream_status: Option<u16>,
    #[serde(default)]
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionErrorResponse {
    pub error: ExtensionErrorDetail,
}

#[derive(Clone)]
pub struct EchoExtensionProvider {
    client: reqwest::Client,
    base_url: String,
}

impl EchoExtensionProvider {
    pub fn new(base_url: impl Into<String>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .connect_timeout(Duration::from_secs(3))
            .build()
            .unwrap_or_default();
        Self::with_client(client, base_url)
    }

    pub fn with_client(client: reqwest::Client, base_url: impl Into<String>) -> Self {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        Self { client, base_url }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub async fn check_health(&self) -> Result<ExtensionHealth, ProviderError> {
        let url = format!("{}/health", self.base_url);
        debug!(url = %url, "Checking extension health");
        let resp = self.client.get(&url).send().await.map_err(Self::map_reqwest_error)?;

        if !resp.status().is_success() {
            return Err(Self::parse_http_error(resp).await);
        }

        let text = resp.text().await.map_err(Self::map_reqwest_error)?;
        let body: ExtensionHealthResponse = serde_json::from_str(&text).map_err(ProviderError::JsonParse)?;
        Ok(body.into_health())
    }

    pub async fn search(&self, query: &str) -> Result<Vec<Track>, ProviderError> {
        let url = format!("{}/search", self.base_url);
        debug!(url = %url, query = %query, "Searching extension catalog");
        let resp = self
            .client
            .get(&url)
            .query(&[("q", query), ("type", "track"), ("limit", "10")])
            .send()
            .await
            .map_err(Self::map_reqwest_error)?;

        if !resp.status().is_success() {
            return Err(Self::parse_http_error(resp).await);
        }

        let text = resp.text().await.map_err(Self::map_reqwest_error)?;
        let body: ExtensionSearchResponse = serde_json::from_str(&text).map_err(ProviderError::JsonParse)?;
        let tracks = body.results.into_iter().map(Track::from).collect();
        Ok(tracks)
    }

    pub async fn resolve_isrc(&self, isrc: &str) -> Result<AudioStream, ProviderError> {
        let url = format!("{}/resolve/isrc/{}", self.base_url, isrc);
        debug!(url = %url, isrc = %isrc, "Resolving stream via ISRC direct lookup");
        let resp = self.client.get(&url).send().await.map_err(Self::map_reqwest_error)?;

        if !resp.status().is_success() {
            return Err(Self::parse_http_error(resp).await);
        }

        let text = resp.text().await.map_err(Self::map_reqwest_error)?;
        let body: ExtensionStreamResponse = serde_json::from_str(&text).map_err(ProviderError::JsonParse)?;
        Ok(body.into_audio_stream())
    }

    pub async fn resolve_stream_with_quality(
        &self,
        track: &Track,
        quality: &str,
    ) -> Result<AudioStream, ProviderError> {
        let url = format!("{}/stream", self.base_url);
        debug!(url = %url, track_id = %track.id, isrc = ?track.isrc, quality = %quality, "Resolving stream from extension");

        let mut req = self.client.get(&url).query(&[("id", &track.id)]);

        if let Some(ref isrc) = track.isrc {
            req = req.query(&[("isrc", isrc)]);
        }
        if !track.title.is_empty() {
            req = req.query(&[("title", &track.title)]);
        }
        if !track.artist.is_empty() {
            req = req.query(&[("artist", &track.artist)]);
        }
        req = req.query(&[("quality", quality)]);

        let resp = req.send().await.map_err(Self::map_reqwest_error)?;

        if !resp.status().is_success() {
            return Err(Self::parse_http_error(resp).await);
        }

        let text = resp.text().await.map_err(Self::map_reqwest_error)?;
        let body: ExtensionStreamResponse = serde_json::from_str(&text).map_err(ProviderError::JsonParse)?;
        Ok(body.into_audio_stream())
    }

    pub async fn resolve_stream(&self, track: &Track) -> Result<AudioStream, ProviderError> {
        self.resolve_stream_with_quality(track, "lossless_max").await
    }

    fn map_reqwest_error(err: reqwest::Error) -> ProviderError {
        if err.is_timeout() {
            ProviderError::Timeout(err.to_string())
        } else if err.is_connect() {
            ProviderError::ConnectionRefused(err.to_string())
        } else {
            ProviderError::Network(err)
        }
    }

    async fn parse_http_error(resp: reqwest::Response) -> ProviderError {
        let status = resp.status();
        let status_code = status.as_u16();

        let text = resp.text().await.unwrap_or_default();
        if let Ok(err_resp) = serde_json::from_str::<ExtensionErrorResponse>(&text) {
            let detail = err_resp.error;
            match detail.code.as_str() {
                "TRACK_NOT_FOUND" => ProviderError::TrackNotFound(detail.message),
                "INVALID_PARAMETERS" => ProviderError::InvalidParameters(detail.message),
                "QUALITY_UNAVAILABLE" => ProviderError::QualityUnavailable(detail.message),
                "RATE_LIMITED" => ProviderError::RateLimited,
                "UPSTREAM_FAILED" => ProviderError::UpstreamFailed(detail.message),
                _ => {
                    if status_code == 404 {
                        ProviderError::TrackNotFound(detail.message)
                    } else if status_code == 400 {
                        ProviderError::InvalidParameters(detail.message)
                    } else if status_code == 416 {
                        ProviderError::QualityUnavailable(detail.message)
                    } else if status_code == 429 {
                        ProviderError::RateLimited
                    } else if status_code == 502 {
                        ProviderError::UpstreamFailed(detail.message)
                    } else {
                        ProviderError::ExtensionError {
                            code: detail.code,
                            message: detail.message,
                            upstream_status: detail.upstream_status,
                            retryable: detail.retryable,
                        }
                    }
                }
            }
        } else {
            match status_code {
                404 => ProviderError::TrackNotFound("Track not found in extension catalog".into()),
                400 => ProviderError::InvalidParameters("Invalid extension request parameters".into()),
                416 => ProviderError::QualityUnavailable("Requested quality tier unavailable".into()),
                429 => ProviderError::RateLimited,
                502 => ProviderError::UpstreamFailed("Extension upstream gateway failure (HTTP 502)".into()),
                code => ProviderError::ApiError {
                    code,
                    message: format!("Extension HTTP error {}", status),
                },
            }
        }
    }
}

#[async_trait]
impl MusicProvider for EchoExtensionProvider {
    fn name(&self) -> &'static str {
        "EchoExtensionProvider"
    }

    fn supported_fidelity(&self) -> AudioFidelity {
        AudioFidelity::LosslessHiRes
    }

    async fn search(&self, query: &str) -> Result<Vec<Track>, ProviderError> {
        self.search(query).await
    }

    async fn resolve_stream(&self, track: &Track) -> Result<AudioStream, ProviderError> {
        self.resolve_stream(track).await
    }
}
