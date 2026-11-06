use thiserror::Error;

/// Comprehensive error taxonomy for Echo Music provider operations.
#[derive(Error, Debug)]
pub enum ProviderError {
    #[error("Network I/O error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Failed to parse JSON response: {0}")]
    JsonParse(#[from] serde_json::Error),

    #[error("Cipher deobfuscation error: {0}")]
    CipherError(String),

    #[error("Track not found: {0}")]
    TrackNotFound(String),

    #[error("No compatible audio streams available for track: {0}")]
    NoStreamsAvailable(String),

    #[error("No stream available for track: {0}")]
    NoStreamAvailable(String),

    #[error("InnerTube API error (HTTP {code}): {message}")]
    ApiError { code: u16, message: String },

    #[error("InnerTube API error: {0}")]
    ApiErrorMsg(String),

    #[error("Rate limited by upstream provider (HTTP 429)")]
    RateLimited,

    #[error("Playback restricted ({status}): {reason}")]
    PlayabilityError { status: String, reason: String },

    #[error("URL parsing error: {0}")]
    UrlParse(#[from] url::ParseError),
}
