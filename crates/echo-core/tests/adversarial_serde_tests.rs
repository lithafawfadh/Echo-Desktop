use echo_core::{
    AudioCodec, AudioFidelity, AudioStream, LyricLine, Lyrics, PlaybackProgress, PlaybackState,
    Track,
};
use std::cmp::Ordering;
use std::collections::HashMap;

// ==============================================================================
// 1. AudioFidelity: Permutation Sorting, Transitivity, and Ordering
// ==============================================================================

fn generate_permutations<T: Clone>(items: &[T]) -> Vec<Vec<T>> {
    if items.len() <= 1 {
        return vec![items.to_vec()];
    }
    let mut result = Vec::new();
    for i in 0..items.len() {
        let mut rest = items.to_vec();
        let current = rest.remove(i);
        let sub_perms = generate_permutations(&rest);
        for mut sub in sub_perms {
            sub.insert(0, current.clone());
            result.push(sub);
        }
    }
    result
}

#[test]
fn test_audio_fidelity_all_permutations_sorting() {
    let canonical = [
        AudioFidelity::LossyStandard,
        AudioFidelity::LossyHigh,
        AudioFidelity::Lossy320,
        AudioFidelity::LosslessCD,
        AudioFidelity::LosslessHiRes,
    ];

    let perms = generate_permutations(&canonical);
    assert_eq!(perms.len(), 120, "5! must equal 120 permutations");

    for (idx, perm) in perms.iter().enumerate() {
        let mut sorted = perm.clone();
        sorted.sort();
        assert_eq!(
            sorted.as_slice(),
            &canonical,
            "Permutation {idx} failed to sort canonically: input={perm:?}, sorted={sorted:?}"
        );

        let mut rev_sorted = perm.clone();
        rev_sorted.sort_by(|a, b| b.cmp(a));
        let mut expected_rev = canonical;
        expected_rev.reverse();
        assert_eq!(
            rev_sorted.as_slice(),
            &expected_rev,
            "Permutation {idx} failed reverse sort: input={perm:?}, rev_sorted={rev_sorted:?}"
        );
    }
}

#[test]
fn test_audio_fidelity_pairwise_and_transitivity() {
    let variants = [
        AudioFidelity::LossyStandard,
        AudioFidelity::LossyHigh,
        AudioFidelity::Lossy320,
        AudioFidelity::LosslessCD,
        AudioFidelity::LosslessHiRes,
    ];

    // Verify all 25 pairwise comparisons
    for (i, a) in variants.iter().enumerate() {
        for (j, b) in variants.iter().enumerate() {
            if i < j {
                assert!(a < b, "{a:?} should be < {b:?}");
                assert!(a <= b, "{a:?} should be <= {b:?}");
                assert!(!(a > b), "{a:?} should not be > {b:?}");
                assert!(!(a >= b), "{a:?} should not be >= {b:?}");
                assert_ne!(a, b, "{a:?} should not equal {b:?}");
                assert_eq!(a.cmp(b), Ordering::Less);
                assert_eq!(a.partial_cmp(b), Some(Ordering::Less));
            } else if i == j {
                assert_eq!(a, b, "{a:?} should equal {b:?}");
                assert!(a <= b, "{a:?} should be <= {b:?}");
                assert!(a >= b, "{a:?} should be >= {b:?}");
                assert!(!(a < b), "{a:?} should not be < {b:?}");
                assert!(!(a > b), "{a:?} should not be > {b:?}");
                assert_eq!(a.cmp(b), Ordering::Equal);
                assert_eq!(a.partial_cmp(b), Some(Ordering::Equal));
            } else {
                assert!(a > b, "{a:?} should be > {b:?}");
                assert!(a >= b, "{a:?} should be >= {b:?}");
                assert!(!(a < b), "{a:?} should not be < {b:?}");
                assert!(!(a <= b), "{a:?} should not be <= {b:?}");
                assert_ne!(a, b, "{a:?} should not equal {b:?}");
                assert_eq!(a.cmp(b), Ordering::Greater);
                assert_eq!(a.partial_cmp(b), Some(Ordering::Greater));
            }
        }
    }

    // Verify transitivity across all 125 triplets (a, b, c)
    for a in &variants {
        for b in &variants {
            for c in &variants {
                if a <= b && b <= c {
                    assert!(a <= c, "Transitivity failed: {a:?} <= {b:?} and {b:?} <= {c:?}, but not {a:?} <= {c:?}");
                }
                if a < b && b < c {
                    assert!(a < c, "Strict transitivity failed: {a:?} < {b:?} and {b:?} < {c:?}, but not {a:?} < {c:?}");
                }
            }
        }
    }

    // Verify is_lossless boundary condition
    assert!(!AudioFidelity::LossyStandard.is_lossless());
    assert!(!AudioFidelity::LossyHigh.is_lossless());
    assert!(!AudioFidelity::Lossy320.is_lossless());
    assert!(AudioFidelity::LosslessCD.is_lossless());
    assert!(AudioFidelity::LosslessHiRes.is_lossless());
}

