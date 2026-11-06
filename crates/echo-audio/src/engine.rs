//! Coordinating streaming audio engine for HTTP streaming, Symphonia decoding,
//! lock-free ring buffering, and CPAL / headless playback.

use crate::decoder::{ChannelStreamReader, StreamDecoder};
use crate::ring_buffer::{
    calculate_position_ms, create_cpal_output_stream, AudioRingBuffer, BufferHealthMetrics,
};
use crate::{AudioController, AudioError};
use bytes::Bytes;
use futures_util::StreamExt;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use symphonia::core::probe::Hint;
use tracing::{error, info};

/// Coordinating audio engine for HTTP streaming, Symphonia decoding,
/// lock-free ring buffering, and CPAL / Headless playback.
pub struct StreamingAudioEngine {
    controller: Arc<AudioController>,
    client: reqwest::Client,
}

impl StreamingAudioEngine {
    pub fn new(controller: Arc<AudioController>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("Echo-Desktop/0.1.0")
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { controller, client }
    }

    pub fn with_client(controller: Arc<AudioController>, client: reqwest::Client) -> Self {
        Self { controller, client }
    }

    pub fn controller(&self) -> &Arc<AudioController> {
        &self.controller
    }

    /// Live playback mode: Streams audio from `url`, decodes in worker thread,
    /// pushes to lock-free ring buffer, and outputs to default CPAL device.
    pub async fn play_stream(&self, url: &str) -> Result<(), AudioError> {
        self.controller.set_playing(true);
        let stop_flag = Arc::new(AtomicBool::new(false));

        // 1. Initiate HTTP stream
        let (byte_tx, byte_rx) = tokio::sync::mpsc::channel::<Bytes>(32);
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| AudioError::Playback(format!("HTTP request failed: {e}")))?;

        if !response.status().is_success() {
            self.controller.set_playing(false);
            return Err(AudioError::Playback(format!(
                "HTTP status error: {}",
                response.status()
            )));
        }

        let mut hint = Hint::new();
        if let Some(ct) = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
        {
            hint.mime_type(ct);
        }
        if url.contains(".mp3") {
            hint.with_extension("mp3");
            hint.mime_type("audio/mpeg");
        } else if url.contains(".m4a") || url.contains("audio/mp4") || url.contains("googlevideo") {
            hint.with_extension("m4a");
            hint.mime_type("audio/mp4");
        }

        // Spawn network pump
        let stop_pump = stop_flag.clone();
        tokio::spawn(async move {
            let mut stream = response.bytes_stream();
            while let Some(chunk_res) = stream.next().await {
                if stop_pump.load(Ordering::Relaxed) {
                    break;
                }
                match chunk_res {
                    Ok(bytes) => {
                        if byte_tx.send(bytes).await.is_err() {
                            break; // Decoder dropped reader
                        }
                    }
                    Err(e) => {
                        error!("HTTP streaming chunk error: {e}");
                        break;
                    }
                }
            }
        });

        // 2. Set up SPSC ring buffer & metrics
        let (mut producer, consumer) = AudioRingBuffer::default_pair();
        let metrics = Arc::new(BufferHealthMetrics::new());

        // 3. Initialize CPAL output
        let cpal_stream = create_cpal_output_stream(
            consumer,
            self.controller.is_playing_arc(),
            metrics.clone(),
            None,
            None,
        )?;

        let sample_rate = cpal_stream.sample_rate();
        let channels = cpal_stream.channels();
        cpal_stream.play()?;

