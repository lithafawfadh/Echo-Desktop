use std::collections::HashMap;
use std::fmt;
use serde::{Deserialize, Serialize};

/// Audio format representation for stream indicators
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AudioCodec {
    Opus,
    Aac,
    Flac,
    Alac,
    Wav,
    Mp3,
    Unknown,
}

impl AudioCodec {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Opus => "Opus",
            Self::Aac => "AAC",
            Self::Flac => "FLAC",
            Self::Alac => "ALAC",
            Self::Wav => "WAV",
            Self::Mp3 => "MP3",
            Self::Unknown => "Unknown",
        }
    }
}

impl fmt::Display for AudioCodec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Detailed audio fidelity classification tier
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum AudioFidelity {
    /// Lossy standard stream (e.g. YouTube Music AAC 128kbps)
    LossyStandard = 1,
    /// Lossy high-quality stream (e.g. YouTube Music Opus 160kbps)
    LossyHigh = 2,
    /// Lossy premium stream (e.g. 320kbs-extension MP3/AAC)
    Lossy320 = 3,
    /// Lossless standard CD quality (16-bit / 44.1kHz FLAC)
    LosslessCD = 4,
    /// Lossless Studio Master (24-bit / 48kHz, 96kHz, or 192kHz FLAC)
    LosslessHiRes = 5,
}

impl AudioFidelity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::LossyStandard => "LossyStandard",
            Self::LossyHigh => "LossyHigh",
            Self::Lossy320 => "Lossy320",
            Self::LosslessCD => "LosslessCD",
            Self::LosslessHiRes => "LosslessHiRes",
        }
    }

    /// Returns true if this fidelity tier is CD quality or higher lossless
    pub fn is_lossless(&self) -> bool {
        *self >= Self::LosslessCD
    }
}

impl fmt::Display for AudioFidelity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Metadata and stream specs for an audio track
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioStream {
    pub url: String,
    pub codec: AudioCodec,
    pub bitrate_kbps: u32,
    pub sample_rate_hz: u32,
    pub is_lossless: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bit_depth: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fidelity: Option<AudioFidelity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_length_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub http_headers: HashMap<String, String>,
}

impl AudioStream {
    /// Constructs a basic audio stream without extension metadata
    pub fn new(
        url: impl Into<String>,
        codec: AudioCodec,
        bitrate_kbps: u32,
        sample_rate_hz: u32,
        is_lossless: bool,
    ) -> Self {
        Self {
            url: url.into(),
            codec,
            bitrate_kbps,
            sample_rate_hz,
            is_lossless,
            bit_depth: None,
            fidelity: None,
            content_length_bytes: None,
            http_headers: HashMap::new(),
        }
    }
}

/// Normalized Track entity across YouTube Music and Lossless Extensions
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub duration_seconds: u32,
    pub thumbnail_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub isrc: Option<String>,
}

impl Track {
    /// Constructs a track without ISRC
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        artist: impl Into<String>,
        album: Option<String>,
        duration_seconds: u32,
        thumbnail_url: Option<String>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            artist: artist.into(),
            album,
            duration_seconds,
            thumbnail_url,
            isrc: None,
        }
    }
}

/// Single synchronized lyric line with millisecond timestamp
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LyricLine {
    pub timestamp_ms: u64,
    pub text: String,
}

/// Full lyrics document (plain text and/or timestamped lines)
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lyrics {
    pub plain: Option<String>,
    pub synced: Vec<LyricLine>,
}

impl Lyrics {
    /// Binary search for current active lyric line at given playback timestamp
    pub fn line_at(&self, position_ms: u64) -> Option<&LyricLine> {
        if self.synced.is_empty() {
            return None;
        }

        // Find the latest line with timestamp <= position_ms
        match self.synced.binary_search_by_key(&position_ms, |l| l.timestamp_ms) {
            Ok(idx) => Some(&self.synced[idx]),
            Err(0) => None, // Before the first lyric line
            Err(idx) => Some(&self.synced[idx - 1]),
        }
    }
}

