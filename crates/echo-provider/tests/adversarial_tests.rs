//! Adversarial Stress Test Suite for `crates/echo-provider`
//!
//! Validates InnerTube search response parsing, duration handling, artist extraction,
//! playability status parsing, stream extraction error handling, and zero unwrap panics.

use echo_provider::cipher::{descramble_signature, transform_stream_url_n, CipherOp};
use echo_provider::error::ProviderError;
use echo_provider::innertube::{
    parse_duration, parse_player_response, parse_search_response, resolve_candidate_to_stream,
    select_itag_140_stream, InnerTubePlayerResponse, RawStreamCandidate,
};
use serde_json::json;

// ============================================================================
// 1. Search Response Parsing with Degraded / Malformed JSON
// ============================================================================

#[test]
fn test_search_missing_tabs_and_empty_contents() {
    // 1. Completely empty object
    let val = json!({});
    let tracks = parse_search_response(&val).expect("Empty JSON should parse to empty vec");
    assert!(tracks.is_empty());

    // 2. contents is empty object
    let val = json!({ "contents": {} });
    let tracks = parse_search_response(&val).expect("Missing tabbedSearchResultsRenderer");
    assert!(tracks.is_empty());

    // 3. tabbedSearchResultsRenderer is empty
    let val = json!({
        "contents": {
            "tabbedSearchResultsRenderer": {}
        }
    });
    let tracks = parse_search_response(&val).expect("Missing tabs");
    assert!(tracks.is_empty());

    // 4. tabs is empty array
    let val = json!({
        "contents": {
            "tabbedSearchResultsRenderer": {
                "tabs": []
            }
        }
    });
    let tracks = parse_search_response(&val).expect("Empty tabs array");
    assert!(tracks.is_empty());

    // 5. tab has null/empty tabRenderer
    let val = json!({
        "contents": {
            "tabbedSearchResultsRenderer": {
                "tabs": [
                    { "tabRenderer": null },
                    {}
                ]
            }
        }
    });
    let tracks = parse_search_response(&val).expect("Tab without tabRenderer");
    assert!(tracks.is_empty());
}

