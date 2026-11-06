use thiserror::Error;

/// Comprehensive error taxonomy for Echo Music provider operations.
#[derive(Error, Debug)]
pub enum ProviderError {
    #[error("Network I/O error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Connection refused: {0}")]
    ConnectionRefused(String),

    #[error("Connection timeout: {0}")]
    Timeout(String),

    #[error("Failed to parse JSON response: {0}")]
    JsonParse(#[from] serde_json::Error),

    #[error("Cipher deobfuscation error: {0}")]
    CipherError(String),

    #[error("Track not found: {0}")]
    TrackNotFound(String),

    #[error("Invalid parameters: {0}")]
    InvalidParameters(String),

    #[error("Requested quality unavailable: {0}")]
    QualityUnavailable(String),

    #[error("Upstream service failed: {0}")]
    UpstreamFailed(String),

    #[error("Extension error [{code}]: {message} (upstream: {upstream_status:?}, retryable: {retryable})")]
    ExtensionError {
        code: String,
        message: String,
        upstream_status: Option<u16>,
        retryable: bool,
    },

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

impl ProviderError {
    pub fn is_not_found(&self) -> bool {
        matches!(self, ProviderError::TrackNotFound(_))
    }

    pub fn is_quality_unavailable(&self) -> bool {
        matches!(self, ProviderError::QualityUnavailable(_))
    }

    pub fn is_connection_failure(&self) -> bool {
        matches!(
            self,
            ProviderError::ConnectionRefused(_)
                | ProviderError::Timeout(_)
                | ProviderError::UpstreamFailed(_)
        ) || match self {
            ProviderError::Network(err) => err.is_connect() || err.is_timeout(),
            _ => false,
        }
    }

    pub fn should_fallback(&self) -> bool {
        self.is_not_found()
            || self.is_quality_unavailable()
            || self.is_connection_failure()
            || matches!(
                self,
                ProviderError::NoStreamsAvailable(_)
                    | ProviderError::NoStreamAvailable(_)
                    | ProviderError::RateLimited
            )
    }
}