        // 4. Spawn decoupled progress reporter (10 Hz)
        let progress_controller = self.controller.clone();
        let progress_metrics = metrics.clone();
        let progress_stop = stop_flag.clone();
        let progress_handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(100));
            while !progress_stop.load(Ordering::Relaxed) && progress_controller.is_playing() {
                interval.tick().await;
                let consumed = progress_metrics.total_consumed();
                let pos_ms = calculate_position_ms(consumed, sample_rate, channels);
                let duration_ms = progress_controller.duration_ms();
                progress_controller.update_progress(pos_ms, duration_ms);
            }
        });

        // 5. Run decoder on dedicated OS thread
        let stop_dec = stop_flag.clone();
        let decode_handle = std::thread::Builder::new()
            .name("echo-audio-decoder".into())
            .spawn(move || {
                let reader = ChannelStreamReader::new(byte_rx);
                let mut decoder = StreamDecoder::new_with_hint(reader, Some(hint))?;
                decoder.decode_into_ring_buffer(producer.inner_mut(), None, Some(&stop_dec))
            })
            .map_err(|e| AudioError::Playback(format!("Failed to spawn decoder thread: {e}")))?;

        // Wait for decode completion or stop signal
        let decode_res = decode_handle
            .join()
            .map_err(|_| AudioError::DecodeError("Decoder thread panicked".into()))?;

        stop_flag.store(true, Ordering::Relaxed);
        self.controller.set_playing(false);
        let _ = progress_handle.await;

        match decode_res {
            Ok(stats) => {
                info!(
                    "Playback decode completed: {} packets, {} samples",
                    stats.packets_decoded, stats.samples_decoded
                );
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// Headless verification mode: Streams from `url`, decodes up to `max_packets`
    /// into ring buffer, and drains samples via fast-forward consumer without touching CPAL.
    /// Exits cleanly and returns decoded packet count.
    pub async fn test_decode_stream(
        &self,
        url: &str,
        max_packets: usize,
    ) -> Result<usize, AudioError> {
        let (byte_tx, byte_rx) = tokio::sync::mpsc::channel::<Bytes>(32);
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| AudioError::Playback(format!("HTTP request failed: {e}")))?;

        if !response.status().is_success() {
            return Err(AudioError::Playback(format!(
                "HTTP status error: {}",
                response.status()
            )));
        }

        let mut hint = Hint::new();
        if let Some(ct) = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
        {
            hint.mime_type(ct);
        } else if url.contains("m4a") || url.contains("googlevideo") {
            hint.mime_type("audio/mp4");
            hint.with_extension("m4a");
        }

        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_pump = stop_flag.clone();

        tokio::spawn(async move {
            let mut stream = response.bytes_stream();
            while let Some(chunk_res) = stream.next().await {
                if stop_pump.load(Ordering::Relaxed) {
                    break;
                }
                match chunk_res {
                    Ok(bytes) => {
                        if byte_tx.send(bytes).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        error!("HTTP streaming chunk error: {e}");
                        break;
                    }
                }
            }
        });

        let reader = ChannelStreamReader::new(byte_rx);
        self.test_decode_reader_with_hint(reader, max_packets, stop_flag, Some(hint))
            .await
    }

    /// In-memory headless decoding from any `std::io::Read + Send + 'static` source.
    /// Used by unit and offline tests with zero network involvement.
    pub async fn test_decode_reader<R: Read + Send + 'static>(
        &self,
        reader: R,
        max_packets: usize,
        stop_flag: Arc<AtomicBool>,
    ) -> Result<usize, AudioError> {
        self.test_decode_reader_with_hint(reader, max_packets, stop_flag, None)
            .await
    }

    /// In-memory headless decoding with optional format hint.
    pub async fn test_decode_reader_with_hint<R: Read + Send + 'static>(
        &self,
        reader: R,
        max_packets: usize,
        stop_flag: Arc<AtomicBool>,
        hint: Option<Hint>,
    ) -> Result<usize, AudioError> {
        let (mut producer, mut consumer) = AudioRingBuffer::default_pair();

        let stop_dec = stop_flag.clone();
        let decoder_handle = std::thread::Builder::new()
            .name("echo-headless-decoder".into())
            .spawn(move || {
                let mut decoder = StreamDecoder::from_reader(reader, hint)?;
                decoder.decode_into_ring_buffer(
                    producer.inner_mut(),
                    Some(max_packets),
                    Some(&stop_dec),
                )
            })
            .map_err(|e| AudioError::Playback(format!("Failed to spawn decoder thread: {e}")))?;

        // Fast-forward consumer drain
        let mut drain_buf = [0.0f32; 2048];
        let mut total_drained = 0usize;

        while !decoder_handle.is_finished() || !consumer.is_empty() {
            let n = consumer.pop_samples(&mut drain_buf);
            if n > 0 {
                total_drained += n;
            } else {
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        }

        let stats = decoder_handle
            .join()
            .map_err(|_| AudioError::DecodeError("Decoder thread panicked".into()))??;

        info!(
            "Headless decode complete: {} packets decoded, {} samples drained",
            stats.packets_decoded, total_drained
        );
        stop_flag.store(true, Ordering::Relaxed);
        Ok(stats.packets_decoded)
    }
}