#[test]
fn test_search_missing_music_shelf_renderer() {
    // 1. sectionListRenderer has empty contents
    let val = json!({
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
    let tracks = parse_search_response(&val).expect("Empty section list");
    assert!(tracks.is_empty());

    // 2. sectionListRenderer has items, but none are musicShelfRenderer
    let val = json!({
        "contents": {
            "tabbedSearchResultsRenderer": {
                "tabs": [{
                    "tabRenderer": {
                        "content": {
                            "sectionListRenderer": {
                                "contents": [
                                    { "itemSectionRenderer": { "someField": 123 } },
                                    { "gridRenderer": { "items": [] } }
                                ]
                            }
                        }
                    }
                }]
            }
        }
    });
    let tracks = parse_search_response(&val).expect("Sections without musicShelfRenderer");
    assert!(tracks.is_empty());

    // 3. musicShelfRenderer exists but contents is empty or missing
    let val = json!({
        "contents": {
            "tabbedSearchResultsRenderer": {
                "tabs": [{
                    "tabRenderer": {
                        "content": {
                            "sectionListRenderer": {
                                "contents": [
                                    {
                                        "musicShelfRenderer": {
                                            "title": { "runs": [{ "text": "Songs" }] },
                                            "contents": []
                                        }
                                    }
                                ]
                            }
                        }
                    }
                }]
            }
        }
    });
    let tracks = parse_search_response(&val).expect("musicShelfRenderer with empty contents");
    assert!(tracks.is_empty());
}

#[test]
fn test_search_missing_flex_columns() {
    // 1. Item without musicResponsiveListItemRenderer
    let val = json!({
        "contents": {
            "tabbedSearchResultsRenderer": {
                "tabs": [{
                    "tabRenderer": {
                        "content": {
                            "sectionListRenderer": {
                                "contents": [{
                                    "musicShelfRenderer": {
                                        "contents": [
                                            {},
                                            { "musicResponsiveListItemRenderer": null }
                                        ]
                                    }
                                }]
                            }
                        }
                    }
                }]
            }
        }
    });
    let tracks = parse_search_response(&val).expect("Items without renderer");
    assert!(tracks.is_empty());

    // 2. musicResponsiveListItemRenderer with null or empty flexColumns
    let val = json!({
        "contents": {
            "tabbedSearchResultsRenderer": {
                "tabs": [{
                    "tabRenderer": {
                        "content": {
                            "sectionListRenderer": {
                                "contents": [{
                                    "musicShelfRenderer": {
                                        "contents": [
                                            {
                                                "musicResponsiveListItemRenderer": {
                                                    "flexColumns": null
                                                }
                                            },
                                            {
                                                "musicResponsiveListItemRenderer": {
                                                    "flexColumns": []
                                                }
                                            }
                                        ]
                                    }
                                }]
                            }
                        }
                    }
                }]
            }
        }
    });
    let tracks = parse_search_response(&val).expect("Item with empty flexColumns");
    assert!(tracks.is_empty());
}

#[test]
fn test_search_empty_runs_and_missing_video_id() {
    // 1. Flex column 0 has empty runs
    let val = json!({
        "contents": {
            "tabbedSearchResultsRenderer": {
                "tabs": [{
                    "tabRenderer": {
                        "content": {
                            "sectionListRenderer": {
                                "contents": [{
                                    "musicShelfRenderer": {
                                        "contents": [{
                                            "musicResponsiveListItemRenderer": {
                                                "flexColumns": [{
                                                    "musicResponsiveListItemFlexColumnRenderer": {
                                                        "text": { "runs": [] }
                                                    }
                                                }]
                                            }
                                        }]
                                    }
                                }]
                            }
                        }
                    }
                }]
            }
        }
    });
    let tracks = parse_search_response(&val).expect("Flex column with empty runs");
    assert!(tracks.is_empty());

    // 2. Flex column 0 has runs with missing or empty videoId
    let val = json!({
        "contents": {
            "tabbedSearchResultsRenderer": {
                "tabs": [{
                    "tabRenderer": {
                        "content": {
                            "sectionListRenderer": {
                                "contents": [{
                                    "musicResponsiveListItemRenderer": {
                                        "flexColumns": [{
                                            "musicResponsiveListItemFlexColumnRenderer": {
                                                "text": {
                                                    "runs": [{
                                                        "text": "Song Without ID",
                                                        "navigationEndpoint": null
                                                    }]
                                                }
                                            }
                                        }]
                                    }
                                }, {
                                    "musicResponsiveListItemRenderer": {
                                        "flexColumns": [{
                                            "musicResponsiveListItemFlexColumnRenderer": {
                                                "text": {
                                                    "runs": [{
                                                        "text": "Song With Empty ID",
                                                        "navigationEndpoint": {
                                                            "watchEndpoint": { "videoId": "" }
                                                        }
                                                    }]
                                                }
                                            }
                                        }]
                                    }
                                }]
                            }
                        }
                    }
                }]
            }
        }
    });
    let tracks = parse_search_response(&val).expect("Items with missing videoId should be skipped");
    assert!(tracks.is_empty());
}

#[test]
fn test_search_missing_duration_run() {
    // Song with videoId and title and artist, but no duration in flexColumns or fixedColumns
    let val = json!({
        "contents": {
            "tabbedSearchResultsRenderer": {
                "tabs": [{
                    "tabRenderer": {
                        "content": {
                            "sectionListRenderer": {
                                "contents": [{
                                    "musicShelfRenderer": {
                                        "contents": [{
                                            "musicResponsiveListItemRenderer": {
                                                "flexColumns": [
                                                    {
                                                        "musicResponsiveListItemFlexColumnRenderer": {
                                                            "text": {
                                                                "runs": [{
                                                                    "text": "Endless Song",
                                                                    "navigationEndpoint": {
                                                                        "watchEndpoint": { "videoId": "endless_001" }
                                                                    }
                                                                }]
                                                            }
                                                        }
                                                    },
                                                    {
                                                        "musicResponsiveListItemFlexColumnRenderer": {
                                                            "text": {
                                                                "runs": [{
                                                                    "text": "Unknown Ambient Artist"
                                                                }]
                                                            }
                                                        }
                                                    }
                                                ]
                                            }
                                        }]
                                    }
                                }]
                            }
                        }
                    }
                }]
            }
        }
    });
    let tracks = parse_search_response(&val).expect("Parse song without duration");
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].id, "endless_001");
    assert_eq!(tracks[0].title, "Endless Song");
    assert_eq!(tracks[0].artist, "Unknown Ambient Artist");
    assert_eq!(tracks[0].duration_seconds, 0); // Missing duration yields 0
}

#[test]
fn test_duration_parsing_variations() {
    // Standard durations
    assert_eq!(parse_duration("0:45"), 45);
    assert_eq!(parse_duration("3:30"), 210);

    // Single-part duration
    assert_eq!(parse_duration("45"), 45);
    assert_eq!(parse_duration("0"), 0);

    // 3-part duration (H:MM:SS)
    // 1:02:30 = 1*3600 + 2*60 + 30 = 3750
    assert_eq!(parse_duration("1:02:30"), 3750);
    // 10:00:00 = 36000
    assert_eq!(parse_duration("10:00:00"), 36000);

    // Non-numeric and malformed durations
    assert_eq!(parse_duration("LIVE"), 0);
    assert_eq!(parse_duration("LIVE STREAM"), 0);
    assert_eq!(parse_duration("NaN"), 0);
    assert_eq!(parse_duration("1:xx"), 0);
    assert_eq!(parse_duration("xx:yy"), 0);
    assert_eq!(parse_duration(":::"), 0);
    assert_eq!(parse_duration("-5:00"), 0);
    assert_eq!(parse_duration(""), 0);
    assert_eq!(parse_duration("   "), 0);

    // Overflow protection: very large numeric strings
    assert_eq!(parse_duration("99999999999999999999999:00"), 0);
    // Saturating arithmetic does not panic on u32 limits
    let _ = parse_duration("4294967295:59");
}