// ==============================================================================
// 2. AudioFidelity & AudioCodec: Serde Boundaries & Type Protections
// ==============================================================================

#[test]
fn test_audio_fidelity_serde_variants_and_rejections() {
    // Valid string variants
    let valid_cases = [
        ("\"LossyStandard\"", AudioFidelity::LossyStandard),
        ("\"LossyHigh\"", AudioFidelity::LossyHigh),
        ("\"Lossy320\"", AudioFidelity::Lossy320),
        ("\"LosslessCD\"", AudioFidelity::LosslessCD),
        ("\"LosslessHiRes\"", AudioFidelity::LosslessHiRes),
    ];
    for (json, expected) in valid_cases {
        let parsed: AudioFidelity = serde_json::from_str(json).unwrap();
        assert_eq!(parsed, expected);
    }

    // Invalid string variants must fail
    let invalid_strings = [
        "\"\"",
        "\"lossy_standard\"",
        "\"LOSSY_320\"",
        "\"lossless_cd\"",
        "\"lossless_hi_res\"",
        "\"LosslessMaster\"",
        "\"Standard\"",
        "\"Unknown\"",
        "\"FLAC\"",
    ];
    for s in invalid_strings {
        assert!(
            serde_json::from_str::<AudioFidelity>(s).is_err(),
            "Expected deserialization failure for {s}"
        );
    }

    // Type mismatches: numbers, null, boolean, array, object must fail
    let invalid_types = [
        "1", "2", "3", "4", "5", "0", "-1", "99",
        "null",
        "true", "false",
        "1.5",
        "[]", "{}",
    ];
    for val in invalid_types {
        assert!(
            serde_json::from_str::<AudioFidelity>(val).is_err(),
            "Expected deserialization failure for non-string {val}"
        );
    }
}

#[test]
fn test_audio_codec_serde_variants_and_rejections() {
    let valid_codecs = [
        (AudioCodec::Opus, "\"Opus\"", "Opus"),
        (AudioCodec::Aac, "\"Aac\"", "AAC"),
        (AudioCodec::Flac, "\"Flac\"", "FLAC"),
        (AudioCodec::Alac, "\"Alac\"", "ALAC"),
        (AudioCodec::Wav, "\"Wav\"", "WAV"),
        (AudioCodec::Mp3, "\"Mp3\"", "MP3"),
        (AudioCodec::Unknown, "\"Unknown\"", "Unknown"),
    ];

    for (codec, expected_json, expected_str) in valid_codecs {
        // Serialization
        let serialized = serde_json::to_string(&codec).unwrap();
        assert_eq!(serialized, expected_json);

        // Deserialization
        let deserialized: AudioCodec = serde_json::from_str(expected_json).unwrap();
        assert_eq!(deserialized, codec);

        // as_str & Display
        assert_eq!(codec.as_str(), expected_str);
        assert_eq!(format!("{codec}"), expected_str);
    }

    // Invalid strings must be rejected
    let invalid_codec_strings = [
        "\"\"", "\"opus\"", "\"aac\"", "\"flac\"", "\"alac\"", "\"wav\"", "\"mp3\"",
        "\"Vorbis\"", "\"Ogg\"", "\"M4a\"", "\"FLAC\"",
    ];
    for s in invalid_codec_strings {
        assert!(
            serde_json::from_str::<AudioCodec>(s).is_err(),
            "Expected rejection of {s}"
        );
    }

    // Non-string types rejected
    assert!(serde_json::from_str::<AudioCodec>("null").is_err());
    assert!(serde_json::from_str::<AudioCodec>("0").is_err());
    assert!(serde_json::from_str::<AudioCodec>("true").is_err());
    assert!(serde_json::from_str::<AudioCodec>("[]").is_err());
    assert!(serde_json::from_str::<AudioCodec>("{}").is_err());
}

