use serde::{Deserialize, Serialize};

/// Audio format representation for stream indicators
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioCodec {
    Opus,
    Aac,
    Flac,
    Mp3,
    Unknown,
}

/// Metadata and stream specs for an audio track
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioStream {
    pub url: String,
    pub codec: AudioCodec,
    pub bitrate_kbps: u32,
    pub sample_rate_hz: u32,
    pub is_lossless: bool,
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
}