#[test]
fn test_search_response_with_3_part_duration() {
    let val = json!({
        "contents": {
            "tabbedSearchResultsRenderer": {
                "tabs": [{
                    "tabRenderer": {
                        "content": {
                            "sectionListRenderer": {
                                "contents": [{
                                    "musicShelfRenderer": {
                                        "contents": [{
                                            "musicResponsiveListItemRenderer": {
                                                "flexColumns": [
                                                    {
                                                        "musicResponsiveListItemFlexColumnRenderer": {
                                                            "text": {
                                                                "runs": [{
                                                                    "text": "Long Symphony",
                                                                    "navigationEndpoint": {
                                                                        "watchEndpoint": { "videoId": "symph_123" }
                                                                    }
                                                                }]
                                                            }
                                                        }
                                                    },
                                                    {
                                                        "musicResponsiveListItemFlexColumnRenderer": {
                                                            "text": {
                                                                "runs": [
                                                                    { "text": "Orchestra" },
                                                                    { "text": " • " },
                                                                    { "text": "1:02:30" }
                                                                ]
                                                            }
                                                        }
                                                    }
                                                ]
                                            }
                                        }]
                                    }
                                }]
                            }
                        }
                    }
                }]
            }
        }
    });

    let tracks = parse_search_response(&val).expect("Parse 3-part duration");
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].id, "symph_123");
    assert_eq!(tracks[0].duration_seconds, 3750);
}

#[test]
fn test_search_response_with_single_part_duration_in_fixed_columns() {
    let val = json!({
        "contents": {
            "tabbedSearchResultsRenderer": {
                "tabs": [{
                    "tabRenderer": {
                        "content": {
                            "sectionListRenderer": {
                                "contents": [{
                                    "musicShelfRenderer": {
                                        "contents": [{
                                            "musicResponsiveListItemRenderer": {
                                                "flexColumns": [
                                                    {
                                                        "musicResponsiveListItemFlexColumnRenderer": {
                                                            "text": {
                                                                "runs": [{
                                                                    "text": "Short Jingle",
                                                                    "navigationEndpoint": {
                                                                        "watchEndpoint": { "videoId": "jingle_45" }
                                                                    }
                                                                }]
                                                            }
                                                        }
                                                    },
                                                    {
                                                        "musicResponsiveListItemFlexColumnRenderer": {
                                                            "text": {
                                                                "runs": [{ "text": "Jingle Band" }]
                                                            }
                                                        }
                                                    }
                                                ],
                                                "fixedColumns": [{
                                                    "musicResponsiveListItemFixedColumnRenderer": {
                                                        "text": {
                                                            "runs": [{ "text": "45" }]
                                                        }
                                                    }
                                                }]
                                            }
                                        }]
                                    }
                                }]
                            }
                        }
                    }
                }]
            }
        }
    });

    let tracks = parse_search_response(&val).expect("Parse fixedColumn single part duration");
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].id, "jingle_45");
    assert_eq!(tracks[0].duration_seconds, 45);
}

#[test]
fn test_search_non_numeric_duration_run_in_search_response() {
    let val = json!({
        "contents": {
            "tabbedSearchResultsRenderer": {
                "tabs": [{
                    "tabRenderer": {
                        "content": {
                            "sectionListRenderer": {
                                "contents": [{
                                    "musicShelfRenderer": {
                                        "contents": [{
                                            "musicResponsiveListItemRenderer": {
                                                "flexColumns": [
                                                    {
                                                        "musicResponsiveListItemFlexColumnRenderer": {
                                                            "text": {
                                                                "runs": [{
                                                                    "text": "Live Stream Track",
                                                                    "navigationEndpoint": {
                                                                        "watchEndpoint": { "videoId": "live_stream_01" }
                                                                    }
                                                                }]
                                                            }
                                                        }
                                                    },
                                                    {
                                                        "musicResponsiveListItemFlexColumnRenderer": {
                                                            "text": {
                                                                "runs": [
                                                                    { "text": "Live Performer" },
                                                                    { "text": " • " },
                                                                    { "text": "LIVE" }
                                                                ]
                                                            }
                                                        }
                                                    }
                                                ]
                                            }
                                        }]
                                    }
                                }]
                            }
                        }
                    }
                }]
            }
        }
    });

    let tracks = parse_search_response(&val).expect("Parse non-numeric duration");
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].duration_seconds, 0); // Non-numeric duration resolves to 0
}

// ============================================================================
// 2. Artist Extraction and Delimiters
// ============================================================================

