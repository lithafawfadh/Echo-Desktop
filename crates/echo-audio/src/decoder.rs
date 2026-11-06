//! Symphonia 0.5 ISO-MP4 AAC audio decoding pipeline and ChannelStreamReader adapter.

use crate::AudioError;
use bytes::Bytes;
use ringbuf::traits::Producer;
use std::io::{self, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{Decoder, DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions, ReadOnlySource};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use tokio::sync::mpsc::Receiver;
use tracing::warn;

/// Thread-safe adapter bridging Tokio async byte streams to synchronous `std::io::Read`.
pub struct ChannelStreamReader {
    receiver: Mutex<Receiver<Bytes>>,
    current_chunk: Option<Bytes>,
}

impl ChannelStreamReader {
    pub fn new(receiver: Receiver<Bytes>) -> Self {
        Self {
            receiver: Mutex::new(receiver),
            current_chunk: None,
        }
    }
}

impl Read for ChannelStreamReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        loop {
            // 1. Consume from existing chunk if available
            if let Some(chunk) = &mut self.current_chunk {
                if !chunk.is_empty() {
                    let to_copy = std::cmp::min(buf.len(), chunk.len());
                    buf[..to_copy].copy_from_slice(&chunk[..to_copy]);
                    *chunk = chunk.slice(to_copy..);
                    if chunk.is_empty() {
                        self.current_chunk = None;
                    }
                    return Ok(to_copy);
                }
            }

            // 2. Block until next chunk arrives or channel is closed
            let next = {
                let mut rx = self.receiver.lock().map_err(|_| {
                    io::Error::new(io::ErrorKind::Other, "ChannelStreamReader mutex poisoned")
                })?;
                rx.blocking_recv()
            };

            match next {
                Some(chunk) => {
                    if !chunk.is_empty() {
                        self.current_chunk = Some(chunk);
                    }
                }
                None => {
                    // Stream closed / EOF
                    return Ok(0);
                }
            }
        }
    }
}

/// Statistics collected during decoding.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DecodeStats {
    pub packets_decoded: usize,
    pub samples_decoded: usize,
    pub duration_ms: u64,
}

/// Symphonia audio demuxer and decoder pipeline.
pub struct StreamDecoder {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
    sample_rate: u32,
    channels: usize,
    sample_buf: Option<SampleBuffer<f32>>,
    total_frames_decoded: u64,
}

impl StreamDecoder {
    /// Creates a new decoder from an async channel receiver configured with an optional format hint.
    pub fn new(reader: ChannelStreamReader) -> Result<Self, AudioError> {
        Self::new_with_hint(reader, None)
    }

    /// Creates a new decoder from an async channel receiver with an explicit format hint.
    pub fn new_with_hint(reader: ChannelStreamReader, hint: Option<Hint>) -> Result<Self, AudioError> {
        let read_only = ReadOnlySource::new(reader);
        let mss_opts = MediaSourceStreamOptions {
            buffer_len: 128 * 1024, // 128 KB buffer cache
        };
        let mss = MediaSourceStream::new(Box::new(read_only), mss_opts);

        Self::from_media_source_stream(mss, hint)
    }

