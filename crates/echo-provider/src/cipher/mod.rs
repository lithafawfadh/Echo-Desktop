pub mod descrambler;
pub mod n_transform;
pub mod ops;

pub use descrambler::{descramble_signature, parse_player_script, PlayerCipherState, DEFAULT_SIGNATURE_TIMESTAMP};
pub use n_transform::{
    deobfuscate_stream_url, execute_n_ops, execute_n_ops_in_place, transform_n_token,
    transform_stream_url_n, transform_stream_url_n_with_ops, NOp,
};
pub use ops::{execute_cipher_ops, execute_cipher_ops_in_place, CipherOp};

use crate::error::ProviderError;
use std::sync::Arc;
use tokio::sync::RwLock;

/// High-level engine managing player cipher state and stream URL deobfuscation.
#[derive(Debug, Clone)]
pub struct YouTubeCipherEngine {
    state: Arc<RwLock<Option<PlayerCipherState>>>,
}

impl Default for YouTubeCipherEngine {
    fn default() -> Self {
        Self {
            state: Arc::new(RwLock::new(None)),
        }
    }
}

impl YouTubeCipherEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets or updates the cached player cipher state (e.g. after fetching base.js).
    pub async fn set_state(&self, state: PlayerCipherState) {
        let mut lock = self.state.write().await;
        *lock = Some(state);
    }

    /// Retrieves the current cached player cipher state.
    pub async fn get_state(&self) -> Option<PlayerCipherState> {
        self.state.read().await.clone()
    }

    /// Evicts the cached player cipher state (e.g. when CDN returns HTTP 403 Forbidden).
    pub async fn invalidate(&self) {
        let mut lock = self.state.write().await;
        *lock = None;
    }

    /// Descrambles a `signatureCipher` string using the provided operations.
    pub fn descramble_signature(cipher_str: &str, ops: &[CipherOp]) -> Result<String, ProviderError> {
        descramble_signature(cipher_str, ops)
    }

    /// Applies the `n`-parameter throttling transform to a stream URL.
    pub fn transform_n(raw_url: &str) -> String {
        transform_stream_url_n(raw_url)
    }
}
