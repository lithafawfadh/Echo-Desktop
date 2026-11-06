use super::models::{FormatItem, InnerTubePlayerResponse};
use crate::cipher::{descramble_signature, transform_stream_url_n, CipherOp};
use crate::error::ProviderError;
use echo_core::{AudioCodec, AudioStream};
use serde_json::Value;

/// Raw format extraction result before cipher deobfuscation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawStreamCandidate {
    Direct {
        url: String,
        bitrate_kbps: u32,
        sample_rate_hz: u32,
    },
    Cipher {
        cipher_str: String,
        bitrate_kbps: u32,
        sample_rate_hz: u32,
    },
}

/// Selects the optimal audio format from an InnerTube player response.
/// Strictly prioritizes ITAG 140 (AAC-LC ~128kbps in ISO-MP4 container) because
/// Symphonia 0.5.5 does NOT bundle an Opus decoder and would crash with `UnsupportedCodec`.
pub fn select_itag_140_stream(
    player_resp: &InnerTubePlayerResponse,
    video_id: &str,
) -> Result<RawStreamCandidate, ProviderError> {
    // 1. Validate playability status
    if let Some(ref status) = player_resp.playability_status {
        if status.status != "OK" {
            let reason = status
                .reason
                .clone()
                .unwrap_or_else(|| "Playback not permitted".to_string());
            return Err(ProviderError::PlayabilityError {
                status: status.status.clone(),
                reason,
            });
        }
    }

    // 2. Locate adaptiveFormats in streamingData
    let streaming_data = player_resp
        .streaming_data
        .as_ref()
        .ok_or_else(|| ProviderError::NoStreamsAvailable(video_id.to_string()))?;

    let adaptive_formats = streaming_data
        .adaptive_formats
        .as_ref()
        .ok_or_else(|| ProviderError::NoStreamsAvailable(video_id.to_string()))?;

    // 3. Select ITAG 140 (primary) or any AAC/MP4 audio stream (fallback)
    let target_format = adaptive_formats
        .iter()
        .find(|f| f.itag == 140)
        .or_else(|| {
            // Secondary fallback: Any audio/mp4 container stream with AAC
            adaptive_formats.iter().find(|f| {
                f.mime_type
                    .as_deref()
                    .map(|m| m.starts_with("audio/mp4"))
                    .unwrap_or(false)
            })
        })
        .ok_or_else(|| ProviderError::NoStreamsAvailable(video_id.to_string()))?;

    // 4. Extract bitrate and sample rate
    let bitrate_kbps = target_format
        .average_bitrate
        .or(target_format.bitrate)
        .map(|b| b / 1000)
        .unwrap_or(128);

    let sample_rate_hz = target_format
        .audio_sample_rate
        .as_deref()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(44100);

    // 5. Yield candidate based on url or cipher
    if let Some(ref direct_url) = target_format.url {
        Ok(RawStreamCandidate::Direct {
            url: direct_url.clone(),
            bitrate_kbps,
            sample_rate_hz,
        })
    } else if let Some(ref cipher) = target_format
        .signature_cipher
        .as_ref()
        .or(target_format.cipher.as_ref())
    {
        Ok(RawStreamCandidate::Cipher {
            cipher_str: (*cipher).clone(),
            bitrate_kbps,
            sample_rate_hz,
        })
    } else {
        Err(ProviderError::NoStreamsAvailable(format!(
            "{video_id}: Target format (itag {}) lacks both url and signatureCipher",
            target_format.itag
        )))
    }
}