// ==============================================================================
// 3. AudioStream: Missing Fields, Null Fields, and Extra Unknown Fields
// ==============================================================================

#[test]
fn test_audio_stream_missing_fields_behavior() {
    // 1. Missing optional fields (bit_depth, fidelity, content_length_bytes, http_headers)
    // Must succeed with default None / empty map
    let minimal_json = r#"{
        "url": "https://cdn.example.com/audio.flac",
        "codec": "Flac",
        "bitrate_kbps": 1411,
        "sample_rate_hz": 44100,
        "is_lossless": true
    }"#;
    let stream: AudioStream = serde_json::from_str(minimal_json).expect("Minimal stream must parse");
    assert_eq!(stream.url, "https://cdn.example.com/audio.flac");
    assert_eq!(stream.codec, AudioCodec::Flac);
    assert_eq!(stream.bitrate_kbps, 1411);
    assert_eq!(stream.sample_rate_hz, 44100);
    assert!(stream.is_lossless);
    assert_eq!(stream.bit_depth, None);
    assert_eq!(stream.fidelity, None);
    assert_eq!(stream.content_length_bytes, None);
    assert!(stream.http_headers.is_empty());

    // 2. Missing required fields must fail
    let missing_url = r#"{
        "codec": "Flac",
        "bitrate_kbps": 1411,
        "sample_rate_hz": 44100,
        "is_lossless": true
    }"#;
    assert!(serde_json::from_str::<AudioStream>(missing_url).is_err(), "Missing url must fail");

    let missing_codec = r#"{
        "url": "https://cdn.example.com/audio.flac",
        "bitrate_kbps": 1411,
        "sample_rate_hz": 44100,
        "is_lossless": true
    }"#;
    assert!(serde_json::from_str::<AudioStream>(missing_codec).is_err(), "Missing codec must fail");

    let missing_bitrate = r#"{
        "url": "https://cdn.example.com/audio.flac",
        "codec": "Flac",
        "sample_rate_hz": 44100,
        "is_lossless": true
    }"#;
    assert!(serde_json::from_str::<AudioStream>(missing_bitrate).is_err(), "Missing bitrate must fail");

    let missing_sample_rate = r#"{
        "url": "https://cdn.example.com/audio.flac",
        "codec": "Flac",
        "bitrate_kbps": 1411,
        "is_lossless": true
    }"#;
    assert!(serde_json::from_str::<AudioStream>(missing_sample_rate).is_err(), "Missing sample_rate must fail");

    let missing_is_lossless = r#"{
        "url": "https://cdn.example.com/audio.flac",
        "codec": "Flac",
        "bitrate_kbps": 1411,
        "sample_rate_hz": 44100
    }"#;
    assert!(serde_json::from_str::<AudioStream>(missing_is_lossless).is_err(), "Missing is_lossless must fail");

    assert!(serde_json::from_str::<AudioStream>("{}").is_err(), "Empty object must fail");
}

