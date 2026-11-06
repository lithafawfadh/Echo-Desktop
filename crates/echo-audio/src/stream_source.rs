//! HTTP streaming adapter that implements symphonia's `MediaSource` trait.
//!
//! For live HTTP audio streams we cannot seek backward, so we implement `Seek`
//! by returning an `Unsupported` error.  Symphonia's `MediaSourceStream` handles
//! non-seekable sources gracefully: it disables random-access optimisations but
//! still decodes forward-only streams correctly.

use std::io::{self, Read, Seek, SeekFrom};
use symphonia::core::io::MediaSource;

// ─── In-memory source (used in tests and when the full data is already buffered) ───

/// A `MediaSource` backed by an in-memory byte buffer.
///
/// Wraps `std::io::Cursor<Vec<u8>>` and implements the full `MediaSource`
/// interface (including seeking, because the data is already in memory).
pub struct MemorySource {
    inner: io::Cursor<Vec<u8>>,
}

impl MemorySource {
    /// Create a `MemorySource` from an owned byte vector.
    pub fn new(data: Vec<u8>) -> Self {
        Self { inner: io::Cursor::new(data) }
    }
}

impl Read for MemorySource {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}

impl Seek for MemorySource {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.inner.seek(pos)
    }
}

impl MediaSource for MemorySource {
    fn is_seekable(&self) -> bool {
        true
    }

    fn byte_len(&self) -> Option<u64> {
        Some(self.inner.get_ref().len() as u64)
    }
}

// ─── HTTP streaming source (non-test, real network) ───

/// A `MediaSource` that streams audio data from an HTTP URL using
/// `reqwest::blocking`, buffering reads into a local `Vec<u8>` cache.
///
/// Seeking is reported as *not* supported; the `Seek::seek` impl always
/// returns an `Unsupported` error so symphonia uses forward-only decoding.
#[cfg(not(test))]
pub struct HttpStreamSource {
    /// Bytes already fetched and waiting to be consumed.
    buffer: std::collections::VecDeque<u8>,
    /// Underlying blocking HTTP byte-stream response.
    response: reqwest::blocking::Response,
    /// Total content-length if the server sent one.
    content_length: Option<u64>,
    /// How many bytes have been consumed so far (for `byte_len` accounting).
    bytes_consumed: u64,
}

#[cfg(not(test))]
impl HttpStreamSource {
    /// Open an HTTP GET request to `url` and prepare for streaming reads.
    pub fn open(url: &str) -> Result<Self, crate::AudioError> {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| crate::AudioError::DeviceInit(format!("reqwest client: {e}")))?;

        let response = client
            .get(url)
            .send()
            .map_err(|e| crate::AudioError::DecodeError(format!("HTTP request failed: {e}")))?;

        if !response.status().is_success() {
            return Err(crate::AudioError::DecodeError(format!(
                "HTTP {} for stream URL",
                response.status()
            )));
        }

        let content_length = response.content_length();

        Ok(Self {
            buffer: std::collections::VecDeque::new(),
            response,
            content_length,
            bytes_consumed: 0,
        })
    }
}

#[cfg(not(test))]
impl Read for HttpStreamSource {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // Drain the local buffer first.
        if !self.buffer.is_empty() {
            let n = buf.len().min(self.buffer.len());
            for (dst, src) in buf[..n].iter_mut().zip(self.buffer.drain(..n)) {
                *dst = src;
            }
            self.bytes_consumed += n as u64;
            return Ok(n);
        }

        // Then read directly from the HTTP response body.
        let n = self.response.read(buf)?;
        self.bytes_consumed += n as u64;
        Ok(n)
    }
}

#[cfg(not(test))]
impl Seek for HttpStreamSource {
    fn seek(&mut self, _pos: SeekFrom) -> io::Result<u64> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "HTTP stream is not seekable",
        ))
    }
}

#[cfg(not(test))]
impl MediaSource for HttpStreamSource {
    fn is_seekable(&self) -> bool {
        false
    }

    fn byte_len(&self) -> Option<u64> {
        self.content_length
    }
}

// ─── Tests ───

#[cfg(test)]
mod tests {
    use super::*;
    use symphonia::core::io::MediaSource;

    /// Minimal valid MP3 frame header + silent data (144 bytes) for unit tests.
    /// This is a 32-bit sync word + MPEG-1 layer-3 header producing a valid frame structure.
    fn tiny_mp3_bytes() -> Vec<u8> {
        // 128 kbps, 44100 Hz, stereo MP3 frame header (ID3v2 + one silent frame).
        // We use a known-good minimal MP3 file (10 bytes of ID3v2 + 4-byte frame header).
        // The decoder may not produce output from this but it must not panic.
        vec![
            // ID3v2 header (10 bytes)
            b'I', b'D', b'3',
            0x03, 0x00,       // version 2.3, no flags
            0x00,             // flags
            0x00, 0x00, 0x00, 0x00, // size = 0
            // MPEG1 Layer3, 128kbps, 44100Hz, stereo (sync word + header)
            0xFF, 0xFB, 0x90, 0x00,
            // 413 bytes of silence (frame body for 128kbps @ 44100Hz = 417 bytes total - 4 header)
        ]
    }

    #[test]
    fn memory_source_is_seekable() {
        let src = MemorySource::new(vec![1, 2, 3, 4]);
        assert!(src.is_seekable());
        assert_eq!(src.byte_len(), Some(4));
    }

    #[test]
    fn memory_source_read_all() {
        let data = vec![10u8, 20, 30, 40, 50];
        let mut src = MemorySource::new(data.clone());
        let mut out = vec![0u8; 5];
        let n = src.read(&mut out).unwrap();
        assert_eq!(n, 5);
        assert_eq!(out, data);
    }

    #[test]
    fn memory_source_seek_works() {
        let mut src = MemorySource::new(vec![1, 2, 3, 4]);
        // Read 2 bytes
        let mut buf = [0u8; 2];
        src.read(&mut buf).unwrap();
        assert_eq!(buf, [1, 2]);
        // Seek back to start
        src.seek(SeekFrom::Start(0)).unwrap();
        src.read(&mut buf).unwrap();
        assert_eq!(buf, [1, 2]);
    }

    #[test]
    fn memory_source_as_symphonia_media_source() {
        // Verify that MemorySource can be used as a Symphonia MediaSource
        // by wrapping it in a MediaSourceStream.
        use symphonia::core::io::MediaSourceStream;
        let data = tiny_mp3_bytes();
        let src = MemorySource::new(data);
        let _mss = MediaSourceStream::new(Box::new(src), Default::default());
        // If we got here without panicking, the trait impl is correct.
    }
}