#[test]
fn test_artist_extraction_delimiters() {
    // 1. Multiple artists with " & " delimiter
    let val_amp = json!({
        "contents": {
            "tabbedSearchResultsRenderer": {
                "tabs": [{
                    "tabRenderer": {
                        "content": {
                            "sectionListRenderer": {
                                "contents": [{
                                    "musicShelfRenderer": {
                                        "contents": [{
                                            "musicResponsiveListItemRenderer": {
                                                "flexColumns": [
                                                    {
                                                        "musicResponsiveListItemFlexColumnRenderer": {
                                                            "text": {
                                                                "runs": [{
                                                                    "text": "Collaboration",
                                                                    "navigationEndpoint": {
                                                                        "watchEndpoint": { "videoId": "collab_amp" }
                                                                    }
                                                                }]
                                                            }
                                                        }
                                                    },
                                                    {
                                                        "musicResponsiveListItemFlexColumnRenderer": {
                                                            "text": {
                                                                "runs": [
                                                                    { "text": "Artist A" },
                                                                    { "text": " & " },
                                                                    { "text": "Artist B" },
                                                                    { "text": " • " },
                                                                    { "text": "3:00" }
                                                                ]
                                                            }
                                                        }
                                                    }
                                                ]
                                            }
                                        }]
                                    }
                                }]
                            }
                        }
                    }
                }]
            }
        }
    });

    let tracks = parse_search_response(&val_amp).expect("Parse & delimiter");
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].artist, "Artist A, Artist B");

    // 2. Multiple artists with ", " delimiter
    let val_comma = json!({
        "contents": {
            "tabbedSearchResultsRenderer": {
                "tabs": [{
                    "tabRenderer": {
                        "content": {
                            "sectionListRenderer": {
                                "contents": [{
                                    "musicShelfRenderer": {
                                        "contents": [{
                                            "musicResponsiveListItemRenderer": {
                                                "flexColumns": [
                                                    {
                                                        "musicResponsiveListItemFlexColumnRenderer": {
                                                            "text": {
                                                                "runs": [{
                                                                    "text": "Trio Song",
                                                                    "navigationEndpoint": {
                                                                        "watchEndpoint": { "videoId": "trio_comma" }
                                                                    }
                                                                }]
                                                            }
                                                        }
                                                    },
                                                    {
                                                        "musicResponsiveListItemFlexColumnRenderer": {
                                                            "text": {
                                                                "runs": [
                                                                    { "text": "First Artist" },
                                                                    { "text": ", " },
                                                                    { "text": "Second Artist" },
                                                                    { "text": ", " },
                                                                    { "text": "Third Artist" },
                                                                    { "text": " • " },
                                                                    { "text": "4:15" }
                                                                ]
                                                            }
                                                        }
                                                    }
                                                ]
                                            }
                                        }]
                                    }
                                }]
                            }
                        }
                    }
                }]
            }
        }
    });

    let tracks = parse_search_response(&val_comma).expect("Parse comma delimiter");
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].artist, "First Artist, Second Artist, Third Artist");
}

