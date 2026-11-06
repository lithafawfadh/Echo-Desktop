//! Echo Music audio playback controller, decoder, ring buffer, and streaming engine.

pub mod decoder;
pub mod engine;
pub mod ring_buffer;

pub use decoder::{ChannelStreamReader, DecodeStats, StreamDecoder};
pub use engine::StreamingAudioEngine;
pub use ring_buffer::{
    calculate_position_ms, AudioConsumer, AudioProducer, AudioRingBuffer, BufferHealthMetrics,
    CpalOutputStream, DEFAULT_PRE_ROLL_SAMPLES, DEFAULT_RING_BUFFER_CAPACITY,
};

use echo_core::PlaybackProgress;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::broadcast;

#[derive(Error, Debug)]
pub enum AudioError {
    #[error("Device initialization failed: {0}")]
    DeviceInit(String),
    #[error("Audio decode error: {0}")]
    DecodeError(String),
    #[error("Playback error: {0}")]
    Playback(String),
}

/// Thread-safe controller for audio playback state
pub struct AudioController {
    is_playing: Arc<AtomicBool>,
    position_ms: Arc<AtomicU64>,
    duration_ms: Arc<AtomicU64>,
    progress_tx: broadcast::Sender<PlaybackProgress>,
}

impl Default for AudioController {
    fn default() -> Self {
        let (progress_tx, _) = broadcast::channel(32);
        Self {
            is_playing: Arc::new(AtomicBool::new(false)),
            position_ms: Arc::new(AtomicU64::new(0)),
            duration_ms: Arc::new(AtomicU64::new(0)),
            progress_tx,
        }
    }
}

impl AudioController {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_playing(&self) -> bool {
        self.is_playing.load(Ordering::Relaxed)
    }

    pub fn set_playing(&self, playing: bool) {
        self.is_playing.store(playing, Ordering::Relaxed);
    }

    pub fn is_playing_arc(&self) -> Arc<AtomicBool> {
        self.is_playing.clone()
    }

    pub fn position_ms(&self) -> u64 {
        self.position_ms.load(Ordering::Relaxed)
    }

    pub fn position_ms_arc(&self) -> Arc<AtomicU64> {
        self.position_ms.clone()
    }

    pub fn duration_ms(&self) -> u64 {
        self.duration_ms.load(Ordering::Relaxed)
    }

    pub fn duration_ms_arc(&self) -> Arc<AtomicU64> {
        self.duration_ms.clone()
    }

    pub fn update_progress(&self, position_ms: u64, duration_ms: u64) {
        self.position_ms.store(position_ms, Ordering::Relaxed);
        self.duration_ms.store(duration_ms, Ordering::Relaxed);
        let _ = self.progress_tx.send(PlaybackProgress {
            position_ms,
            duration_ms,
        });
    }

    pub fn subscribe_progress(&self) -> broadcast::Receiver<PlaybackProgress> {
        self.progress_tx.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_controller_state() {
        let controller = AudioController::new();
        assert!(!controller.is_playing());

        controller.set_playing(true);
        assert!(controller.is_playing());

        controller.update_progress(1500, 3000);
        assert_eq!(controller.position_ms(), 1500);
        assert_eq!(controller.duration_ms(), 3000);
    }
}
