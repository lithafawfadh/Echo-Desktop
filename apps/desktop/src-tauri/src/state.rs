use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use echo_core::{AudioStream, PlaybackState, Track};
use echo_audio::{AudioController, StreamingAudioEngine};
use echo_provider::YouTubeMusicClient;
use echo_lyrics::LrcLibClient;

pub struct AppState {
    pub provider: Arc<YouTubeMusicClient>,
    pub audio_controller: Arc<AudioController>,
    pub audio_engine: Arc<StreamingAudioEngine>,
    pub lyrics_client: Arc<LrcLibClient>,
    pub current_track: Arc<RwLock<Option<Track>>>,
    pub current_stream: Arc<RwLock<Option<AudioStream>>>,
    pub playback_state: Arc<RwLock<PlaybackState>>,
    pub playback_abort: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
}

impl AppState {
    pub fn new() -> Self {
        let audio_controller = Arc::new(AudioController::new());
        let audio_engine = Arc::new(StreamingAudioEngine::new(audio_controller.clone()));
        let provider = Arc::new(YouTubeMusicClient::new().expect("Failed to initialize InnerTube client"));
        let lyrics_client = Arc::new(LrcLibClient::new());

        Self {
            provider,
            audio_controller,
            audio_engine,
            lyrics_client,
            current_track: Arc::new(RwLock::new(None)),
            current_stream: Arc::new(RwLock::new(None)),
            playback_state: Arc::new(RwLock::new(PlaybackState::Stopped)),
            playback_abort: Arc::new(Mutex::new(None)),
        }
    }
}