#[test]
fn test_unlinked_artists_and_missing_album_browse_id() {
    // 1. Unlinked artist (no navigationEndpoint on artist run)
    let val_unlinked = json!({
        "contents": {
            "tabbedSearchResultsRenderer": {
                "tabs": [{
                    "tabRenderer": {
                        "content": {
                            "sectionListRenderer": {
                                "contents": [{
                                    "musicShelfRenderer": {
                                        "contents": [{
                                            "musicResponsiveListItemRenderer": {
                                                "flexColumns": [
                                                    {
                                                        "musicResponsiveListItemFlexColumnRenderer": {
                                                            "text": {
                                                                "runs": [{
                                                                    "text": "Indie Track",
                                                                    "navigationEndpoint": {
                                                                        "watchEndpoint": { "videoId": "indie_01" }
                                                                    }
                                                                }]
                                                            }
                                                        }
                                                    },
                                                    {
                                                        "musicResponsiveListItemFlexColumnRenderer": {
                                                            "text": {
                                                                "runs": [
                                                                    { "text": "Unlinked Garage Band", "navigationEndpoint": null },
                                                                    { "text": " • " },
                                                                    { "text": "2:50" }
                                                                ]
                                                            }
                                                        }
                                                    }
                                                ]
                                            }
                                        }]
                                    }
                                }]
                            }
                        }
                    }
                }]
            }
        }
    });

    let tracks = parse_search_response(&val_unlinked).expect("Parse unlinked artist");
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].artist, "Unlinked Garage Band");
    assert_eq!(tracks[0].album, None);

    // 2. Album with valid MPREb_ browseId
    let val_album_ok = json!({
        "contents": {
            "tabbedSearchResultsRenderer": {
                "tabs": [{
                    "tabRenderer": {
                        "content": {
                            "sectionListRenderer": {
                                "contents": [{
                                    "musicShelfRenderer": {
                                        "contents": [{
                                            "musicResponsiveListItemRenderer": {
                                                "flexColumns": [
                                                    {
                                                        "musicResponsiveListItemFlexColumnRenderer": {
                                                            "text": {
                                                                "runs": [{
                                                                    "text": "Album Cut",
                                                                    "navigationEndpoint": {
                                                                        "watchEndpoint": { "videoId": "album_track_01" }
                                                                    }
                                                                }]
                                                            }
                                                        }
                                                    },
                                                    {
                                                        "musicResponsiveListItemFlexColumnRenderer": {
                                                            "text": {
                                                                "runs": [
                                                                    { "text": "Main Artist" },
                                                                    { "text": " • " },
                                                                    {
                                                                        "text": "Greatest Hits",
                                                                        "navigationEndpoint": {
                                                                            "browseEndpoint": {
                                                                                "browseId": "MPREb_album_id_999"
                                                                            }
                                                                        }
                                                                    },
                                                                    { "text": " • " },
                                                                    { "text": "3:45" }
                                                                ]
                                                            }
                                                        }
                                                    }
                                                ]
                                            }
                                        }]
                                    }
                                }]
                            }
                        }
                    }
                }]
            }
        }
    });

    let tracks = parse_search_response(&val_album_ok).expect("Parse valid album browseId");
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].artist, "Main Artist");
    assert_eq!(tracks[0].album, Some("Greatest Hits".to_string()));

    // 3. Album without MPREb_ prefix (e.g. general channel browseId like UC...)
    let val_non_mpreb = json!({
        "contents": {
            "tabbedSearchResultsRenderer": {
                "tabs": [{
                    "tabRenderer": {
                        "content": {
                            "sectionListRenderer": {
                                "contents": [{
                                    "musicShelfRenderer": {
                                        "contents": [{
                                            "musicResponsiveListItemRenderer": {
                                                "flexColumns": [
                                                    {
                                                        "musicResponsiveListItemFlexColumnRenderer": {
                                                            "text": {
                                                                "runs": [{
                                                                    "text": "Channel Track",
                                                                    "navigationEndpoint": {
                                                                        "watchEndpoint": { "videoId": "channel_track_01" }
                                                                    }
                                                                }]
                                                            }
                                                        }
                                                    },
                                                    {
                                                        "musicResponsiveListItemFlexColumnRenderer": {
                                                            "text": {
                                                                "runs": [
                                                                    { "text": "Artist X" },
                                                                    { "text": " • " },
                                                                    {
                                                                        "text": "Channel Release",
                                                                        "navigationEndpoint": {
                                                                            "browseEndpoint": {
                                                                                "browseId": "UC_channel_not_mpreb"
                                                                            }
                                                                        }
                                                                    },
                                                                    { "text": " • " },
                                                                    { "text": "3:45" }
                                                                ]
                                                            }
                                                        }
                                                    }
                                                ]
                                            }
                                        }]
                                    }
                                }]
                            }
                        }
                    }
                }]
            }
        }
    });

    let tracks = parse_search_response(&val_non_mpreb).expect("Parse non-MPREb browseId");
    assert_eq!(tracks.len(), 1);
    // When browseId lacks MPREb_, it is not recognized as an album.
    assert_eq!(tracks[0].album, None);
}

// ============================================================================
// 3. Player Response Parsing & Playability Errors
// ============================================================================

#[test]
fn test_player_login_required() {
    let player_json = json!({
        "playabilityStatus": {
            "status": "LOGIN_REQUIRED",
            "reason": "Sign in to confirm you’re not a bot"
        }
    });

    let resp: InnerTubePlayerResponse = serde_json::from_value(player_json.clone()).unwrap();
    let err = select_itag_140_stream(&resp, "botguard_vid").unwrap_err();

    match err {
        ProviderError::PlayabilityError { status, reason } => {
            assert_eq!(status, "LOGIN_REQUIRED");
            assert_eq!(reason, "Sign in to confirm you’re not a bot");
        }
        other => panic!("Expected PlayabilityError, got: {:?}", other),
    }

    // Also verify parse_player_response behaves consistently
    let err2 = parse_player_response(&player_json).unwrap_err();
    match err2 {
        ProviderError::PlayabilityError { status, reason } => {
            assert_eq!(status, "LOGIN_REQUIRED");
            assert_eq!(reason, "Sign in to confirm you’re not a bot");
        }
        other => panic!("Expected PlayabilityError from parse_player_response, got: {:?}", other),
    }
}