    /// Creates a new decoder from any `std::io::Read + Send + 'static` source.
    pub fn from_reader<R: Read + Send + 'static>(
        reader: R,
        hint: Option<Hint>,
    ) -> Result<Self, AudioError> {
        struct SyncWrapper<R>(Mutex<R>);
        impl<R: Read> Read for SyncWrapper<R> {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                self.0
                    .lock()
                    .map_err(|_| io::Error::new(io::ErrorKind::Other, "SyncWrapper lock poisoned"))?
                    .read(buf)
            }
        }

        let sync_reader = SyncWrapper(Mutex::new(reader));
        let read_only = ReadOnlySource::new(sync_reader);
        let mss_opts = MediaSourceStreamOptions {
            buffer_len: 128 * 1024,
        };
        let mss = MediaSourceStream::new(Box::new(read_only), mss_opts);

        Self::from_media_source_stream(mss, hint)
    }

    /// Initializes demuxer and codec decoder from a configured `MediaSourceStream`.
    pub fn from_media_source_stream(
        mss: MediaSourceStream,
        hint: Option<Hint>,
    ) -> Result<Self, AudioError> {
        let hint = hint.unwrap_or_default();
        let probed = symphonia::default::get_probe()
            .format(
                &hint,
                mss,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .map_err(|e| AudioError::DecodeError(format!("Format probe failed: {e}")))?;

        let format = probed.format;

        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or_else(|| {
                AudioError::DecodeError("No valid audio track found in stream".to_string())
            })?;

        let track_id = track.id;
        let sample_rate = track.codec_params.sample_rate.unwrap_or(44100);
        let channels = track
            .codec_params
            .channels
            .map(|c| c.count())
            .or_else(|| track.codec_params.channel_layout.map(|l| l.into_channels().count()))
            .unwrap_or(2);

        let decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions { verify: true })
            .map_err(|e| {
                AudioError::DecodeError(format!("Failed to initialize codec decoder: {e}"))
            })?;

        Ok(Self {
            format,
            decoder,
            track_id,
            sample_rate,
            channels,
            sample_buf: None,
            total_frames_decoded: 0,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    pub fn track_id(&self) -> u32 {
        self.track_id
    }

    pub fn total_frames_decoded(&self) -> u64 {
        self.total_frames_decoded
    }

    /// Decodes a single packet, returning interleaved f32 PCM samples, or `None` on EOF.
    pub fn decode_next(&mut self) -> Result<Option<&[f32]>, AudioError> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(packet) => packet,
                Err(SymphoniaError::IoError(ref err))
                    if err.kind() == io::ErrorKind::UnexpectedEof =>
                {
                    return Ok(None);
                }
                Err(SymphoniaError::ResetRequired) => {
                    self.decoder.reset();
                    continue;
                }
                Err(err) => {
                    return Err(AudioError::DecodeError(format!("Demuxer error: {err}")));
                }
            };

            if packet.track_id() != self.track_id {
                continue;
            }

            match self.decoder.decode(&packet) {
                Ok(decoded) => {
                    let spec = *decoded.spec();
                    let duration = decoded.capacity() as u64;
                    let frames = decoded.frames() as u64;

                    if self.sample_buf.as_ref().map_or(true, |b| {
                        b.capacity() < decoded.frames() * spec.channels.count()
                    }) {
                        self.sample_buf = Some(SampleBuffer::<f32>::new(duration, spec));
                    }

                    let buf = self.sample_buf.as_mut().unwrap();
                    buf.copy_interleaved_ref(decoded);
                    self.total_frames_decoded += frames;

                    return Ok(Some(buf.samples()));
                }
                Err(SymphoniaError::DecodeError(msg)) => {
                    warn!("Non-fatal decode warning: {msg}, skipping packet");
                    continue;
                }
                Err(SymphoniaError::ResetRequired) => {
                    self.decoder.reset();
                    continue;
                }
                Err(err) => {
                    return Err(AudioError::DecodeError(format!("Fatal decode error: {err}")));
                }
            }
        }
    }

    /// Streams decoded audio into the ring buffer with backpressure.
    pub fn decode_into_ring_buffer(
        &mut self,
        producer: &mut impl Producer<Item = f32>,
        max_packets: Option<usize>,
        stop_signal: Option<&AtomicBool>,
    ) -> Result<DecodeStats, AudioError> {
        let mut stats = DecodeStats::default();

        while max_packets.map_or(true, |max| stats.packets_decoded < max) {
            if let Some(stop) = stop_signal {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
            }

            match self.decode_next()? {
                Some(samples) => {
                    let mut pushed = 0;
                    while pushed < samples.len() {
                        if let Some(stop) = stop_signal {
                            if stop.load(Ordering::Relaxed) {
                                break;
                            }
                        }
                        let n = producer.push_slice(&samples[pushed..]);
                        pushed += n;
                        if pushed < samples.len() {
                            std::thread::sleep(Duration::from_millis(5));
                        }
                    }

                    stats.packets_decoded += 1;
                    stats.samples_decoded += samples.len();
                }
                None => {
                    // Stream finished
                    break;
                }
            }
        }

        if self.sample_rate > 0 {
            stats.duration_ms = (self.total_frames_decoded * 1000) / self.sample_rate as u64;
        }
        Ok(stats)
    }

    /// Convenience method to decode into an `AudioProducer`.
    pub fn decode_into_audio_producer(
        &mut self,
        producer: &mut crate::ring_buffer::AudioProducer,
        max_packets: Option<usize>,
        stop_signal: Option<&AtomicBool>,
    ) -> Result<DecodeStats, AudioError> {
        self.decode_into_ring_buffer(producer.inner_mut(), max_packets, stop_signal)
    }
}