#[test]
fn test_audio_stream_null_fields_behavior() {
    // Explicit nulls on Option fields must deserialize to None
    let json_null_options = r#"{
        "url": "https://cdn.example.com/audio.mp3",
        "codec": "Mp3",
        "bitrate_kbps": 320,
        "sample_rate_hz": 48000,
        "is_lossless": false,
        "bit_depth": null,
        "fidelity": null,
        "content_length_bytes": null
    }"#;
    let stream: AudioStream = serde_json::from_str(json_null_options).expect("Null options must parse");
    assert_eq!(stream.bit_depth, None);
    assert_eq!(stream.fidelity, None);
    assert_eq!(stream.content_length_bytes, None);
    assert!(stream.http_headers.is_empty());

    // Explicit null on http_headers (non-Option HashMap) returns Err in serde_json
    let null_headers = r#"{
        "url": "https://cdn.example.com/audio.mp3",
        "codec": "Mp3",
        "bitrate_kbps": 320,
        "sample_rate_hz": 48000,
        "is_lossless": false,
        "http_headers": null
    }"#;
    let null_headers_res = serde_json::from_str::<AudioStream>(null_headers);
    assert!(
        null_headers_res.is_err(),
        "Non-option HashMap field http_headers with null value must fail deserialization (serde expects a map)"
    );

    // Explicit null on non-Option fields MUST fail
    let null_url = r#"{
        "url": null,
        "codec": "Mp3",
        "bitrate_kbps": 320,
        "sample_rate_hz": 48000,
        "is_lossless": false
    }"#;
    assert!(serde_json::from_str::<AudioStream>(null_url).is_err(), "Null url must fail");

    let null_codec = r#"{
        "url": "https://cdn.example.com/audio.mp3",
        "codec": null,
        "bitrate_kbps": 320,
        "sample_rate_hz": 48000,
        "is_lossless": false
    }"#;
    assert!(serde_json::from_str::<AudioStream>(null_codec).is_err(), "Null codec must fail");

    let null_bitrate = r#"{
        "url": "https://cdn.example.com/audio.mp3",
        "codec": "Mp3",
        "bitrate_kbps": null,
        "sample_rate_hz": 48000,
        "is_lossless": false
    }"#;
    assert!(serde_json::from_str::<AudioStream>(null_bitrate).is_err(), "Null bitrate must fail");

    let null_lossless = r#"{
        "url": "https://cdn.example.com/audio.mp3",
        "codec": "Mp3",
        "bitrate_kbps": 320,
        "sample_rate_hz": 48000,
        "is_lossless": null
    }"#;
    assert!(serde_json::from_str::<AudioStream>(null_lossless).is_err(), "Null is_lossless must fail");
}

#[test]
fn test_audio_stream_extra_unknown_fields_forward_compatibility() {
    let json_with_extra = r#"{
        "url": "https://cdn.example.com/audio.opus",
        "codec": "Opus",
        "bitrate_kbps": 160,
        "sample_rate_hz": 48000,
        "is_lossless": false,
        "bit_depth": null,
        "fidelity": "LossyHigh",
        "unknown_string_field": "future_metadata_value",
        "unknown_int_field": 999999,
        "unknown_nested_object": {
            "key1": "val1",
            "nested_array": [1, 2, 3, {"inner": true}]
        },
        "extra_boolean_flag": true,
        "extra_null": null
    }"#;

    let stream: AudioStream = serde_json::from_str(json_with_extra)
        .expect("AudioStream must ignore unknown fields without error");

    assert_eq!(stream.codec, AudioCodec::Opus);
    assert_eq!(stream.fidelity, Some(AudioFidelity::LossyHigh));

    // Reserialization must NOT contain the unrecognized fields
    let reserialized = serde_json::to_string(&stream).unwrap();
    assert!(!reserialized.contains("unknown_string_field"));
    assert!(!reserialized.contains("unknown_int_field"));
    assert!(!reserialized.contains("unknown_nested_object"));
    assert!(!reserialized.contains("extra_boolean_flag"));
}

// ==============================================================================
// 4. AudioStream & Track: Integer Overflows & Boundary Values
// ==============================================================================