#[test]
fn test_player_unplayable() {
    let player_json = json!({
        "playabilityStatus": {
            "status": "UNPLAYABLE",
            "reason": "This video is not available in your country"
        }
    });

    let resp: InnerTubePlayerResponse = serde_json::from_value(player_json.clone()).unwrap();
    let err = select_itag_140_stream(&resp, "geo_blocked_vid").unwrap_err();

    match err {
        ProviderError::PlayabilityError { status, reason } => {
            assert_eq!(status, "UNPLAYABLE");
            assert_eq!(reason, "This video is not available in your country");
        }
        other => panic!("Expected PlayabilityError, got: {:?}", other),
    }

    let err2 = parse_player_response(&player_json).unwrap_err();
    match err2 {
        ProviderError::PlayabilityError { status, reason } => {
            assert_eq!(status, "UNPLAYABLE");
            assert_eq!(reason, "This video is not available in your country");
        }
        other => panic!("Expected PlayabilityError, got: {:?}", other),
    }
}

#[test]
fn test_player_missing_reason_fallback() {
    let player_json = json!({
        "playabilityStatus": {
            "status": "ERROR"
        }
    });

    let resp: InnerTubePlayerResponse = serde_json::from_value(player_json).unwrap();
    let err = select_itag_140_stream(&resp, "error_vid").unwrap_err();

    match err {
        ProviderError::PlayabilityError { status, reason } => {
            assert_eq!(status, "ERROR");
            assert_eq!(reason, "Playback not permitted");
        }
        other => panic!("Expected PlayabilityError, got: {:?}", other),
    }
}

#[test]
fn test_player_missing_streaming_data() {
    let player_json = json!({
        "playabilityStatus": { "status": "OK" }
    });

    let resp: InnerTubePlayerResponse = serde_json::from_value(player_json.clone()).unwrap();
    let err = select_itag_140_stream(&resp, "no_stream_data_vid").unwrap_err();

    match err {
        ProviderError::NoStreamsAvailable(vid) => {
            assert_eq!(vid, "no_stream_data_vid");
        }
        other => panic!("Expected NoStreamsAvailable, got: {:?}", other),
    }

    let err2 = parse_player_response(&player_json).unwrap_err();
    match err2 {
        ProviderError::NoStreamsAvailable(msg) => {
            assert!(msg.contains("streamingData"));
        }
        other => panic!("Expected NoStreamsAvailable from parse_player_response, got: {:?}", other),
    }
}

#[test]
fn test_player_missing_adaptive_formats() {
    let player_json = json!({
        "playabilityStatus": { "status": "OK" },
        "streamingData": {
            "formats": []
        }
    });

    let resp: InnerTubePlayerResponse = serde_json::from_value(player_json.clone()).unwrap();
    let err = select_itag_140_stream(&resp, "no_adaptive_vid").unwrap_err();

    match err {
        ProviderError::NoStreamsAvailable(vid) => {
            assert_eq!(vid, "no_adaptive_vid");
        }
        other => panic!("Expected NoStreamsAvailable, got: {:?}", other),
    }

    let err2 = parse_player_response(&player_json).unwrap_err();
    match err2 {
        ProviderError::NoStreamsAvailable(msg) => {
            assert!(msg.contains("adaptiveFormats"));
        }
        other => panic!("Expected NoStreamsAvailable, got: {:?}", other),
    }
}

#[test]
fn test_player_formats_without_audio_mp4() {
    // Only itag 251 (Opus WebM) and itag 137 (Video MP4) - no itag 140 and no audio/mp4
    let player_json = json!({
        "playabilityStatus": { "status": "OK" },
        "streamingData": {
            "adaptiveFormats": [
                {
                    "itag": 251,
                    "mimeType": "audio/webm; codecs=\"opus\"",
                    "bitrate": 160000,
                    "url": "https://example.com/opus.webm"
                },
                {
                    "itag": 137,
                    "mimeType": "video/mp4; codecs=\"avc1.640028\"",
                    "bitrate": 4000000,
                    "url": "https://example.com/video.mp4"
                }
            ]
        }
    });

    let resp: InnerTubePlayerResponse = serde_json::from_value(player_json).unwrap();
    let err = select_itag_140_stream(&resp, "no_aac_vid").unwrap_err();

    // Must be rejected to protect Symphonia from an Opus UnsupportedCodec crash
    match err {
        ProviderError::NoStreamsAvailable(vid) => {
            assert_eq!(vid, "no_aac_vid");
        }
        other => panic!("Expected NoStreamsAvailable, got: {:?}", other),
    }
}

#[test]
fn test_player_formats_missing_both_url_and_signature_cipher() {
    let player_json = json!({
        "playabilityStatus": { "status": "OK" },
        "streamingData": {
            "adaptiveFormats": [
                {
                    "itag": 140,
                    "mimeType": "audio/mp4; codecs=\"mp4a.40.2\"",
                    "bitrate": 128000,
                    "audioSampleRate": "44100",
                    "url": null,
                    "signatureCipher": null,
                    "cipher": null
                }
            ]
        }
    });

    let resp: InnerTubePlayerResponse = serde_json::from_value(player_json).unwrap();
    let err = select_itag_140_stream(&resp, "empty_format_vid").unwrap_err();

    match err {
        ProviderError::NoStreamsAvailable(msg) => {
            assert!(msg.contains("lacks both url and signatureCipher"));
        }
        other => panic!("Expected NoStreamsAvailable, got: {:?}", other),
    }
}

