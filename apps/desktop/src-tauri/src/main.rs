#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod state;

use state::AppState;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,echo_desktop=debug".into()),
        )
        .init();

    tauri::Builder::default()
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::search_tracks,
            commands::resolve_stream,
            commands::play_stream,
            commands::pause_stream,
            commands::get_playback_state,
            commands::get_lyrics,
            commands::fetch_lyrics,
            commands::get_album_thumb,
            commands::seek_to,
            commands::toggle_playback,
            commands::previous_track,
            commands::next_track,
            commands::get_router_health,
            commands::minimize_window,
            commands::toggle_maximize_window,
            commands::close_window,
            commands::play_direct_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Echo Desktop application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use async_trait::async_trait;
    use echo_core::{AudioCodec, AudioFidelity, AudioStream, PlaybackState, Track};
    use echo_provider::{DynamicFallbackRouter, MusicProvider, ProviderError, RouterHealthReport};

    struct MockInnerProvider {
        handler: Arc<dyn Fn(&Track) -> Result<AudioStream, ProviderError> + Send + Sync>,
    }

    #[async_trait]
    impl MusicProvider for MockInnerProvider {
        fn name(&self) -> &'static str {
            "MockInnerProvider"
        }

        async fn search(&self, _query: &str) -> Result<Vec<Track>, ProviderError> {
            Ok(vec![])
        }

        async fn resolve_stream(&self, track: &Track) -> Result<AudioStream, ProviderError> {
            (self.handler)(track)
        }
    }

    #[tokio::test]
    async fn test_app_state_initialization() {
        let state = AppState::new();
        let playback_state = *state.playback_state.read().await;
        assert_eq!(playback_state, PlaybackState::Stopped);
        assert!(!state.audio_controller.is_playing());
        assert_eq!(state.audio_controller.position_ms(), 0);
        assert_eq!(state.router.name(), "EchoDynamicFallbackRouter");
        assert!(state.router.tier1_lossless().is_some());
        assert!(state.router.tier2_highbitrate().is_some());
    }

    #[tokio::test]
    async fn test_app_state_custom_router_injection_and_default() {
        let default_state = AppState::default();
        assert_eq!(default_state.router.name(), "EchoDynamicFallbackRouter");

        let mock_provider = Arc::new(MockInnerProvider {
            handler: Arc::new(|_t| {
                Ok(AudioStream {
                    url: "https://mock.stream/audio.flac".into(),
                    codec: AudioCodec::Flac,
                    bitrate_kbps: 4608,
                    sample_rate_hz: 96000,
                    is_lossless: true,
                    bit_depth: Some(24),
                    fidelity: Some(AudioFidelity::LosslessHiRes),
                    content_length_bytes: Some(50_000_000),
                    http_headers: HashMap::new(),
                })
            }),
        });

        let custom_router = Arc::new(DynamicFallbackRouter::new(None, None, mock_provider));
        let custom_state = AppState::with_router(custom_router);

        assert!(custom_state.router.tier1_lossless().is_none());
        assert!(custom_state.router.tier2_highbitrate().is_none());
        assert_eq!(custom_state.router.tier3_innertube().name(), "MockInnerProvider");
    }

    #[tokio::test]
    async fn test_playback_state_transitions() {
        let state = AppState::new();
        {
            let mut ps = state.playback_state.write().await;
            *ps = PlaybackState::Buffering;
        }
        assert_eq!(*state.playback_state.read().await, PlaybackState::Buffering);

        {
            let mut ps = state.playback_state.write().await;
            *ps = PlaybackState::Playing;
        }
        assert_eq!(*state.playback_state.read().await, PlaybackState::Playing);
    }

    #[tokio::test]
    async fn test_demo_track_stream_resolution_with_fidelity() {
        let state = AppState::new();
        let demo_track = Track {
            id: "radar_01".into(),
            title: "Solar Resonance".into(),
            artist: "Echo Harmonic Labs".into(),
            album: Some("Lossless Echoes".into()),
            duration_seconds: 245,
            thumbnail_url: None,
            isrc: None,
        };

        let stream_res = commands::resolve_stream_inner(&state, demo_track).await;
        assert!(stream_res.is_ok());
        let stream = stream_res.unwrap();
        assert_eq!(stream.codec, AudioCodec::Flac);
        assert_eq!(stream.bitrate_kbps, 1411);
        assert_eq!(stream.sample_rate_hz, 96000);
        assert!(stream.is_lossless);
        assert_eq!(stream.bit_depth, Some(24));
        assert_eq!(stream.fidelity, Some(AudioFidelity::LosslessHiRes));
    }

    #[tokio::test]
    async fn test_resolve_stream_inner_all_demo_stream_families() {
        let state = AppState::new();

        let families = vec![
            ("radar_ambient", AudioCodec::Flac, 1411, 96000, true, Some(24), Some(AudioFidelity::LosslessHiRes)),
            ("acoustic_folk", AudioCodec::Flac, 1411, 96000, true, Some(24), Some(AudioFidelity::LosslessHiRes)),
            ("radio_broadcast", AudioCodec::Aac, 256, 48000, false, None, Some(AudioFidelity::LossyHigh)),
            ("synth_retrowave", AudioCodec::Opus, 256, 48000, false, None, Some(AudioFidelity::LossyHigh)),
            ("synth_flac_demo", AudioCodec::Flac, 256, 48000, false, None, Some(AudioFidelity::LossyHigh)),
        ];

        for (id, exp_codec, exp_bitrate, exp_rate, exp_lossless, exp_bit_depth, exp_fidelity) in families {
            let track = Track {
                id: id.into(),
                title: format!("Title for {id}"),
                artist: "Demo Artist".into(),
                album: None,
                duration_seconds: 180,
                thumbnail_url: None,
                isrc: None,
            };

            let stream = commands::resolve_stream_inner(&state, track)
                .await
                .unwrap_or_else(|e| panic!("Failed to resolve demo track {id}: {e}"));

            assert_eq!(stream.codec, exp_codec, "Codec mismatch for {id}");
            assert_eq!(stream.bitrate_kbps, exp_bitrate, "Bitrate mismatch for {id}");
            assert_eq!(stream.sample_rate_hz, exp_rate, "Sample rate mismatch for {id}");
            assert_eq!(stream.is_lossless, exp_lossless, "Lossless mismatch for {id}");
            assert_eq!(stream.bit_depth, exp_bit_depth, "Bit depth mismatch for {id}");
            assert_eq!(stream.fidelity, exp_fidelity, "Fidelity mismatch for {id}");
        }
    }

    #[tokio::test]
    async fn test_resolve_stream_inner_router_invocation_with_custom_mock() {
        let mock_provider = Arc::new(MockInnerProvider {
            handler: Arc::new(|track| {
                Ok(AudioStream {
                    url: format!("https://mock.fallback.cdn/{}.flac", track.id),
                    codec: AudioCodec::Flac,
                    bitrate_kbps: 9216,
                    sample_rate_hz: 192000,
                    is_lossless: true,
                    bit_depth: Some(24),
                    fidelity: Some(AudioFidelity::LosslessHiRes),
                    content_length_bytes: Some(85_000_000),
                    http_headers: HashMap::new(),
                })
            }),
        });

        let router = Arc::new(DynamicFallbackRouter::new(None, None, mock_provider));
        let state = AppState::with_router(router);

        let live_track = Track {
            id: "prod_live_track_888".into(),
            title: "Live Master Track".into(),
            artist: "Studio Ensemble".into(),
            album: Some("Master Recordings".into()),
            duration_seconds: 320,
            thumbnail_url: Some("https://example.com/art.jpg".into()),
            isrc: Some("USRC17607839".into()),
        };

        let stream = commands::resolve_stream_inner(&state, live_track)
            .await
            .expect("resolve_stream_inner should invoke custom router");

        assert_eq!(stream.url, "https://mock.fallback.cdn/prod_live_track_888.flac");
        assert_eq!(stream.codec, AudioCodec::Flac);
        assert_eq!(stream.sample_rate_hz, 192000);
        assert_eq!(stream.bit_depth, Some(24));
        assert_eq!(stream.fidelity, Some(AudioFidelity::LosslessHiRes));
    }

    #[tokio::test]
    async fn test_resolve_stream_inner_router_error_propagation() {
        let mock_provider = Arc::new(MockInnerProvider {
            handler: Arc::new(|track| {
                Err(ProviderError::NoStreamsAvailable(track.id.clone()))
            }),
        });

        let router = Arc::new(DynamicFallbackRouter::new(None, None, mock_provider));
        let state = AppState::with_router(router);

        let non_demo_track = Track {
            id: "missing_track_999".into(),
            title: "Missing Audio".into(),
            artist: "Unknown Artist".into(),
            album: None,
            duration_seconds: 60,
            thumbnail_url: None,
            isrc: None,
        };

        let err = commands::resolve_stream_inner(&state, non_demo_track)
            .await
            .expect_err("Should propagate router error");

        assert!(
            err.contains("Stream resolution error"),
            "Error should be formatted properly: {err}"
        );
        assert!(
            err.contains("missing_track_999"),
            "Error should include track id: {err}"
        );
    }

    #[tokio::test]
    async fn test_router_health_report_verification() {
        let state = AppState::new();
        let default_report = state.router.check_health().await;
        assert!(default_report.tier1_lossless_configured);
        assert!(default_report.tier2_highbitrate_configured);
        assert!(default_report.tier3_innertube_available);

        let mock_provider = Arc::new(MockInnerProvider {
            handler: Arc::new(|_| Err(ProviderError::NoStreamsAvailable("x".into()))),
        });
        let unconfigured_router = DynamicFallbackRouter::new(None, None, mock_provider);
        let unconf_report = unconfigured_router.check_health().await;
        assert!(!unconf_report.tier1_lossless_configured);
        assert!(!unconf_report.tier1_lossless_healthy);
        assert!(!unconf_report.tier2_highbitrate_configured);
        assert!(!unconf_report.tier2_highbitrate_healthy);
        assert!(unconf_report.tier3_innertube_available);

        let json = serde_json::to_string(&unconf_report).expect("Serialization failed");
        let roundtrip: RouterHealthReport = serde_json::from_str(&json).expect("Deserialization failed");
        assert_eq!(roundtrip, unconf_report);
    }

    #[test]
    fn test_track_meta_payload_serialization_with_fidelity() {
        let payload = commands::TrackMetaPayload {
            id: "test_123".into(),
            title: "Crystal Glass".into(),
            artist: "Echo Artist".into(),
            album: Some("Debut".into()),
            duration_seconds: 200,
            thumbnail_url: Some("https://example.com/thumb.jpg".into()),
            codec: AudioCodec::Flac,
            bitrate_kbps: 4608,
            sample_rate_hz: 96000,
            is_lossless: true,
            bit_depth: Some(24),
            fidelity: Some(AudioFidelity::LosslessHiRes),
        };

        let json = serde_json::to_string(&payload).expect("Serialization failed");
        assert!(json.contains("\"codec\":\"Flac\""));
        assert!(json.contains("\"bitrate_kbps\":4608"));
        assert!(json.contains("\"sample_rate_hz\":96000"));
        assert!(json.contains("\"is_lossless\":true"));
        assert!(json.contains("\"bit_depth\":24"));
        assert!(json.contains("\"fidelity\":\"LosslessHiRes\""));

        let deserialized: commands::TrackMetaPayload = serde_json::from_str(&json).expect("Deserialization failed");
        assert_eq!(deserialized.fidelity, Some(AudioFidelity::LosslessHiRes));
        assert_eq!(deserialized.bit_depth, Some(24));
    }

    #[test]
    fn test_track_meta_payload_fidelity_matrix_serialization_and_deserialization() {
        struct Case {
            fidelity: Option<AudioFidelity>,
            bit_depth: Option<u8>,
            codec: AudioCodec,
            exp_fidelity_substr: Option<&'static str>,
            exp_bit_depth_substr: Option<&'static str>,
        }

        let matrix = vec![
            Case {
                fidelity: Some(AudioFidelity::LosslessHiRes),
                bit_depth: Some(24),
                codec: AudioCodec::Flac,
                exp_fidelity_substr: Some("\"fidelity\":\"LosslessHiRes\""),
                exp_bit_depth_substr: Some("\"bit_depth\":24"),
            },
            Case {
                fidelity: Some(AudioFidelity::LosslessCD),
                bit_depth: Some(16),
                codec: AudioCodec::Alac,
                exp_fidelity_substr: Some("\"fidelity\":\"LosslessCD\""),
                exp_bit_depth_substr: Some("\"bit_depth\":16"),
            },
            Case {
                fidelity: Some(AudioFidelity::Lossy320),
                bit_depth: None,
                codec: AudioCodec::Mp3,
                exp_fidelity_substr: Some("\"fidelity\":\"Lossy320\""),
                exp_bit_depth_substr: None,
            },
            Case {
                fidelity: Some(AudioFidelity::LossyHigh),
                bit_depth: None,
                codec: AudioCodec::Aac,
                exp_fidelity_substr: Some("\"fidelity\":\"LossyHigh\""),
                exp_bit_depth_substr: None,
            },
            Case {
                fidelity: Some(AudioFidelity::LossyStandard),
                bit_depth: None,
                codec: AudioCodec::Opus,
                exp_fidelity_substr: Some("\"fidelity\":\"LossyStandard\""),
                exp_bit_depth_substr: None,
            },
            Case {
                fidelity: None,
                bit_depth: None,
                codec: AudioCodec::Wav,
                exp_fidelity_substr: None,
                exp_bit_depth_substr: None,
            },
            Case {
                fidelity: None,
                bit_depth: Some(32),
                codec: AudioCodec::Unknown,
                exp_fidelity_substr: None,
                exp_bit_depth_substr: Some("\"bit_depth\":32"),
            },
            Case {
                fidelity: Some(AudioFidelity::LosslessHiRes),
                bit_depth: None,
                codec: AudioCodec::Flac,
                exp_fidelity_substr: Some("\"fidelity\":\"LosslessHiRes\""),
                exp_bit_depth_substr: None,
            },
        ];

        for (idx, case) in matrix.into_iter().enumerate() {
            let payload = commands::TrackMetaPayload {
                id: format!("track_{idx}"),
                title: format!("Title {idx}"),
                artist: "Matrix Artist".into(),
                album: None,
                duration_seconds: 150,
                thumbnail_url: None,
                codec: case.codec,
                bitrate_kbps: 1000,
                sample_rate_hz: 48000,
                is_lossless: case.codec == AudioCodec::Flac || case.codec == AudioCodec::Alac || case.codec == AudioCodec::Wav,
                bit_depth: case.bit_depth,
                fidelity: case.fidelity,
            };

            let json = serde_json::to_string(&payload)
                .unwrap_or_else(|e| panic!("Case {idx} serialization failed: {e}"));

            if let Some(exp_fid) = case.exp_fidelity_substr {
                assert!(json.contains(exp_fid), "Case {idx}: expected JSON to contain {exp_fid}, got: {json}");
            } else {
                assert!(!json.contains("\"fidelity\""), "Case {idx}: expected fidelity to be omitted, got: {json}");
            }

            if let Some(exp_bd) = case.exp_bit_depth_substr {
                assert!(json.contains(exp_bd), "Case {idx}: expected JSON to contain {exp_bd}, got: {json}");
            } else {
                assert!(!json.contains("\"bit_depth\""), "Case {idx}: expected bit_depth to be omitted, got: {json}");
            }

            let roundtrip: commands::TrackMetaPayload = serde_json::from_str(&json)
                .unwrap_or_else(|e| panic!("Case {idx} deserialization failed: {e}"));

            assert_eq!(roundtrip.fidelity, case.fidelity, "Case {idx} fidelity mismatch");
            assert_eq!(roundtrip.bit_depth, case.bit_depth, "Case {idx} bit_depth mismatch");
            assert_eq!(roundtrip.codec, case.codec, "Case {idx} codec mismatch");
        }
    }

    #[test]
    fn test_track_meta_payload_deserialization_edge_cases_and_nulls() {
        // Explicit null fields
        let json_with_nulls = r#"{
            "id": "null_test_01",
            "title": "Ambient Nulls",
            "artist": "Zen Master",
            "album": null,
            "duration_seconds": 300,
            "thumbnail_url": null,
            "codec": "Flac",
            "bitrate_kbps": 1411,
            "sample_rate_hz": 44100,
            "is_lossless": true,
            "bit_depth": null,
            "fidelity": null
        }"#;

        let payload1: commands::TrackMetaPayload = serde_json::from_str(json_with_nulls)
            .expect("Should deserialize payload with explicit nulls");
        assert_eq!(payload1.id, "null_test_01");
        assert_eq!(payload1.album, None);
        assert_eq!(payload1.thumbnail_url, None);
        assert_eq!(payload1.bit_depth, None);
        assert_eq!(payload1.fidelity, None);

        // Completely omitted optional fields
        let json_omitted = r#"{
            "id": "omitted_test_02",
            "title": "Minimalist",
            "artist": "Stripped Down",
            "duration_seconds": 180,
            "codec": "Opus",
            "bitrate_kbps": 160,
            "sample_rate_hz": 48000,
            "is_lossless": false
        }"#;

        let payload2: commands::TrackMetaPayload = serde_json::from_str(json_omitted)
            .expect("Should deserialize payload with omitted optional fields");
        assert_eq!(payload2.id, "omitted_test_02");
        assert_eq!(payload2.album, None);
        assert_eq!(payload2.thumbnail_url, None);
        assert_eq!(payload2.bit_depth, None);
        assert_eq!(payload2.fidelity, None);

        // Extra unknown fields ignored
        let json_extra_fields = r#"{
            "id": "extra_fields_03",
            "title": "Forward Compatible",
            "artist": "Extensible",
            "duration_seconds": 210,
            "codec": "Aac",
            "bitrate_kbps": 256,
            "sample_rate_hz": 44100,
            "is_lossless": false,
            "unrecognized_future_field": 42,
            "vendor_extension": {"nested": "value"}
        }"#;

        let payload3: commands::TrackMetaPayload = serde_json::from_str(json_extra_fields)
            .expect("Should deserialize payload with extra unknown fields");
        assert_eq!(payload3.id, "extra_fields_03");
    }

    #[test]
    fn test_track_meta_payload_deserialization_invalid_inputs_rejected() {
        // Unknown fidelity string variant
        let bad_fidelity = r#"{
            "id": "bad_fid",
            "title": "Bad Fidelity",
            "artist": "Glitch",
            "duration_seconds": 120,
            "codec": "Flac",
            "bitrate_kbps": 1411,
            "sample_rate_hz": 44100,
            "is_lossless": true,
            "fidelity": "UltraMasterUncompressed"
        }"#;
        assert!(serde_json::from_str::<commands::TrackMetaPayload>(bad_fidelity).is_err());

        // Unknown codec variant
        let bad_codec = r#"{
            "id": "bad_codec",
            "title": "Bad Codec",
            "artist": "Glitch",
            "duration_seconds": 120,
            "codec": "RealAudioG2",
            "bitrate_kbps": 1411,
            "sample_rate_hz": 44100,
            "is_lossless": true
        }"#;
        assert!(serde_json::from_str::<commands::TrackMetaPayload>(bad_codec).is_err());

        // Invalid type for bit_depth (string instead of integer)
        let bad_bit_depth_type = r#"{
            "id": "bad_bd",
            "title": "Bad Bit Depth",
            "artist": "Glitch",
            "duration_seconds": 120,
            "codec": "Flac",
            "bitrate_kbps": 1411,
            "sample_rate_hz": 44100,
            "is_lossless": true,
            "bit_depth": "twenty_four"
        }"#;
        assert!(serde_json::from_str::<commands::TrackMetaPayload>(bad_bit_depth_type).is_err());

        // Negative integer for bit_depth
        let negative_bit_depth = r#"{
            "id": "neg_bd",
            "title": "Negative Bit Depth",
            "artist": "Glitch",
            "duration_seconds": 120,
            "codec": "Flac",
            "bitrate_kbps": 1411,
            "sample_rate_hz": 44100,
            "is_lossless": true,
            "bit_depth": -16
        }"#;
        assert!(serde_json::from_str::<commands::TrackMetaPayload>(negative_bit_depth).is_err());

        // Missing required field 'id'
        let missing_id = r#"{
            "title": "Missing ID",
            "artist": "Glitch",
            "duration_seconds": 120,
            "codec": "Flac",
            "bitrate_kbps": 1411,
            "sample_rate_hz": 44100,
            "is_lossless": true
        }"#;
        assert!(serde_json::from_str::<commands::TrackMetaPayload>(missing_id).is_err());
    }
}

