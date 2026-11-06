//! Bounded lock-free SPSC ring-buffer, underrun silence padding, and CPAL output stream.

use crate::AudioError;
use ringbuf::traits::{Consumer, Observer, Producer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

#[cfg(feature = "cpal")]
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
#[cfg(feature = "cpal")]
use tracing::{error, info};

/// Default capacity for the audio sample ring buffer (240,000 f32 samples ~ 960 KB).
/// At 48,000 Hz stereo, provides 2.50 seconds of buffer.
/// At 44,100 Hz stereo, provides 2.72 seconds of buffer.
pub const DEFAULT_RING_BUFFER_CAPACITY: usize = 240_000;

/// Default pre-roll buffer size before unpausing stream playback (~0.5s of audio).
pub const DEFAULT_PRE_ROLL_SAMPLES: usize = 48_000;

/// Diagnostic buffer health metrics.
#[derive(Debug, Default)]
pub struct BufferHealthMetrics {
    total_produced: AtomicU64,
    total_consumed: AtomicU64,
    underrun_count: AtomicU64,
    underrun_samples: AtomicU64,
}

impl BufferHealthMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn record_produced(&self, count: usize) {
        self.total_produced
            .fetch_add(count as u64, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_consumed(&self, count: usize) {
        self.total_consumed
            .fetch_add(count as u64, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_underrun(&self, missing: usize) {
        self.underrun_count.fetch_add(1, Ordering::Relaxed);
        self.underrun_samples
            .fetch_add(missing as u64, Ordering::Relaxed);
    }

    pub fn total_produced(&self) -> u64 {
        self.total_produced.load(Ordering::Relaxed)
    }

    pub fn total_consumed(&self) -> u64 {
        self.total_consumed.load(Ordering::Relaxed)
    }

    pub fn underrun_count(&self) -> u64 {
        self.underrun_count.load(Ordering::Relaxed)
    }

    pub fn underrun_samples(&self) -> u64 {
        self.underrun_samples.load(Ordering::Relaxed)
    }

    pub fn reset(&self) {
        self.total_produced.store(0, Ordering::Relaxed);
        self.total_consumed.store(0, Ordering::Relaxed);
        self.underrun_count.store(0, Ordering::Relaxed);
        self.underrun_samples.store(0, Ordering::Relaxed);
    }
}

/// SPSC Ring-Buffer Producer residing on the decoding thread.
pub struct AudioProducer {
    inner: HeapProd<f32>,
}

impl AudioProducer {
    pub fn new(inner: HeapProd<f32>) -> Self {
        Self { inner }
    }

    /// Pushes interleaved f32 PCM samples into the ring buffer.
    /// Returns the number of samples actually written.
    #[inline]
    pub fn push_samples(&mut self, samples: &[f32]) -> usize {
        self.inner.push_slice(samples)
    }

    #[inline]
    pub fn vacant_len(&self) -> usize {
        self.inner.vacant_len()
    }

    #[inline]
    pub fn occupied_len(&self) -> usize {
        self.inner.occupied_len()
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.inner.capacity().get()
    }

    #[inline]
    pub fn is_full(&self) -> bool {
        self.inner.is_full()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    #[inline]
    pub fn inner_mut(&mut self) -> &mut HeapProd<f32> {
        &mut self.inner
    }

    #[inline]
    pub fn into_inner(self) -> HeapProd<f32> {
        self.inner
    }
}

/// SPSC Ring-Buffer Consumer residing on the CPAL audio callback or headless thread.
pub struct AudioConsumer {
    inner: HeapCons<f32>,
}

impl AudioConsumer {
    pub fn new(inner: HeapCons<f32>) -> Self {
        Self { inner }
    }

    /// Pops raw samples into `output`. Returns count of valid samples read.
    #[inline]
    pub fn pop_samples(&mut self, output: &mut [f32]) -> usize {
        self.inner.pop_slice(output)
    }

    /// Pops samples into `output`, filling any unsupplied remainder with 0.0f32 silence.
    /// Returns the count of actual audio samples popped from the buffer.
    #[inline]
    pub fn pop_samples_with_padding(
        &mut self,
        output: &mut [f32],
        metrics: Option<&BufferHealthMetrics>,
    ) -> usize {
        let read = self.pop_samples(output);
        if let Some(m) = metrics {
            m.record_consumed(read);
        }

        if read < output.len() {
            output[read..].fill(0.0);
            if let Some(m) = metrics {
                m.record_underrun(output.len() - read);
            }
        }
        read
    }

    #[inline]
    pub fn occupied_len(&self) -> usize {
        self.inner.occupied_len()
    }

    #[inline]
    pub fn vacant_len(&self) -> usize {
        self.inner.vacant_len()
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.inner.capacity().get()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    #[inline]
    pub fn inner_mut(&mut self) -> &mut HeapCons<f32> {
        &mut self.inner
    }

    #[inline]
    pub fn into_inner(self) -> HeapCons<f32> {
        self.inner
    }
}

/// Ring buffer factory creating the bounded SPSC producer-consumer pair.
pub struct AudioRingBuffer;

impl AudioRingBuffer {
    pub fn create(capacity: usize) -> (AudioProducer, AudioConsumer) {
        let rb = HeapRb::<f32>::new(capacity);
        let (prod, cons) = rb.split();
        (AudioProducer::new(prod), AudioConsumer::new(cons))
    }

    pub fn default_pair() -> (AudioProducer, AudioConsumer) {
        Self::create(DEFAULT_RING_BUFFER_CAPACITY)
    }
}

/// Calculate playback position in milliseconds from consumed samples.
/// Position (ms) = (samples * 1000) / (sample_rate * channels).
#[inline]
pub fn calculate_position_ms(samples_consumed: u64, sample_rate: u32, channels: u16) -> u64 {
    if sample_rate == 0 || channels == 0 {
        return 0;
    }
    (samples_consumed * 1000) / (sample_rate as u64 * channels as u64)
}

/// Managed CPAL audio output stream holding the active audio stream.
#[cfg(feature = "cpal")]
pub struct CpalOutputStream {
    _stream: cpal::Stream,
    config: cpal::StreamConfig,
}

// Safety: Windows WASAPI cpal::Stream handles thread safety internally,
// but carries NotSendSyncAcrossAllPlatforms for portable safety reasons.
// Wrapping it in an Arc or moving it across threads in a Windows context is safe.
#[cfg(feature = "cpal")]
unsafe impl Send for CpalOutputStream {}
#[cfg(feature = "cpal")]
unsafe impl Sync for CpalOutputStream {}

#[cfg(feature = "cpal")]
impl CpalOutputStream {
    pub fn new(stream: cpal::Stream, config: cpal::StreamConfig) -> Self {
        Self {
            _stream: stream,
            config,
        }
    }

    pub fn play(&self) -> Result<(), AudioError> {
        self._stream
            .play()
            .map_err(|e| AudioError::Playback(format!("Failed to play CPAL stream: {e}")))
    }

    pub fn pause(&self) -> Result<(), AudioError> {
        self._stream
            .pause()
            .map_err(|e| AudioError::Playback(format!("Failed to pause CPAL stream: {e}")))
    }

    pub fn sample_rate(&self) -> u32 {
        self.config.sample_rate.0
    }

    pub fn channels(&self) -> u16 {
        self.config.channels
    }
}

#[cfg(not(feature = "cpal"))]
pub struct CpalOutputStream;

#[cfg(not(feature = "cpal"))]
impl CpalOutputStream {
    pub fn play(&self) -> Result<(), AudioError> {
        Err(AudioError::DeviceInit("cpal feature is disabled".into()))
    }

    pub fn pause(&self) -> Result<(), AudioError> {
        Err(AudioError::DeviceInit("cpal feature is disabled".into()))
    }

    pub fn sample_rate(&self) -> u32 {
        44100
    }

    pub fn channels(&self) -> u16 {
        2
    }
}

/// Setup a CPAL audio output stream with underrun silence padding.
#[cfg(feature = "cpal")]
pub fn create_cpal_output_stream(
    mut consumer: AudioConsumer,
    is_playing: Arc<AtomicBool>,
    metrics: Arc<BufferHealthMetrics>,
    preferred_sample_rate: Option<u32>,
    preferred_channels: Option<u16>,
) -> Result<CpalOutputStream, AudioError> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| AudioError::DeviceInit("No audio output device found on host".into()))?;

    let device_name = device
        .name()
        .unwrap_or_else(|_| "Unknown Audio Device".into());
    info!("Initializing CPAL audio output device: {}", device_name);

    let default_config = device
        .default_output_config()
        .map_err(|e| AudioError::DeviceInit(format!("Failed to query default output config: {e}")))?;

    // Attempt to match preferred sample rate and channels if supported
    let mut selected_config = default_config.config();
    if let (Some(target_rate), Some(target_channels)) = (preferred_sample_rate, preferred_channels) {
        if let Ok(supported_ranges) = device.supported_output_configs() {
            for range in supported_ranges {
                if range.channels() == target_channels
                    && range.sample_format() == cpal::SampleFormat::F32
                    && range.min_sample_rate().0 <= target_rate
                    && range.max_sample_rate().0 >= target_rate
                {
                    selected_config = cpal::StreamConfig {
                        channels: target_channels,
                        sample_rate: cpal::SampleRate(target_rate),
                        buffer_size: cpal::BufferSize::Default,
                    };
                    break;
                }
            }
        }
    }

    let err_fn = |err| {
        error!("CPAL audio output stream error: {}", err);
    };

    let stream = device
        .build_output_stream(
            &selected_config,
            move |output: &mut [f32], _: &cpal::OutputCallbackInfo| {
                if !is_playing.load(Ordering::Relaxed) {
                    output.fill(0.0);
                    return;
                }
                consumer.pop_samples_with_padding(output, Some(&metrics));
            },
            err_fn,
            None,
        )
        .map_err(|e| AudioError::Playback(format!("Failed to build CPAL output stream: {e}")))?;

    Ok(CpalOutputStream::new(stream, selected_config))
}

#[cfg(not(feature = "cpal"))]
pub fn create_cpal_output_stream(
    _consumer: AudioConsumer,
    _is_playing: Arc<AtomicBool>,
    _metrics: Arc<BufferHealthMetrics>,
    _preferred_sample_rate: Option<u32>,
    _preferred_channels: Option<u16>,
) -> Result<CpalOutputStream, AudioError> {
    Err(AudioError::DeviceInit("cpal feature is disabled".into()))
}

/// Headless audio sink that drains samples from the consumer without audio hardware.
pub struct HeadlessAudioSink {
    metrics: Arc<BufferHealthMetrics>,
}

impl HeadlessAudioSink {
    pub fn new(metrics: Arc<BufferHealthMetrics>) -> Self {
        Self { metrics }
    }

    /// Drains up to `max_samples` from the consumer.
    pub fn drain_samples(&self, consumer: &mut AudioConsumer, max_samples: usize) -> usize {
        let mut buffer = [0.0f32; 1024];
        let mut total_drained = 0;
        while total_drained < max_samples {
            let to_read = (max_samples - total_drained).min(buffer.len());
            let read = consumer.pop_samples(&mut buffer[..to_read]);
            if read == 0 {
                break;
            }
            self.metrics.record_consumed(read);
            total_drained += read;
        }
        total_drained
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_ring_buffer_basic_push_pop() {
        let (mut prod, mut cons) = AudioRingBuffer::create(1024);
        assert_eq!(prod.capacity(), 1024);
        assert_eq!(cons.capacity(), 1024);
        assert!(prod.is_empty());

        let input = [1.0f32, 2.0, 3.0, 4.0, 5.0];
        let written = prod.push_samples(&input);
        assert_eq!(written, 5);
        assert_eq!(prod.occupied_len(), 5);
        assert_eq!(cons.occupied_len(), 5);

        let mut output = [0.0f32; 5];
        let read = cons.pop_samples(&mut output);
        assert_eq!(read, 5);
        assert_eq!(output, input);
        assert!(cons.is_empty());
    }

    #[test]
    fn test_underrun_silence_padding() {
        let (mut prod, mut cons) = AudioRingBuffer::create(1024);
        let metrics = BufferHealthMetrics::new();

        // Push 3 valid samples
        prod.push_samples(&[0.5f32, 0.75, 1.0]);

        // Request 8 samples in output buffer pre-filled with 99.0
        let mut output = [99.0f32; 8];
        let read = cons.pop_samples_with_padding(&mut output, Some(&metrics));

        assert_eq!(read, 3);
        assert_eq!(output[0], 0.5);
        assert_eq!(output[1], 0.75);
        assert_eq!(output[2], 1.0);
        // Remainder must be zeroed out
        for sample in &output[3..] {
            assert_eq!(*sample, 0.0);
        }

        assert_eq!(metrics.total_consumed(), 3);
        assert_eq!(metrics.underrun_count(), 1);
        assert_eq!(metrics.underrun_samples(), 5);
    }

    #[test]
    fn test_position_ms_calculation() {
        // 44.1 kHz stereo: 88,200 samples = 1000 ms
        assert_eq!(calculate_position_ms(88_200, 44_100, 2), 1000);
        // 48.0 kHz stereo: 96,000 samples = 1000 ms
        assert_eq!(calculate_position_ms(96_000, 48_000, 2), 1000);
        // 0 samples = 0 ms
        assert_eq!(calculate_position_ms(0, 44_100, 2), 0);
        // Invalid rates = 0 ms
        assert_eq!(calculate_position_ms(1000, 0, 2), 0);
        assert_eq!(calculate_position_ms(1000, 44_100, 0), 0);
    }

    #[test]
    fn test_ring_buffer_concurrency() {
        let (mut prod, mut cons) = AudioRingBuffer::create(4096);
        const TOTAL_SAMPLES: usize = 50_000;

        let prod_handle = thread::spawn(move || {
            let chunk = [1.0f32; 256];
            let mut pushed = 0;
            while pushed < TOTAL_SAMPLES {
                let to_push = (TOTAL_SAMPLES - pushed).min(chunk.len());
                let n = prod.push_samples(&chunk[..to_push]);
                pushed += n;
                if n == 0 {
                    thread::yield_now();
                }
            }
        });

        let cons_handle = thread::spawn(move || {
            let mut buf = [0.0f32; 128];
            let mut received = 0;
            while received < TOTAL_SAMPLES {
                let n = cons.pop_samples(&mut buf);
                received += n;
                if n == 0 {
                    thread::yield_now();
                }
            }
            received
        });

        prod_handle.join().unwrap();
        let total = cons_handle.join().unwrap();
        assert_eq!(total, TOTAL_SAMPLES);
    }

    #[test]
    fn test_ring_buffer_full_behavior() {
        let (mut prod, _cons) = AudioRingBuffer::create(64);
        let data = [1.0f32; 64];
        let written = prod.push_samples(&data);
        assert_eq!(written, 64);
        assert!(prod.is_full());

        // Further writes return 0 without panic
        let extra = [2.0f32; 10];
        let extra_written = prod.push_samples(&extra);
        assert_eq!(extra_written, 0);
    }
}