#[test]
fn test_resolve_candidate_cipher_without_loaded_ops() {
    let candidate = RawStreamCandidate::Cipher {
        cipher_str: "s=test_sig&url=https%3A%2F%2Fexample.com".to_string(),
        bitrate_kbps: 128,
        sample_rate_hz: 44100,
    };

    let err = resolve_candidate_to_stream(candidate, None).unwrap_err();
    match err {
        ProviderError::CipherError(msg) => {
            assert!(msg.contains("Cipher operations not loaded"));
        }
        other => panic!("Expected CipherError when ops is None, got: {:?}", other),
    }
}

#[test]
fn test_descramble_signature_malformed_queries() {
    let ops = [CipherOp::Reverse];

    // 1. Missing 's' parameter
    let err_missing_s = descramble_signature("url=https%3A%2F%2Fexample.com", &ops).unwrap_err();
    match err_missing_s {
        ProviderError::CipherError(msg) => assert!(msg.contains("Missing 's' parameter")),
        other => panic!("Expected CipherError, got: {:?}", other),
    }

    // 2. Missing 'url' parameter
    let err_missing_url = descramble_signature("s=some_scrambled_sig", &ops).unwrap_err();
    match err_missing_url {
        ProviderError::CipherError(msg) => assert!(msg.contains("Missing 'url' parameter")),
        other => panic!("Expected CipherError, got: {:?}", other),
    }
}

#[test]
fn test_transform_stream_url_n_edge_cases() {
    // 1. Empty URL
    assert_eq!(transform_stream_url_n(""), "");

    // 2. URL without n parameter
    let no_n = "https://rr1---sn-xxx.googlevideo.com/videoplayback?itag=140";
    assert_eq!(transform_stream_url_n(no_n), no_n);

    // 3. Completely malformed URL string with n=
    let malformed = "http://[invalid url with spaces]?n=test";
    let res = transform_stream_url_n(malformed);
    assert_eq!(res, malformed); // Non-fatal fallback returns original
}

// ============================================================================
// 4. Zero Unwrap Panic Assertions on Degraded Types
// ============================================================================

#[test]
fn test_json_parse_error_variants() {
    // Malformed JSON types passed into parse_search_response
    let number_val = json!(42);
    let err_num = parse_search_response(&number_val).unwrap_err();
    match err_num {
        ProviderError::JsonParse(_) => {}
        other => panic!("Expected JsonParse error, got {:?}", other),
    }

    let string_val = json!("not a valid search structure");
    let err_str = parse_search_response(&string_val).unwrap_err();
    match err_str {
        ProviderError::JsonParse(_) => {}
        other => panic!("Expected JsonParse error, got {:?}", other),
    }

    let array_val = json!([1, 2, 3]);
    let err_arr = parse_search_response(&array_val).unwrap_err();
    match err_arr {
        ProviderError::JsonParse(_) => {}
        other => panic!("Expected JsonParse error, got {:?}", other),
    }
}

#[test]
fn test_artist_feat_delimiter_and_runs_behavior() {
    // Case 1: "feat." as a standalone delimiter run
    let val_feat = json!({
        "contents": {
            "tabbedSearchResultsRenderer": {
                "tabs": [{
                    "tabRenderer": {
                        "content": {
                            "sectionListRenderer": {
                                "contents": [{
                                    "musicShelfRenderer": {
                                        "contents": [{
                                            "musicResponsiveListItemRenderer": {
                                                "flexColumns": [
                                                    {
                                                        "musicResponsiveListItemFlexColumnRenderer": {
                                                            "text": {
                                                                "runs": [{
                                                                    "text": "Hip Hop Track",
                                                                    "navigationEndpoint": {
                                                                        "watchEndpoint": { "videoId": "hiphop_feat" }
                                                                    }
                                                                }]
                                                            }
                                                        }
                                                    },
                                                    {
                                                        "musicResponsiveListItemFlexColumnRenderer": {
                                                            "text": {
                                                                "runs": [
                                                                    { "text": "Lead Rapper" },
                                                                    { "text": " feat. " },
                                                                    { "text": "Guest Star" },
                                                                    { "text": " • " },
                                                                    { "text": "3:20" }
                                                                ]
                                                            }
                                                        }
                                                    }
                                                ]
                                            }
                                        }]
                                    }
                                }]
                            }
                        }
                    }
                }]
            }
        }
    });

    let tracks = parse_search_response(&val_feat).expect("Parse feat run");
    assert_eq!(tracks.len(), 1);
    // Observe: because "feat." is not "&" or ",", it is treated as a token in the artist list
    assert_eq!(tracks[0].artist, "Lead Rapper, feat., Guest Star");

    // Case 2: "feat." inside the artist name run
    let val_feat_embedded = json!({
        "contents": {
            "tabbedSearchResultsRenderer": {
                "tabs": [{
                    "tabRenderer": {
                        "content": {
                            "sectionListRenderer": {
                                "contents": [{
                                    "musicShelfRenderer": {
                                        "contents": [{
                                            "musicResponsiveListItemRenderer": {
                                                "flexColumns": [
                                                    {
                                                        "musicResponsiveListItemFlexColumnRenderer": {
                                                            "text": {
                                                                "runs": [{
                                                                    "text": "Hip Hop Track 2",
                                                                    "navigationEndpoint": {
                                                                        "watchEndpoint": { "videoId": "hiphop_feat2" }
                                                                    }
                                                                }]
                                                            }
                                                        }
                                                    },
                                                    {
                                                        "musicResponsiveListItemFlexColumnRenderer": {
                                                            "text": {
                                                                "runs": [
                                                                    { "text": "Lead Rapper feat. Guest Star" },
                                                                    { "text": " • " },
                                                                    { "text": "3:20" }
                                                                ]
                                                            }
                                                        }
                                                    }
                                                ]
                                            }
                                        }]
                                    }
                                }]
                            }
                        }
                    }
                }]
            }
        }
    });

    let tracks2 = parse_search_response(&val_feat_embedded).expect("Parse embedded feat");
    assert_eq!(tracks2.len(), 1);
    assert_eq!(tracks2[0].artist, "Lead Rapper feat. Guest Star");
}

