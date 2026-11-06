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
            commands::minimize_window,
            commands::toggle_maximize_window,
            commands::close_window,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Echo Desktop application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use echo_core::{AudioCodec, PlaybackState, Track};

    #[tokio::test]
    async fn test_app_state_initialization() {
        let state = AppState::new();
        let playback_state = *state.playback_state.read().await;
        assert_eq!(playback_state, PlaybackState::Stopped);
        assert!(!state.audio_controller.is_playing());
        assert_eq!(state.audio_controller.position_ms(), 0);
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
    async fn test_demo_track_stream_resolution() {
        let state = AppState::new();
        let demo_track = Track {
            id: "radar_01".into(),
            title: "Solar Resonance".into(),
            artist: "Echo Harmonic Labs".into(),
            album: Some("Lossless Echoes".into()),
            duration_seconds: 245,
            thumbnail_url: None,
        };

        let stream_res = commands::resolve_stream_inner(&state, demo_track).await;
        assert!(stream_res.is_ok());
        let stream = stream_res.unwrap();
        assert_eq!(stream.codec, AudioCodec::Flac);
        assert_eq!(stream.bitrate_kbps, 1411);
        assert_eq!(stream.sample_rate_hz, 96000);
        assert!(stream.is_lossless);
    }

    #[test]
    fn test_track_meta_payload_serialization() {
        let payload = commands::TrackMetaPayload {
            id: "test_123".into(),
            title: "Crystal Glass".into(),
            artist: "Echo Artist".into(),
            album: Some("Debut".into()),
            duration_seconds: 200,
            thumbnail_url: Some("https://example.com/thumb.jpg".into()),
            codec: AudioCodec::Flac,
            bitrate_kbps: 1411,
            sample_rate_hz: 96000,
            is_lossless: true,
        };

        let json = serde_json::to_string(&payload).expect("Serialization failed");
        assert!(json.contains("\"codec\":\"Flac\""));
        assert!(json.contains("\"bitrate_kbps\":1411"));
        assert!(json.contains("\"sample_rate_hz\":96000"));
        assert!(json.contains("\"is_lossless\":true"));
    }
}