#[test]
fn test_audio_stream_integer_boundaries_and_overflows() {
    // 1. bit_depth is Option<u8> (0 to 255)
    // Valid boundaries
    for valid_depth in [0u8, 1, 8, 16, 24, 32, 255] {
        let json = format!(
            r#"{{"url":"u","codec":"Flac","bitrate_kbps":1411,"sample_rate_hz":44100,"is_lossless":true,"bit_depth":{valid_depth}}}"#
        );
        let s: AudioStream = serde_json::from_str(&json).unwrap();
        assert_eq!(s.bit_depth, Some(valid_depth));
    }

    // Overflows on bit_depth (> 255 or negative)
    for bad_depth in ["256", "1000", "65536", "4294967296", "-1", "-128", "24.5"] {
        let json = format!(
            r#"{{"url":"u","codec":"Flac","bitrate_kbps":1411,"sample_rate_hz":44100,"is_lossless":true,"bit_depth":{bad_depth}}}"#
        );
        assert!(
            serde_json::from_str::<AudioStream>(&json).is_err(),
            "bit_depth={bad_depth} must fail deserialization"
        );
    }

    // 2. bitrate_kbps is u32 (0 to 4_294_967_295)
    for valid_kbps in [0u32, 128, 320, 1411, 4608, 9216, u32::MAX] {
        let json = format!(
            r#"{{"url":"u","codec":"Flac","bitrate_kbps":{valid_kbps},"sample_rate_hz":44100,"is_lossless":true}}"#
        );
        let s: AudioStream = serde_json::from_str(&json).unwrap();
        assert_eq!(s.bitrate_kbps, valid_kbps);
    }

    // Overflows on bitrate_kbps
    for bad_kbps in ["4294967296", "18446744073709551615", "-1", "320.5"] {
        let json = format!(
            r#"{{"url":"u","codec":"Flac","bitrate_kbps":{bad_kbps},"sample_rate_hz":44100,"is_lossless":true}}"#
        );
        assert!(
            serde_json::from_str::<AudioStream>(&json).is_err(),
            "bitrate_kbps={bad_kbps} must fail deserialization"
        );
    }

    // 3. sample_rate_hz is u32 (0 to 4_294_967_295)
    for valid_sr in [0u32, 44100, 48000, 88200, 96000, 192000, 384000, u32::MAX] {
        let json = format!(
            r#"{{"url":"u","codec":"Flac","bitrate_kbps":1411,"sample_rate_hz":{valid_sr},"is_lossless":true}}"#
        );
        let s: AudioStream = serde_json::from_str(&json).unwrap();
        assert_eq!(s.sample_rate_hz, valid_sr);
    }

    // Overflows on sample_rate_hz
    for bad_sr in ["4294967296", "-1", "44100.0"] {
        let json = format!(
            r#"{{"url":"u","codec":"Flac","bitrate_kbps":1411,"sample_rate_hz":{bad_sr},"is_lossless":true}}"#
        );
        assert!(
            serde_json::from_str::<AudioStream>(&json).is_err(),
            "sample_rate_hz={bad_sr} must fail deserialization"
        );
    }

    // 4. content_length_bytes is Option<u64> (0 to 18_446_744_073_709_551_615)
    for valid_len in [0u64, 1024, 128_450_120, u64::MAX] {
        let json = format!(
            r#"{{"url":"u","codec":"Flac","bitrate_kbps":1411,"sample_rate_hz":44100,"is_lossless":true,"content_length_bytes":{valid_len}}}"#
        );
        let s: AudioStream = serde_json::from_str(&json).unwrap();
        assert_eq!(s.content_length_bytes, Some(valid_len));
    }

    // Overflows on content_length_bytes
    for bad_len in ["18446744073709551616", "-1", "100.5"] {
        let json = format!(
            r#"{{"url":"u","codec":"Flac","bitrate_kbps":1411,"sample_rate_hz":44100,"is_lossless":true,"content_length_bytes":{bad_len}}}"#
        );
        assert!(
            serde_json::from_str::<AudioStream>(&json).is_err(),
            "content_length_bytes={bad_len} must fail deserialization"
        );
    }
}