/// Playback status states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlaybackState {
    Stopped,
    Buffering,
    Playing,
    Paused,
}

/// Progress update event sent to UI
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaybackProgress {
    pub position_ms: u64,
    pub duration_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_lyrics_line_at() {
        let lyrics = Lyrics {
            plain: None,
            synced: vec![
                LyricLine { timestamp_ms: 1000, text: "Line 1".into() },
                LyricLine { timestamp_ms: 3000, text: "Line 2".into() },
                LyricLine { timestamp_ms: 5000, text: "Line 3".into() },
            ],
        };

        assert_eq!(lyrics.line_at(500), None);
        assert_eq!(lyrics.line_at(1000).unwrap().text, "Line 1");
        assert_eq!(lyrics.line_at(2500).unwrap().text, "Line 1");
        assert_eq!(lyrics.line_at(3000).unwrap().text, "Line 2");
        assert_eq!(lyrics.line_at(4999).unwrap().text, "Line 2");
        assert_eq!(lyrics.line_at(7000).unwrap().text, "Line 3");
    }

    #[test]
    fn test_audio_fidelity_discriminants_and_ordering() {
        assert_eq!(AudioFidelity::LossyStandard as u8, 1);
        assert_eq!(AudioFidelity::LossyHigh as u8, 2);
        assert_eq!(AudioFidelity::Lossy320 as u8, 3);
        assert_eq!(AudioFidelity::LosslessCD as u8, 4);
        assert_eq!(AudioFidelity::LosslessHiRes as u8, 5);

        assert!(AudioFidelity::LossyStandard < AudioFidelity::LossyHigh);
        assert!(AudioFidelity::LossyHigh < AudioFidelity::Lossy320);
        assert!(AudioFidelity::Lossy320 < AudioFidelity::LosslessCD);
        assert!(AudioFidelity::LosslessCD < AudioFidelity::LosslessHiRes);

        assert!(!AudioFidelity::LossyStandard.is_lossless());
        assert!(!AudioFidelity::LossyHigh.is_lossless());
        assert!(!AudioFidelity::Lossy320.is_lossless());
        assert!(AudioFidelity::LosslessCD.is_lossless());
        assert!(AudioFidelity::LosslessHiRes.is_lossless());

        let mut set = HashSet::new();
        set.insert(AudioFidelity::LosslessHiRes);
        assert!(set.contains(&AudioFidelity::LosslessHiRes));
    }

    #[test]
    fn test_audio_fidelity_serde_roundtrip() {
        let variants = [
            (AudioFidelity::LossyStandard, "\"LossyStandard\""),
            (AudioFidelity::LossyHigh, "\"LossyHigh\""),
            (AudioFidelity::Lossy320, "\"Lossy320\""),
            (AudioFidelity::LosslessCD, "\"LosslessCD\""),
            (AudioFidelity::LosslessHiRes, "\"LosslessHiRes\""),
        ];

        for (variant, expected_json) in variants {
            let serialized = serde_json::to_string(&variant).unwrap();
            assert_eq!(serialized, expected_json);
            let deserialized: AudioFidelity = serde_json::from_str(&serialized).unwrap();
            assert_eq!(deserialized, variant);
        }
    }

    #[test]
    fn test_audio_codec_extended_variants() {
        let mut set = HashSet::new();
        set.insert(AudioCodec::Alac);
        set.insert(AudioCodec::Wav);
        assert_eq!(set.len(), 2);

        let alac_json = serde_json::to_string(&AudioCodec::Alac).unwrap();
        assert_eq!(alac_json, "\"Alac\"");
        let wav_json = serde_json::to_string(&AudioCodec::Wav).unwrap();
        assert_eq!(wav_json, "\"Wav\"");

        assert_eq!(AudioCodec::Alac.as_str(), "ALAC");
        assert_eq!(AudioCodec::Wav.as_str(), "WAV");
    }

    #[test]
    fn test_audio_stream_backward_compatible_deserialization() {
        // Legacy payload without new fields
        let legacy_json = r#"{
            "url": "https://cdn.example.com/stream.mp4",
            "codec": "Aac",
            "bitrate_kbps": 128,
            "sample_rate_hz": 44100,
            "is_lossless": false
        }"#;

        let stream: AudioStream = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(stream.url, "https://cdn.example.com/stream.mp4");
        assert_eq!(stream.codec, AudioCodec::Aac);
        assert_eq!(stream.bit_depth, None);
        assert_eq!(stream.fidelity, None);
        assert_eq!(stream.content_length_bytes, None);
        assert!(stream.http_headers.is_empty());
    }

    #[test]
    fn test_audio_stream_null_fields_deserialization() {
        let json_with_nulls = r#"{
            "url": "https://cdn.example.com/stream.mp4",
            "codec": "Aac",
            "bitrate_kbps": 128,
            "sample_rate_hz": 44100,
            "is_lossless": false,
            "bit_depth": null,
            "fidelity": null,
            "content_length_bytes": null
        }"#;

        let stream: AudioStream = serde_json::from_str(json_with_nulls).unwrap();
        assert_eq!(stream.bit_depth, None);
        assert_eq!(stream.fidelity, None);
        assert_eq!(stream.content_length_bytes, None);
    }

    #[test]
    fn test_audio_stream_serialization_skips_none() {
        let stream = AudioStream::new(
            "https://cdn.example.com/stream.mp4",
            AudioCodec::Aac,
            128,
            44100,
            false,
        );

        let json = serde_json::to_string(&stream).unwrap();
        assert!(!json.contains("bit_depth"));
        assert!(!json.contains("fidelity"));
        assert!(!json.contains("content_length_bytes"));
        assert!(!json.contains("http_headers"));
    }

    #[test]
    fn test_audio_stream_full_roundtrip() {
        let mut headers = HashMap::new();
        headers.insert("User-Agent".to_string(), "EchoDesktop/1.0".to_string());
        headers.insert("Accept-Ranges".to_string(), "bytes".to_string());

        let stream = AudioStream {
            url: "https://cdn.lossless.echomusic.internal/stream.flac".to_string(),
            codec: AudioCodec::Flac,
            bitrate_kbps: 4608,
            sample_rate_hz: 96000,
            is_lossless: true,
            bit_depth: Some(24),
            fidelity: Some(AudioFidelity::LosslessHiRes),
            content_length_bytes: Some(128_450_120),
            http_headers: headers,
        };

        let json = serde_json::to_string(&stream).unwrap();
        assert!(json.contains("\"bit_depth\":24"));
        assert!(json.contains("\"fidelity\":\"LosslessHiRes\""));
        assert!(json.contains("\"content_length_bytes\":128450120"));
        assert!(json.contains("\"http_headers\":{"));

        let deserialized: AudioStream = serde_json::from_str(&json).unwrap();
        assert_eq!(stream, deserialized);
    }

    #[test]
    fn test_track_backward_compatibility() {
        let legacy_json = r#"{
            "id": "trk_001",
            "title": "Solar Flare",
            "artist": "Echo Labs",
            "album": "Cosmic",
            "duration_seconds": 240,
            "thumbnail_url": null
        }"#;

        let track: Track = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(track.id, "trk_001");
        assert_eq!(track.isrc, None);

        let serialized = serde_json::to_string(&track).unwrap();
        assert!(!serialized.contains("isrc"));

        let with_isrc = Track {
            id: "trk_002".into(),
            title: "Get Lucky".into(),
            artist: "Daft Punk".into(),
            album: Some("Random Access Memories".into()),
            duration_seconds: 369,
            thumbnail_url: None,
            isrc: Some("USQX91300105".into()),
        };

        let json_isrc = serde_json::to_string(&with_isrc).unwrap();
        assert!(json_isrc.contains("\"isrc\":\"USQX91300105\""));

        let roundtrip: Track = serde_json::from_str(&json_isrc).unwrap();
        assert_eq!(with_isrc, roundtrip);
    }
}
