use echo_core::LyricLine;
#[cfg(feature = "remote")]
use echo_core::Lyrics;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum LyricsError {
    #[cfg(feature = "remote")]
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("No lyrics found")]
    NotFound,
    #[error("Parse error")]
    ParseError,
}

#[cfg(feature = "remote")]
#[derive(Debug, serde::Deserialize)]
struct LrcLibResponse {
    #[serde(rename = "plainLyrics")]
    plain_lyrics: Option<String>,
    #[serde(rename = "syncedLyrics")]
    synced_lyrics: Option<String>,
}

/// Parses an LRC formatted string ([mm:ss.xx] or [mm:ss.xxx]) into structured lines
pub fn parse_lrc(lrc_text: &str) -> Vec<LyricLine> {
    let mut lines = Vec::new();

    for line in lrc_text.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('[') {
            continue;
        }

        if let Some(close_bracket) = trimmed.find(']') {
            let tag = &trimmed[1..close_bracket];
            let text = trimmed[close_bracket + 1..].trim().to_string();

            if let Some(ms) = parse_timestamp(tag) {
                lines.push(LyricLine {
                    timestamp_ms: ms,
                    text,
                });
            }
        }
    }

    lines.sort_by_key(|l| l.timestamp_ms);
    lines
}

fn parse_timestamp(tag: &str) -> Option<u64> {
    let mut parts = tag.split(':');
    let minutes_str = parts.next()?;
    let seconds_str = parts.next()?;

    let minutes: u64 = minutes_str.parse().ok()?;
    let mut sec_parts = seconds_str.split('.');
    let seconds: u64 = sec_parts.next()?.parse().ok()?;

    let frac_str = sec_parts.next().unwrap_or("0");
    let ms: u64 = match frac_str.len() {
        1 => frac_str.parse::<u64>().ok()? * 100,
        2 => frac_str.parse::<u64>().ok()? * 10,
        3 => frac_str.parse::<u64>().ok()?,
        _ => frac_str[..3].parse::<u64>().ok()?,
    };

    Some(minutes * 60 * 1000 + seconds * 1000 + ms)
}

#[cfg(feature = "remote")]
pub struct LrcLibClient {
    client: reqwest::Client,
    base_url: String,
}

#[cfg(feature = "remote")]
impl Default for LrcLibClient {
    fn default() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("EchoDesktop/0.1.0 (https://github.com/EchoMusicApp)")
                .build()
                .unwrap_or_default(),
            base_url: "https://lrclib.net/api".to_string(),
        }
    }
}

#[cfg(feature = "remote")]
impl LrcLibClient {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn fetch_lyrics(
        &self,
        track_name: &str,
        artist_name: &str,
        duration_seconds: Option<u32>,
    ) -> Result<Lyrics, LyricsError> {
        let url = format!("{}/get", self.base_url);
        let mut request = self
            .client
            .get(&url)
            .query(&[("track_name", track_name), ("artist_name", artist_name)]);

        let dur_str;
        if let Some(d) = duration_seconds {
            dur_str = d.to_string();
            request = request.query(&[("duration", &dur_str)]);
        }

        let resp = request.send().await?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(LyricsError::NotFound);
        }

        let payload: LrcLibResponse = resp.json().await?;

        let synced = if let Some(synced_raw) = payload.synced_lyrics {
            parse_lrc(&synced_raw)
        } else {
            Vec::new()
        };

        Ok(Lyrics {
            plain: payload.plain_lyrics,
            synced,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_lrc_standard() {
        let lrc = r#"
[00:12.00] Line 1
[00:15.50] Line 2
[01:02.345] Line 3 with more time
"#;
        let lines = parse_lrc(lrc);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].timestamp_ms, 12000);
        assert_eq!(lines[0].text, "Line 1");
        assert_eq!(lines[1].timestamp_ms, 15500);
        assert_eq!(lines[1].text, "Line 2");
        assert_eq!(lines[2].timestamp_ms, 62345);
        assert_eq!(lines[2].text, "Line 3 with more time");
    }

    #[test]
    fn test_parse_lrc_unsorted() {
        let lrc = r#"
[00:20.00] Second
[00:10.00] First
"#;
        let lines = parse_lrc(lrc);
        assert_eq!(lines[0].timestamp_ms, 10000);
        assert_eq!(lines[0].text, "First");
        assert_eq!(lines[1].timestamp_ms, 20000);
        assert_eq!(lines[1].text, "Second");
    }
}