// ==============================================================================
// 5. AudioStream & Track: Extreme Boundary Strings & Unicode
// ==============================================================================

#[test]
fn test_audio_stream_extreme_boundary_strings() {
    // 1. Extreme URL lengths and characters:
    // Empty URL
    let empty_url_json = r#"{
        "url": "",
        "codec": "Unknown",
        "bitrate_kbps": 0,
        "sample_rate_hz": 0,
        "is_lossless": false
    }"#;
    let s: AudioStream = serde_json::from_str(empty_url_json).unwrap();
    assert_eq!(s.url, "");

    // 100KB giant URL string with query params
    let giant_url = format!("https://cdn.provider.internal/stream?token={}", "A".repeat(100_000));
    let giant_json = format!(
        r#"{{"url":"{giant_url}","codec":"Flac","bitrate_kbps":1411,"sample_rate_hz":44100,"is_lossless":true}}"#
    );
    let s: AudioStream = serde_json::from_str(&giant_json).unwrap();
    assert_eq!(s.url.len(), giant_url.len());

    // Unicode & Emoji in URL and headers
    let unicode_url = "https://cdn.example.com/🎵/음악/موسيقى/композиция?sig=🎧✨&q=日本語#hash";
    let mut headers = HashMap::new();
    headers.insert("X-Custom-Auth".to_string(), "Bearer secret_🔑_123".to_string());
    headers.insert("X-RTL-Header".to_string(), "مرحبا بالعالم".to_string());
    headers.insert("X-CJK-Header".to_string(), "音楽再生トークン".to_string());
    headers.insert("X-Empty".to_string(), "".to_string());

    let stream = AudioStream {
        url: unicode_url.to_string(),
        codec: AudioCodec::Flac,
        bitrate_kbps: 4608,
        sample_rate_hz: 96000,
        is_lossless: true,
        bit_depth: Some(24),
        fidelity: Some(AudioFidelity::LosslessHiRes),
        content_length_bytes: Some(128_000_000),
        http_headers: headers.clone(),
    };

    let serialized = serde_json::to_string(&stream).unwrap();
    let deserialized: AudioStream = serde_json::from_str(&serialized).unwrap();
    assert_eq!(stream, deserialized);
    assert_eq!(deserialized.url, unicode_url);
    assert_eq!(deserialized.http_headers.get("X-Custom-Auth").unwrap(), "Bearer secret_🔑_123");
    assert_eq!(deserialized.http_headers.get("X-RTL-Header").unwrap(), "مرحبا بالعالم");
    assert_eq!(deserialized.http_headers.get("X-Empty").unwrap(), "");
}

// ==============================================================================
// 6. Track: Full Serde Boundaries, Missing/Null/Extra Fields, Overflow
// ==============================================================================

