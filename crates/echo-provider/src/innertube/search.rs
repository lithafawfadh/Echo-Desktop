use super::models::InnerTubeSearchResponse;
use crate::error::ProviderError;
use echo_core::Track;
use serde_json::Value;

/// Protobuf token to filter InnerTube search results specifically to Songs.
pub const SONG_FILTER_PARAMS: &str = "EgWKAQIIAWoKEAkQBRAKEAMQBBAF";

/// Parses a duration timestamp string (`"M:SS"` or `"H:MM:SS"`) into total seconds.
/// Returns 0 for missing or unparseable timestamps (e.g. live streams).
pub fn parse_duration(time_str: &str) -> u32 {
    let trimmed = time_str.trim();
    if trimmed.is_empty() {
        return 0;
    }

    let parts: Vec<&str> = trimmed.split(':').collect();
    if parts.is_empty() {
        return 0;
    }

    let mut total_seconds: u32 = 0;
    for part in parts {
        match part.parse::<u32>() {
            Ok(val) => {
                total_seconds = total_seconds.saturating_mul(60).saturating_add(val);
            }
            Err(_) => return 0,
        }
    }

    total_seconds
}

/// Parses an InnerTube search JSON response (represented as `serde_json::Value`) into structured `Track` entities.
pub fn parse_search_response(json: &Value) -> Result<Vec<Track>, ProviderError> {
    let response: InnerTubeSearchResponse = serde_json::from_value(json.clone())?;
    parse_search_response_typed(&response)
}

