//! Empirical Stress Tests for YouTubeMusicClient lifecycle and format selection.

use echo_core::AudioCodec;
use echo_provider::cipher::{parse_player_script, YouTubeCipherEngine};
use echo_provider::error::ProviderError;
use echo_provider::innertube::{
    resolve_candidate_to_stream, select_itag_140_stream, InnerTubePlayerResponse,
    RawStreamCandidate, YouTubeMusicClient,
};
use serde_json::json;

const SAMPLE_BASE_JS: &str = include_str!("fixtures/sample_base.js");

#[tokio::test]
async fn test_cold_client_initial_state() {
    let client = YouTubeMusicClient::new().expect("Failed to instantiate YouTubeMusicClient");
    assert!(
        client.cipher_engine().get_state().await.is_none(),
        "Cold YouTubeMusicClient must start with None cipher_engine state"
    );
}

#[tokio::test]
async fn test_client_with_preloaded_cipher_engine() {
    let engine = YouTubeCipherEngine::new();
    let state = parse_player_script(SAMPLE_BASE_JS).expect("Should parse sample base.js");
    engine.set_state(state.clone()).await;

    let client = YouTubeMusicClient::with_cipher_engine(engine)
        .expect("Failed to instantiate YouTubeMusicClient with engine");
    let loaded = client.cipher_engine().get_state().await;
    assert_eq!(
        loaded,
        Some(state),
        "Client with explicit engine must preserve initialized state"
    );
}

#[test]
fn test_itag_140_strict_priority_over_itag_251_opus() {
    // Both itag 251 (Opus ~160kbps) and itag 140 (AAC ~128kbps) present.
    // Must select itag 140 to protect Symphonia decoder.
    let player_json = json!({
        "playabilityStatus": { "status": "OK" },
        "streamingData": {
            "adaptiveFormats": [
                {
                    "itag": 251,
                    "mimeType": "audio/webm; codecs=\"opus\"",
                    "bitrate": 160000,
                    "averageBitrate": 155000,
                    "audioSampleRate": "48000",
                    "url": "https://rr1---sn-test.googlevideo.com/videoplayback?itag=251&n=test"
                },
                {
                    "itag": 140,
                    "mimeType": "audio/mp4; codecs=\"mp4a.40.2\"",
                    "bitrate": 128000,
                    "averageBitrate": 127000,
                    "audioSampleRate": "44100",
                    "url": "https://rr1---sn-test.googlevideo.com/videoplayback?itag=140&n=X_LCxVDjAavgE5t"
                }
            ]
        }
    });

    let resp: InnerTubePlayerResponse = serde_json::from_value(player_json).unwrap();
    let candidate = select_itag_140_stream(&resp, "test_itag_priority").unwrap();

    match candidate {
        RawStreamCandidate::Direct {
            url,
            bitrate_kbps,
            sample_rate_hz,
        } => {
            assert!(url.contains("itag=140"), "Selected URL must be itag 140");
            assert!(!url.contains("itag=251"), "Selected URL must NOT be itag 251");
            assert_eq!(bitrate_kbps, 127, "Bitrate must match itag 140 (127 kbps)");
            assert_eq!(sample_rate_hz, 44100, "Sample rate must match itag 140 (44100 Hz)");

            let stream = resolve_candidate_to_stream(
                RawStreamCandidate::Direct {
                    url,
                    bitrate_kbps,
                    sample_rate_hz,
                },
                None,
            )
            .unwrap();
            assert_eq!(stream.codec, AudioCodec::Aac, "Codec must be AAC for Symphonia");
            assert_eq!(stream.bitrate_kbps, 127);
            assert_eq!(stream.sample_rate_hz, 44100);
            assert!(!stream.is_lossless);
        }
        RawStreamCandidate::Cipher { .. } => panic!("Expected direct stream candidate"),
    }
}