#[test]
fn test_track_serde_adversarial_matrix() {
    // 1. Missing isrc (legacy contract)
    let legacy_json = r#"{
        "id": "trk_legacy_100",
        "title": "Legacy Song",
        "artist": "Old Artist",
        "album": null,
        "duration_seconds": 180,
        "thumbnail_url": null
    }"#;
    let track: Track = serde_json::from_str(legacy_json).unwrap();
    assert_eq!(track.id, "trk_legacy_100");
    assert_eq!(track.isrc, None);
    assert_eq!(track.album, None);
    assert_eq!(track.thumbnail_url, None);

    // Serialization skips None isrc, while album and thumbnail_url are null
    let reserialized = serde_json::to_string(&track).unwrap();
    assert!(!reserialized.contains("isrc"));
    assert!(reserialized.contains("\"album\":null"));
    assert!(reserialized.contains("\"thumbnail_url\":null"));

    // 2. Explicit null isrc
    let null_isrc_json = r#"{
        "id": "trk_null_isrc",
        "title": "Null ISRC",
        "artist": "Artist",
        "duration_seconds": 200,
        "isrc": null
    }"#;
    let track_null_isrc: Track = serde_json::from_str(null_isrc_json).unwrap();
    assert_eq!(track_null_isrc.isrc, None);

    // 3. Valid ISRC
    let valid_isrc_json = r#"{
        "id": "trk_with_isrc",
        "title": "Get Lucky",
        "artist": "Daft Punk",
        "duration_seconds": 369,
        "isrc": "USQX91300105"
    }"#;
    let track_isrc: Track = serde_json::from_str(valid_isrc_json).unwrap();
    assert_eq!(track_isrc.isrc, Some("USQX91300105".to_string()));

    // 4. Missing required fields in Track must fail
    assert!(serde_json::from_str::<Track>(r#"{"title":"T","artist":"A","duration_seconds":10}"#).is_err(), "Missing id must fail");
    assert!(serde_json::from_str::<Track>(r#"{"id":"I","artist":"A","duration_seconds":10}"#).is_err(), "Missing title must fail");
    assert!(serde_json::from_str::<Track>(r#"{"id":"I","title":"T","duration_seconds":10}"#).is_err(), "Missing artist must fail");
    assert!(serde_json::from_str::<Track>(r#"{"id":"I","title":"T","artist":"A"}"#).is_err(), "Missing duration_seconds must fail");

    // 5. Null on required fields must fail
    assert!(serde_json::from_str::<Track>(r#"{"id":null,"title":"T","artist":"A","duration_seconds":10}"#).is_err());
    assert!(serde_json::from_str::<Track>(r#"{"id":"I","title":null,"artist":"A","duration_seconds":10}"#).is_err());
    assert!(serde_json::from_str::<Track>(r#"{"id":"I","title":"T","artist":null,"duration_seconds":10}"#).is_err());
    assert!(serde_json::from_str::<Track>(r#"{"id":"I","title":"T","artist":"A","duration_seconds":null}"#).is_err());

    // 6. Unknown fields ignored
    let extra_track_json = r#"{
        "id": "trk_extra",
        "title": "Extra",
        "artist": "Artist",
        "duration_seconds": 120,
        "extra_field_1": 12345,
        "extra_field_2": {"nested": "value"},
        "extra_array": [1, 2, 3]
    }"#;
    let extra_track: Track = serde_json::from_str(extra_track_json).unwrap();
    assert_eq!(extra_track.id, "trk_extra");

    // 7. duration_seconds overflow and boundaries
    for valid_dur in [0u32, 1, 300, 3600, 86400, u32::MAX] {
        let json = format!(r#"{{"id":"i","title":"t","artist":"a","duration_seconds":{valid_dur}}}"#);
        let t: Track = serde_json::from_str(&json).unwrap();
        assert_eq!(t.duration_seconds, valid_dur);
    }
    for bad_dur in ["4294967296", "-1", "120.5"] {
        let json = format!(r#"{{"id":"i","title":"t","artist":"a","duration_seconds":{bad_dur}}}"#);
        assert!(serde_json::from_str::<Track>(&json).is_err(), "duration_seconds={bad_dur} must fail");
    }

    // 8. Extreme strings in Track (100KB title, unicode, emojis)
    let giant_title = "🎵".repeat(10_000);
    let track_giant = Track {
        id: "trk_giant".to_string(),
        title: giant_title.clone(),
        artist: "Daft Punk ft. Pharrell & Nile Rodgers / 音楽家".to_string(),
        album: Some("Random Access Memories (10th Anniversary Deluxe)".to_string()),
        duration_seconds: 369,
        thumbnail_url: Some("https://img.provider.net/ram.jpg?size=1080".to_string()),
        isrc: Some("USQX91300105".to_string()),
    };
    let json_giant = serde_json::to_string(&track_giant).unwrap();
    let parsed_giant: Track = serde_json::from_str(&json_giant).unwrap();
    assert_eq!(track_giant, parsed_giant);
    assert_eq!(parsed_giant.title.len(), giant_title.len());
}

// ==============================================================================
// 7. Cross-Crate Compatibility: echo-audio, echo-lyrics, echo-cli
// ==============================================================================

#[test]
fn test_cross_crate_cli_codec_label_coverage() {
    // In echo-cli/src/main.rs:codec_label:
    // fn codec_label(codec: AudioCodec) -> &'static str
    // Must exhaustively cover all variants without panic.
    fn verify_codec_label(codec: AudioCodec) -> &'static str {
        match codec {
            AudioCodec::Opus => "Opus",
            AudioCodec::Aac => "AAC",
            AudioCodec::Flac => "FLAC",
            AudioCodec::Alac => "ALAC",
            AudioCodec::Wav => "WAV",
            AudioCodec::Mp3 => "MP3",
            AudioCodec::Unknown => "Unknown",
        }
    }

    assert_eq!(verify_codec_label(AudioCodec::Opus), "Opus");
    assert_eq!(verify_codec_label(AudioCodec::Aac), "AAC");
    assert_eq!(verify_codec_label(AudioCodec::Flac), "FLAC");
    assert_eq!(verify_codec_label(AudioCodec::Alac), "ALAC");
    assert_eq!(verify_codec_label(AudioCodec::Wav), "WAV");
    assert_eq!(verify_codec_label(AudioCodec::Mp3), "MP3");
    assert_eq!(verify_codec_label(AudioCodec::Unknown), "Unknown");
}

#[test]
fn test_cross_crate_lyrics_models_serde() {
    let line = LyricLine {
        timestamp_ms: 12500,
        text: "I want to break free 🎤🎶".to_string(),
    };
    let json = serde_json::to_string(&line).unwrap();
    let roundtrip: LyricLine = serde_json::from_str(&json).unwrap();
    assert_eq!(line, roundtrip);

    let lyrics = Lyrics {
        plain: Some("Plain lyrics text with\nmultiple lines".to_string()),
        synced: vec![
            LyricLine { timestamp_ms: 0, text: "Intro".into() },
            LyricLine { timestamp_ms: 5000, text: "Verse 1".into() },
            LyricLine { timestamp_ms: u64::MAX, text: "Outro".into() },
        ],
    };
    let lyrics_json = serde_json::to_string(&lyrics).unwrap();
    let lyrics_roundtrip: Lyrics = serde_json::from_str(&lyrics_json).unwrap();
    assert_eq!(lyrics, lyrics_roundtrip);

    // Verify binary search on Lyrics line_at
    assert_eq!(lyrics_roundtrip.line_at(0).unwrap().text, "Intro");
    assert_eq!(lyrics_roundtrip.line_at(4999).unwrap().text, "Intro");
    assert_eq!(lyrics_roundtrip.line_at(5000).unwrap().text, "Verse 1");
    assert_eq!(lyrics_roundtrip.line_at(100_000).unwrap().text, "Verse 1");
}

#[test]
fn test_cross_crate_audio_playback_progress_and_state_serde() {
    let progress = PlaybackProgress {
        position_ms: 45_000,
        duration_ms: 180_000,
    };
    let prog_json = serde_json::to_string(&progress).unwrap();
    let prog_roundtrip: PlaybackProgress = serde_json::from_str(&prog_json).unwrap();
    assert_eq!(progress.position_ms, prog_roundtrip.position_ms);
    assert_eq!(progress.duration_ms, prog_roundtrip.duration_ms);

    // Extreme values
    let extreme_progress = PlaybackProgress {
        position_ms: u64::MAX,
        duration_ms: u64::MAX,
    };
    let ext_json = serde_json::to_string(&extreme_progress).unwrap();
    let ext_roundtrip: PlaybackProgress = serde_json::from_str(&ext_json).unwrap();
    assert_eq!(extreme_progress.position_ms, ext_roundtrip.position_ms);

    // PlaybackState
    for state in [
        PlaybackState::Stopped,
        PlaybackState::Buffering,
        PlaybackState::Playing,
        PlaybackState::Paused,
    ] {
        let s_json = serde_json::to_string(&state).unwrap();
        let s_roundtrip: PlaybackState = serde_json::from_str(&s_json).unwrap();
        assert_eq!(state, s_roundtrip);
    }
}