#[test]
fn test_thumbnail_resolution_upgrade_and_edge_cases() {
    let val = json!({
        "contents": {
            "tabbedSearchResultsRenderer": {
                "tabs": [{
                    "tabRenderer": {
                        "content": {
                            "sectionListRenderer": {
                                "contents": [{
                                    "musicShelfRenderer": {
                                        "contents": [
                                            // Item 1: Standard Google thumbnail format with =w...-h...
                                            {
                                                "musicResponsiveListItemRenderer": {
                                                    "flexColumns": [{
                                                        "musicResponsiveListItemFlexColumnRenderer": {
                                                            "text": {
                                                                "runs": [{
                                                                    "text": "Song 1",
                                                                    "navigationEndpoint": {
                                                                        "watchEndpoint": { "videoId": "vid_1" }
                                                                    }
                                                                }]
                                                            }
                                                        }
                                                    }],
                                                    "thumbnail": {
                                                        "musicThumbnailRenderer": {
                                                            "thumbnail": {
                                                                "thumbnails": [
                                                                    { "url": "https://lh3.googleusercontent.com/abc=w60-h60-l90-rj" },
                                                                    { "url": "https://lh3.googleusercontent.com/abc=w120-h120-l90-rj" }
                                                                ]
                                                            }
                                                        }
                                                    }
                                                }
                                            },
                                            // Item 2: Custom CDN thumbnail without =w token
                                            {
                                                "musicResponsiveListItemRenderer": {
                                                    "flexColumns": [{
                                                        "musicResponsiveListItemFlexColumnRenderer": {
                                                            "text": {
                                                                "runs": [{
                                                                    "text": "Song 2",
                                                                    "navigationEndpoint": {
                                                                        "watchEndpoint": { "videoId": "vid_2" }
                                                                    }
                                                                }]
                                                            }
                                                        }
                                                    }],
                                                    "thumbnail": {
                                                        "musicThumbnailRenderer": {
                                                            "thumbnail": {
                                                                "thumbnails": [
                                                                    { "url": "https://cdn.example.com/cover.png" }
                                                                ]
                                                            }
                                                        }
                                                    }
                                                }
                                            },
                                            // Item 3: Missing thumbnail container
                                            {
                                                "musicResponsiveListItemRenderer": {
                                                    "flexColumns": [{
                                                        "musicResponsiveListItemFlexColumnRenderer": {
                                                            "text": {
                                                                "runs": [{
                                                                    "text": "Song 3",
                                                                    "navigationEndpoint": {
                                                                        "watchEndpoint": { "videoId": "vid_3" }
                                                                    }
                                                                }]
                                                            }
                                                        }
                                                    }],
                                                    "thumbnail": null
                                                }
                                            }
                                        ]
                                    }
                                }]
                            }
                        }
                    }
                }]
            }
        }
    });

    let tracks = parse_search_response(&val).expect("Parse thumbnails");
    assert_eq!(tracks.len(), 3);

    // Item 1: Upgraded to 544x544
    assert_eq!(
        tracks[0].thumbnail_url,
        Some("https://lh3.googleusercontent.com/abc=w544-h544-l90-rj".to_string())
    );

    // Item 2: Preserved intact without =w token
    assert_eq!(
        tracks[1].thumbnail_url,
        Some("https://cdn.example.com/cover.png".to_string())
    );

    // Item 3: None
    assert_eq!(tracks[2].thumbnail_url, None);
}

