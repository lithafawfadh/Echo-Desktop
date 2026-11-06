use std::time::Duration;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State, Window};
use tracing::{info, warn};

use echo_core::{AudioCodec, AudioStream, Lyrics, PlaybackProgress, PlaybackState, Track};
use echo_provider::MusicProvider;

use crate::state::AppState;

/// Combined track metadata and stream quality payload for frontend UI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackMetaPayload {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub duration_seconds: u32,
    pub thumbnail_url: Option<String>,
    pub codec: AudioCodec,
    pub bitrate_kbps: u32,
    pub sample_rate_hz: u32,
    pub is_lossless: bool,
}

/// Searches tracks via InnerTube YouTube Music provider
#[tauri::command]
pub async fn search_tracks(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<Track>, String> {
    info!("IPC search_tracks: query='{}'", query);
    state
        .provider
        .search(&query)
        .await
        .map_err(|e| format!("Search error: {e}"))
}

/// Inner stream resolver logic usable directly in unit tests and commands
pub async fn resolve_stream_inner(
    state: &AppState,
    track: Track,
) -> Result<AudioStream, String> {
    info!("IPC resolve_stream: track='{}' ({})", track.title, track.id);

    // Provide demo stream specs for discovery bento cards or local demo ids
    if track.id.starts_with("radar_")
        || track.id.starts_with("synth_")
        || track.id.starts_with("radio_")
        || track.id.starts_with("acoustic_")
    {
        return Ok(AudioStream {
            url: "https://echo.local/stream.mp4".into(),
            codec: if track.id.contains("flac") || track.id.starts_with("radar_") || track.id.starts_with("acoustic_") {
                AudioCodec::Flac
            } else if track.id.starts_with("radio_") {
                AudioCodec::Aac
            } else {
                AudioCodec::Opus
            },
            bitrate_kbps: if track.id.starts_with("radar_") || track.id.starts_with("acoustic_") {
                1411
            } else {
                256
            },
            sample_rate_hz: if track.id.starts_with("radar_") || track.id.starts_with("acoustic_") {
                96000
            } else {
                48000
            },
            is_lossless: track.id.starts_with("radar_") || track.id.starts_with("acoustic_"),
        });
    }

    state
        .provider
        .resolve_stream(&track)
        .await
        .map_err(|e| format!("Stream resolution error: {e}"))
}

/// Resolves a playable audio stream for a given track
#[tauri::command]
pub async fn resolve_stream(
    state: State<'_, AppState>,
    track: Track,
) -> Result<AudioStream, String> {
    resolve_stream_inner(&state, track).await
}

/// Initiates playback for a track, resolving stream and fetching lyrics
#[tauri::command]
pub async fn play_stream(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    track: Track,
) -> Result<AudioStream, String> {
    info!("IPC play_stream: track='{}' ({})", track.title, track.id);

    // Cancel any previous playback task
    {
        let mut abort_guard = state.playback_abort.lock().await;
        if let Some(abort_tx) = abort_guard.take() {
            let _ = abort_tx.send(());
        }
    }

    // Resolve stream with graceful fallback for demo/curated tracks
    let stream = resolve_stream(state.clone(), track.clone()).await?;

    let meta_payload = TrackMetaPayload {
        id: track.id.clone(),
        title: track.title.clone(),
        artist: track.artist.clone(),
        album: track.album.clone(),
        duration_seconds: track.duration_seconds,
        thumbnail_url: track.thumbnail_url.clone(),
        codec: stream.codec,
        bitrate_kbps: stream.bitrate_kbps,
        sample_rate_hz: stream.sample_rate_hz,
        is_lossless: stream.is_lossless,
    };

    // Update state
    {
        let mut t_guard = state.current_track.write().await;
        *t_guard = Some(track.clone());
    }
    {
        let mut s_guard = state.current_stream.write().await;
        *s_guard = Some(stream.clone());
    }
    {
        let mut state_guard = state.playback_state.write().await;
        *state_guard = PlaybackState::Playing;
    }

    // Emit combined track metadata and playing state
    let _ = app_handle.emit("playback://track_change", &meta_payload);
    let _ = app_handle.emit("playback://status", &PlaybackState::Playing);

    // Asynchronously fetch synchronized lyrics in background
    let lyrics_client = state.lyrics_client.clone();
    let app_handle_lyrics = app_handle.clone();
    let lyrics_title = track.title.clone();
    let track_title = track.title.clone();
    let track_artist = track.artist.clone();
    let duration = track.duration_seconds;

    tokio::spawn(async move {
        match lyrics_client
            .fetch_lyrics(&lyrics_title, &track_artist, Some(duration))
            .await
        {
            Ok(lyrics) => {
                info!("Lyrics loaded for '{}': {} lines", lyrics_title, lyrics.synced.len());
                let _ = app_handle_lyrics.emit("lyrics://ready", &lyrics);
            }
            Err(e) => {
                warn!("Lyrics fetch failed for '{}': {e}", lyrics_title);
                let _ = app_handle_lyrics.emit("lyrics://ready", &Lyrics::default());
            }
        }
    });

    // Spawn playback engine task with graceful fallback for headless/no-audio environments
    let (abort_tx, mut abort_rx) = tokio::sync::oneshot::channel::<()>();
    {
        let mut abort_guard = state.playback_abort.lock().await;
        *abort_guard = Some(abort_tx);
    }

    let controller = state.audio_controller.clone();
    let engine = state.audio_engine.clone();
    let stream_url = stream.url.clone();
    let duration_ms = (track.duration_seconds as u64) * 1000;
    let app_handle_progress = app_handle.clone();
    let state_status = state.playback_state.clone();

    tokio::spawn(async move {
        controller.set_playing(true);
        controller.update_progress(0, duration_ms);

        // Forward progress events to Tauri frontend
        let mut progress_rx = controller.subscribe_progress();
        let app_handle_ev = app_handle_progress.clone();
        tokio::spawn(async move {
            while let Ok(progress) = progress_rx.recv().await {
                let _ = app_handle_ev.emit("playback://progress", &progress);
            }
        });

        // Attempt playback via CPAL engine; if device init fails, use simulated clock
        tokio::select! {
            _ = &mut abort_rx => {
                info!("Playback aborted for '{}'", track_title);
            }
            play_res = engine.play_stream(&stream_url) => {
                if let Err(e) = play_res {
                    warn!("Direct audio output unavailable ({e}), using simulated playback clock");
                    let step = 100u64;
                    let mut interval = tokio::time::interval(Duration::from_millis(step));
                    let mut pos = controller.position_ms();

                    while pos < duration_ms {
                        tokio::select! {
                            _ = &mut abort_rx => {
                                info!("Simulated playback aborted for '{}'", track_title);
                                break;
                            }
                            _ = interval.tick() => {
                                if controller.is_playing() {
                                    // Advance position smoothly from actual controller timestamp (handles seek while playing)
                                    pos = controller.position_ms() + step;
                                    if pos > duration_ms {
                                        pos = duration_ms;
                                    }
                                    controller.update_progress(pos, duration_ms);
                                } else {
                                    // When paused, stay aligned with controller in case user seeked while paused
                                    pos = controller.position_ms();
                                }
                            }
                        }
                    }
                }
            }
        }

        controller.set_playing(false);
        {
            let mut sg = state_status.write().await;
            *sg = PlaybackState::Stopped;
        }
        let _ = app_handle_progress.emit("playback://status", &PlaybackState::Stopped);
    });

    Ok(stream)
}

/// Pauses current playback stream
#[tauri::command]
pub async fn pause_stream(
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    info!("IPC pause_stream");
    state.audio_controller.set_playing(false);
    {
        let mut sg = state.playback_state.write().await;
        *sg = PlaybackState::Paused;
    }
    let _ = app_handle.emit("playback://status", &PlaybackState::Paused);
    Ok(())
}

/// Retrieves current playback state
#[tauri::command]
pub async fn get_playback_state(
    state: State<'_, AppState>,
) -> Result<PlaybackState, String> {
    let sg = state.playback_state.read().await;
    Ok(*sg)
}

/// Fetches lyrics from LRCLIB for a specified track
#[tauri::command]
pub async fn get_lyrics(
    state: State<'_, AppState>,
    track_name: String,
    artist_name: String,
    duration_seconds: Option<u32>,
) -> Result<Lyrics, String> {
    info!("IPC get_lyrics: track='{}', artist='{}'", track_name, artist_name);
    state
        .lyrics_client
        .fetch_lyrics(&track_name, &artist_name, duration_seconds)
        .await
        .map_err(|e| format!("Lyrics error: {e}"))
}

/// Alias for get_lyrics adhering to DESIGN_SYSTEM.md contract
#[tauri::command]
pub async fn fetch_lyrics(
    state: State<'_, AppState>,
    track_name: String,
    artist_name: String,
) -> Result<Lyrics, String> {
    get_lyrics(state, track_name, artist_name, None).await
}

/// Retrieves downscaled album thumbnail URL or cached blob identifier
#[tauri::command]
pub async fn get_album_thumb(
    state: State<'_, AppState>,
    track_id: String,
    size: Option<String>,
) -> Result<String, String> {
    info!("IPC get_album_thumb: track_id='{}', size={:?}", track_id, size);
    let current_track = state.current_track.read().await;
    if let Some(ref t) = *current_track {
        if t.id == track_id {
            if let Some(ref thumb) = t.thumbnail_url {
                return Ok(thumb.clone());
            }
        }
    }
    Ok(format!("https://images.unsplash.com/photo-1618005182384-a83a8bd57fbe?w={}&q=80",
        if size.as_deref() == Some("sm") { 96 } else { 384 }))
}

/// Seeks playback to position in milliseconds
#[tauri::command]
pub async fn seek_to(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    position_ms: u64,
) -> Result<(), String> {
    info!("IPC seek_to: position_ms={}", position_ms);
    let dur = state.audio_controller.duration_ms();
    state.audio_controller.update_progress(position_ms, dur);
    let _ = app_handle.emit(
        "playback://progress",
        &PlaybackProgress {
            position_ms,
            duration_ms: dur,
        },
    );
    Ok(())
}

/// Toggles between playing and paused state
#[tauri::command]
pub async fn toggle_playback(
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let is_playing = state.audio_controller.is_playing();
    if is_playing {
        pause_stream(app_handle, state).await?;
        Ok(false)
    } else {
        state.audio_controller.set_playing(true);
        {
            let mut sg = state.playback_state.write().await;
            *sg = PlaybackState::Playing;
        }
        let _ = app_handle.emit("playback://status", &PlaybackState::Playing);
        Ok(true)
    }
}

/// Previous track handler - restarts track from beginning
#[tauri::command]
pub async fn previous_track(
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    info!("IPC previous_track: restarting track");
    seek_to(app_handle, state, 0).await
}

/// Next track handler - resets progress and stops active track
#[tauri::command]
pub async fn next_track(
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    info!("IPC next_track");
    state.audio_controller.set_playing(false);
    state.audio_controller.update_progress(0, 0);
    {
        let mut sg = state.playback_state.write().await;
        *sg = PlaybackState::Stopped;
    }
    let _ = app_handle.emit("playback://status", &PlaybackState::Stopped);
    let _ = app_handle.emit(
        "playback://progress",
        &PlaybackProgress {
            position_ms: 0,
            duration_ms: 0,
        },
    );
    Ok(())
}

/// Minimizes the Tauri window
#[tauri::command]
pub async fn minimize_window(window: Window) -> Result<(), String> {
    window.minimize().map_err(|e| e.to_string())
}

/// Toggles maximize state of the Tauri window
#[tauri::command]
pub async fn toggle_maximize_window(window: Window) -> Result<(), String> {
    if window.is_maximized().map_err(|e| e.to_string())? {
        window.unmaximize().map_err(|e| e.to_string())
    } else {
        window.maximize().map_err(|e| e.to_string())
    }
}

/// Closes the Tauri window
#[tauri::command]
pub async fn close_window(window: Window) -> Result<(), String> {
    window.close().map_err(|e| e.to_string())
}