#[test]
fn test_itag_140_cipher_format_selected_over_itag_251_direct() {
    // Even if itag 251 is Direct (unencrypted) and itag 140 is Cipher (encrypted),
    // itag 140 MUST still be selected because Symphonia cannot decode Opus.
    let player_json = json!({
        "playabilityStatus": { "status": "OK" },
        "streamingData": {
            "adaptiveFormats": [
                {
                    "itag": 251,
                    "mimeType": "audio/webm; codecs=\"opus\"",
                    "bitrate": 160000,
                    "url": "https://rr1---sn-test.googlevideo.com/videoplayback?itag=251"
                },
                {
                    "itag": 140,
                    "mimeType": "audio/mp4; codecs=\"mp4a.40.2\"",
                    "bitrate": 128000,
                    "audioSampleRate": "44100",
                    "signatureCipher": "s=abcdef1234567890&sp=sig&url=https%3A%2F%2Frr1---sn-test.googlevideo.com%2Fvideoplayback%3Fitag%3D140%26n%3DX_LCxVDjAavgE5t"
                }
            ]
        }
    });

    let resp: InnerTubePlayerResponse = serde_json::from_value(player_json).unwrap();
    let candidate = select_itag_140_stream(&resp, "test_cipher_priority").unwrap();

    match candidate {
        RawStreamCandidate::Cipher {
            cipher_str,
            bitrate_kbps,
            sample_rate_hz,
        } => {
            assert!(cipher_str.contains("itag%3D140"));
            assert_eq!(bitrate_kbps, 128);
            assert_eq!(sample_rate_hz, 44100);

            // Cold state (no ops) returns CipherError
            let cold_err = resolve_candidate_to_stream(
                RawStreamCandidate::Cipher {
                    cipher_str: cipher_str.clone(),
                    bitrate_kbps,
                    sample_rate_hz,
                },
                None,
            )
            .unwrap_err();

            match cold_err {
                ProviderError::CipherError(msg) => {
                    assert!(msg.contains("Cipher operations not loaded"));
                }
                other => panic!("Expected CipherError when ops is None, got {:?}", other),
            }

            // Warm state (with ops from base.js) resolves successfully
            let state = parse_player_script(SAMPLE_BASE_JS).unwrap();
            let stream = resolve_candidate_to_stream(
                RawStreamCandidate::Cipher {
                    cipher_str,
                    bitrate_kbps,
                    sample_rate_hz,
                },
                Some(&state.cipher_ops),
            )
            .unwrap();

            assert_eq!(stream.codec, AudioCodec::Aac);
            assert!(stream.url.contains("&sig="));
            assert!(stream.url.contains("itag=140"));
        }
        RawStreamCandidate::Direct { .. } => panic!("Expected cipher candidate for itag 140"),
    }
}

#[test]
fn test_only_opus_itag_251_rejected_without_symphonia_panic() {
    let player_json = json!({
        "playabilityStatus": { "status": "OK" },
        "streamingData": {
            "adaptiveFormats": [
                {
                    "itag": 251,
                    "mimeType": "audio/webm; codecs=\"opus\"",
                    "bitrate": 160000,
                    "url": "https://rr1---sn-test.googlevideo.com/videoplayback?itag=251"
                }
            ]
        }
    });

    let resp: InnerTubePlayerResponse = serde_json::from_value(player_json).unwrap();
    let err = select_itag_140_stream(&resp, "opus_only_track").unwrap_err();

    match err {
        ProviderError::NoStreamsAvailable(id) => {
            assert_eq!(id, "opus_only_track");
        }
        other => panic!("Expected NoStreamsAvailable when only Opus is present, got {:?}", other),
    }
}

#[tokio::test]
async fn test_cold_client_resolve_stream_url_live_probe() {
    let client = YouTubeMusicClient::new().expect("Client instantiation failed");
    assert!(client.cipher_engine().get_state().await.is_none());

    // Probe live stream resolution on a standard public video ID ("5NV6Rdv1a3I" - Daft Punk Get Lucky)
    // Guard with a short timeout so offline/throttled environments do not block test execution.
    let probe_future = client.resolve_stream_url("5NV6Rdv1a3I");
    match tokio::time::timeout(std::time::Duration::from_secs(2), probe_future).await {
        Ok(Ok(stream)) => {
            assert_eq!(stream.codec, AudioCodec::Aac);
            assert!(!stream.url.is_empty());
        }
        Ok(Err(e)) => {
            match e {
                ProviderError::PlayabilityError { .. }
                | ProviderError::NoStreamsAvailable(_)
                | ProviderError::RateLimited
                | ProviderError::Network(_) => {
                    // Valid typed responses from YouTube
                }
                ProviderError::CipherError(msg) => {
                    // Confirm that lazy loading prevented the cold-client "Cipher operations not loaded" failure
                    assert!(
                        !msg.contains("Cipher operations not loaded"),
                        "Lazy cipher loading failed: returned 'Cipher operations not loaded'"
                    );
                }
                other => panic!("Unexpected error variant from resolve_stream_url: {:?}", other),
            }
        }
        Err(_timeout) => {
            // Network probe timed out (acceptable in offline or rate-limited environments)
        }
    }
}

#[tokio::test]
async fn test_client_refresh_player_cipher_probe() {
    let client = YouTubeMusicClient::new().expect("Client instantiation failed");
    assert!(client.cipher_engine().get_state().await.is_none());

    let refresh_future = client.refresh_player_cipher();
    match tokio::time::timeout(std::time::Duration::from_secs(2), refresh_future).await {
        Ok(Ok(())) => {
            let state = client.cipher_engine().get_state().await;
            assert!(state.is_some(), "State should be populated after refresh");
            let state = state.unwrap();
            assert!(!state.cipher_ops.is_empty(), "Live base.js should yield cipher ops");
        }
        Ok(Err(e)) => {
            match e {
                ProviderError::Network(_) | ProviderError::CipherError(_) => {}
                other => panic!("Unexpected error from refresh_player_cipher: {:?}", other),
            }
        }
        Err(_timeout) => {
            // Network probe timed out (acceptable in offline or rate-limited environments)
        }
    }
}