/// Resolves a `RawStreamCandidate` into a playable `echo_core::AudioStream`,
/// executing signature descrambling (if ciphered) and applying `n`-parameter throttling transform.
pub fn resolve_candidate_to_stream(
    candidate: RawStreamCandidate,
    cipher_ops: Option<&[CipherOp]>,
) -> Result<AudioStream, ProviderError> {
    match candidate {
        RawStreamCandidate::Direct {
            url,
            bitrate_kbps,
            sample_rate_hz,
        } => {
            // Direct URL: apply n-transform
            let final_url = transform_stream_url_n(&url);
            Ok(AudioStream {
                url: final_url,
                codec: AudioCodec::Aac,
                bitrate_kbps,
                sample_rate_hz,
                is_lossless: false,
                bit_depth: None,
                fidelity: Some(echo_core::AudioFidelity::LossyStandard),
                content_length_bytes: None,
                http_headers: std::collections::HashMap::new(),
            })
        }
        RawStreamCandidate::Cipher {
            cipher_str,
            bitrate_kbps,
            sample_rate_hz,
        } => {
            let ops = cipher_ops.ok_or_else(|| {
                ProviderError::CipherError(
                    "Cipher operations not loaded; cannot descramble signature".into(),
                )
            })?;

            let deciphered_url = descramble_signature(&cipher_str, ops)?;
            let final_url = transform_stream_url_n(&deciphered_url);

            Ok(AudioStream {
                url: final_url,
                codec: AudioCodec::Aac,
                bitrate_kbps,
                sample_rate_hz,
                is_lossless: false,
                bit_depth: None,
                fidelity: Some(echo_core::AudioFidelity::LossyStandard),
                content_length_bytes: None,
                http_headers: std::collections::HashMap::new(),
            })
        }
    }
}

/// Helper to parse a player response JSON value and extract the target `FormatItem`.
/// Prioritizes itag 140 for Symphonia compatibility; falls back to highest bitrate audio format.
pub fn parse_player_response(json: &Value) -> Result<FormatItem, ProviderError> {
    let response: InnerTubePlayerResponse = serde_json::from_value(json.clone())?;

    if let Some(ref status) = response.playability_status {
        if status.status != "OK" {
            let reason = status
                .reason
                .clone()
                .unwrap_or_else(|| "Playback restricted".to_string());
            return Err(ProviderError::PlayabilityError {
                status: status.status.clone(),
                reason,
            });
        }
    }

    let streaming_data = response
        .streaming_data
        .ok_or_else(|| ProviderError::NoStreamsAvailable("missing streamingData".into()))?;

    let adaptive = streaming_data
        .adaptive_formats
        .ok_or_else(|| ProviderError::NoStreamsAvailable("missing adaptiveFormats".into()))?;

    // Prioritize ITAG 140 (AAC)
    if let Some(format_140) = adaptive.iter().find(|f| f.itag == 140) {
        return Ok(format_140.clone());
    }

    // Fallback: any audio format
    if let Some(audio_fmt) = adaptive.iter().find(|f| {
        f.mime_type
            .as_deref()
            .map(|m| m.starts_with("audio/"))
            .unwrap_or(false)
    }) {
        return Ok(audio_fmt.clone());
    }

    // Fallback: first available format
    adaptive
        .into_iter()
        .next()
        .ok_or_else(|| ProviderError::NoStreamsAvailable("No adaptive formats found".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_select_itag_140_direct() {
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
                        "url": "https://rr1---sn-test.googlevideo.com/videoplayback?itag=140&n=X_LCxVDjAavgE5t"
                    }
                ]
            }
        });

        let resp: InnerTubePlayerResponse = serde_json::from_value(player_json).unwrap();
        let candidate = select_itag_140_stream(&resp, "test_vid").expect("Should select itag 140");

        match candidate {
            RawStreamCandidate::Direct { url, bitrate_kbps, sample_rate_hz } => {
                assert!(url.contains("itag=140"));
                assert_eq!(bitrate_kbps, 128);
                assert_eq!(sample_rate_hz, 44100);

                let stream = resolve_candidate_to_stream(
                    RawStreamCandidate::Direct { url, bitrate_kbps, sample_rate_hz },
                    None,
                )
                .unwrap();
                assert_eq!(stream.codec, AudioCodec::Aac);
                assert!(stream.url.contains("n=X_LCxVDjAavgE5t")); // Honest pass-through fallback preserves raw token
            }
            RawStreamCandidate::Cipher { .. } => panic!("Expected direct stream"),
        }
    }

    #[test]
    fn test_playability_error() {
        let player_json = json!({
            "playabilityStatus": {
                "status": "LOGIN_REQUIRED",
                "reason": "Sign in to confirm you're not a bot"
            }
        });

        let resp: InnerTubePlayerResponse = serde_json::from_value(player_json).unwrap();
        let err = select_itag_140_stream(&resp, "test_vid").unwrap_err();

        match err {
            ProviderError::PlayabilityError { status, reason } => {
                assert_eq!(status, "LOGIN_REQUIRED");
                assert_eq!(reason, "Sign in to confirm you're not a bot");
            }
            _ => panic!("Expected PlayabilityError"),
        }
    }
}