/// Parses a strongly-typed `InnerTubeSearchResponse` into structured `Track` entities.
pub fn parse_search_response_typed(response: &InnerTubeSearchResponse) -> Result<Vec<Track>, ProviderError> {
    let mut tracks = Vec::new();

    let tabs = response
        .contents
        .as_ref()
        .and_then(|c| c.tabbed_search_results_renderer.as_ref())
        .and_then(|t| t.tabs.as_ref());

    let tabs = match tabs {
        Some(t) if !t.is_empty() => t,
        _ => return Ok(tracks),
    };

    let section_list = tabs[0]
        .tab_renderer
        .as_ref()
        .and_then(|tr| tr.content.as_ref())
        .and_then(|c| c.section_list_renderer.as_ref())
        .and_then(|sl| sl.contents.as_ref());

    let sections = match section_list {
        Some(s) => s,
        None => return Ok(tracks),
    };

    // Find the musicShelfRenderer (typically the first shelf containing songs)
    for section in sections {
        let shelf = match section.music_shelf_renderer.as_ref() {
            Some(sh) => sh,
            None => continue,
        };

        let items = match shelf.contents.as_ref() {
            Some(it) => it,
            None => continue,
        };

        for item in items {
            let renderer = match item.music_responsive_list_item_renderer.as_ref() {
                Some(r) => r,
                None => continue,
            };

            let flex_cols = match renderer.flex_columns.as_ref() {
                Some(cols) if !cols.is_empty() => cols,
                _ => continue,
            };

            // 1. Video ID & Title from FlexColumn 0
            let col0_runs = flex_cols[0]
                .music_responsive_list_item_flex_column_renderer
                .as_ref()
                .and_then(|r| r.text.as_ref())
                .and_then(|t| t.runs.as_ref());

            let (video_id, title) = match col0_runs {
                Some(runs) if !runs.is_empty() => {
                    let vid = runs[0]
                        .navigation_endpoint
                        .as_ref()
                        .and_then(|ne| ne.watch_endpoint.as_ref())
                        .and_then(|we| we.video_id.clone());
                    let tit = runs[0].text.clone();
                    (vid, tit)
                }
                _ => (None, String::new()),
            };

            let id = match video_id {
                Some(id) if !id.is_empty() => id,
                _ => continue, // Video ID is mandatory for a playable track
            };

            // 2. Artist, Album, and Duration from FlexColumn 1
            let mut artists = Vec::new();
            let mut album: Option<String> = None;
            let mut duration_str = String::new();

            if flex_cols.len() > 1 {
                if let Some(col1_runs) = flex_cols[1]
                    .music_responsive_list_item_flex_column_renderer
                    .as_ref()
                    .and_then(|r| r.text.as_ref())
                    .and_then(|t| t.runs.as_ref())
                {
                    for run in col1_runs {
                        let text = run.text.trim();
                        // Ignore bullet separators
                        if text == "•" || text == "\u{2022}" || text.is_empty() {
                            continue;
                        }

                        // Check for album browseId (MPREb_...)
                        if let Some(endpoint) = &run.navigation_endpoint {
                            if let Some(browse) = &endpoint.browse_endpoint {
                                if let Some(bid) = &browse.browse_id {
                                    if bid.starts_with("MPREb_") {
                                        album = Some(text.to_string());
                                        continue;
                                    }
                                }
                            }
                        }

                        // Check for timestamp duration (contains ':')
                        if text.contains(':') && text.chars().all(|c| c.is_ascii_digit() || c == ':') {
                            duration_str = text.to_string();
                            continue;
                        }

                        // Check for artist separator ('&' or ',')
                        if text == "&" || text == "," {
                            continue;
                        }

                        // If not album and not duration, treat as artist
                        if album.is_none() && duration_str.is_empty() {
                            artists.push(text.to_string());
                        }
                    }
                }
            }

            // Fallback duration from fixedColumns if not found in flexColumns
            if duration_str.is_empty() {
                if let Some(fixed_cols) = renderer.fixed_columns.as_ref() {
                    if let Some(first_fixed) = fixed_cols.first() {
                        if let Some(runs) = first_fixed
                            .music_responsive_list_item_fixed_column_renderer
                            .as_ref()
                            .and_then(|r| r.text.as_ref())
                            .and_then(|t| t.runs.as_ref())
                        {
                            if let Some(run) = runs.first() {
                                duration_str = run.text.trim().to_string();
                            }
                        }
                    }
                }
            }

            let artist = if artists.is_empty() {
                "Unknown Artist".to_string()
            } else {
                artists.join(", ")
            };

            let duration_seconds = parse_duration(&duration_str);

            // 3. Thumbnail URL
            let mut thumbnail_url = renderer
                .thumbnail
                .as_ref()
                .and_then(|t| t.music_thumbnail_renderer.as_ref())
                .and_then(|tr| tr.thumbnail.as_ref())
                .and_then(|sub| sub.thumbnails.as_ref())
                .and_then(|th_list| th_list.last())
                .map(|item| item.url.clone());

            // Upgrade thumbnail resolution if standard Google sizing token is present
            if let Some(ref mut url) = thumbnail_url {
                if url.contains("=w") && url.contains("-h") {
                    if let Some(idx) = url.find("=w") {
                        let base = &url[..idx];
                        *url = format!("{base}=w544-h544-l90-rj");
                    }
                }
            }

            tracks.push(Track {
                id,
                title,
                artist,
                album,
                duration_seconds,
                thumbnail_url,
            });
        }
    }

    Ok(tracks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("0:05"), 5);
        assert_eq!(parse_duration("3:51"), 231);
        assert_eq!(parse_duration("4:08"), 248);
        assert_eq!(parse_duration("1:15:20"), 4520);
        assert_eq!(parse_duration(""), 0);
        assert_eq!(parse_duration("LIVE"), 0);
        assert_eq!(parse_duration("invalid"), 0);
    }

    #[test]
    fn test_parse_search_response_empty() {
        let empty_json = json!({
            "contents": {
                "tabbedSearchResultsRenderer": {
                    "tabs": [{
                        "tabRenderer": {
                            "content": {
                                "sectionListRenderer": {
                                    "contents": []
                                }
                            }
                        }
                    }]
                }
            }
        });

        let tracks = parse_search_response(&empty_json).expect("Should parse empty");
        assert!(tracks.is_empty());
    }
}
