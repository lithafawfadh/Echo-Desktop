use echo_core::AudioCodec;
use echo_provider::cipher::{
    descramble_signature, execute_cipher_ops, execute_n_ops, parse_player_script,
    transform_n_token, transform_stream_url_n, transform_stream_url_n_with_ops, CipherOp, NOp,
    PlayerCipherState, YouTubeCipherEngine,
};
use echo_provider::error::ProviderError;
use echo_provider::innertube::{
    parse_duration, parse_player_response, parse_search_response, resolve_candidate_to_stream,
    select_itag_140_stream, InnerTubePlayerResponse, RawStreamCandidate,
};
use serde_json::json;

const SEARCH_RESPONSE_JSON: &str = include_str!("fixtures/search_response.json");
const SEARCH_EMPTY_JSON: &str = include_str!("fixtures/search_empty.json");
const PLAYER_DIRECT_JSON: &str = include_str!("fixtures/player_response_direct.json");
const PLAYER_CIPHER_JSON: &str = include_str!("fixtures/player_response_cipher.json");
const SAMPLE_BASE_JS: &str = include_str!("fixtures/sample_base.js");

#[test]
fn test_duration_parsing() {
    assert_eq!(parse_duration("0:05"), 5);
    assert_eq!(parse_duration("3:45"), 225);
    assert_eq!(parse_duration("4:08"), 248);
    assert_eq!(parse_duration("1:24:15"), 5055);
    assert_eq!(parse_duration("invalid"), 0);
    assert_eq!(parse_duration("LIVE"), 0);
    assert_eq!(parse_duration(""), 0);
}

#[test]
fn test_search_response_parsing() {
    let json_val: serde_json::Value = serde_json::from_str(SEARCH_RESPONSE_JSON).unwrap();
    let tracks = parse_search_response(&json_val).unwrap();

    assert_eq!(tracks.len(), 3);

    // Track 0: Get Lucky
    assert_eq!(tracks[0].id, "5NV6Rdv1a3I");
    assert_eq!(tracks[0].title, "Get Lucky");
    assert_eq!(tracks[0].artist, "Daft Punk");
    assert_eq!(tracks[0].album, Some("Random Access Memories".to_string()));
    assert_eq!(tracks[0].duration_seconds, 248);
    assert_eq!(
        tracks[0].thumbnail_url,
        Some("https://lh3.googleusercontent.com/cover_large.jpg=w544-h544-l90-rj".to_string())
    );

    // Track 1: Instant Crush
    assert_eq!(tracks[1].id, "a5uQMwRMHcs");
    assert_eq!(tracks[1].title, "Instant Crush");
    assert_eq!(tracks[1].artist, "Daft Punk");
    assert_eq!(tracks[1].album, Some("Random Access Memories".to_string()));
    assert_eq!(tracks[1].duration_seconds, 337);

    // Track 2: Alive 2007 (Full Concert: 1:24:15 = 5055s)
    assert_eq!(tracks[2].id, "live_concert_01");
    assert_eq!(tracks[2].title, "Alive 2007 (Full Concert)");
    assert_eq!(tracks[2].artist, "Daft Punk");
    assert_eq!(tracks[2].album, None);
    assert_eq!(tracks[2].duration_seconds, 5055);
}

#[test]
fn test_search_empty_response() {
    let json_val: serde_json::Value = serde_json::from_str(SEARCH_EMPTY_JSON).unwrap();
    let tracks = parse_search_response(&json_val).unwrap();
    assert!(tracks.is_empty());
}

#[test]
fn test_player_response_itag_140_priority() {
    let json_val: serde_json::Value = serde_json::from_str(PLAYER_DIRECT_JSON).unwrap();
    let player_resp: InnerTubePlayerResponse = serde_json::from_value(json_val).unwrap();

    // Verify itag 140 is selected over itag 251 to ensure Symphonia decoder compatibility
    let candidate = select_itag_140_stream(&player_resp, "5NV6Rdv1a3I").unwrap();
    match candidate {
        RawStreamCandidate::Direct {
            url,
            bitrate_kbps,
            sample_rate_hz,
        } => {
            assert!(url.contains("itag=140"));
            assert_eq!(bitrate_kbps, 128);
            assert_eq!(sample_rate_hz, 44100);

            // Resolve to AudioStream and verify n-throttling transform is applied
            let stream = resolve_candidate_to_stream(
                RawStreamCandidate::Direct {
                    url,
                    bitrate_kbps,
                    sample_rate_hz,
                },
                None,
            )
            .unwrap();

            assert_eq!(stream.codec, AudioCodec::Aac);
            assert_eq!(stream.bitrate_kbps, 128);
            assert_eq!(stream.sample_rate_hz, 44100);
            assert!(!stream.is_lossless);
            assert!(stream.url.contains("n=X_LCxVDjAavgE5t")); // Raw token preserved under honest pass-through
        }
        RawStreamCandidate::Cipher { .. } => panic!("Expected Direct candidate"),
    }
}

#[test]
fn test_parse_player_response_format() {
    let json_val: serde_json::Value = serde_json::from_str(PLAYER_DIRECT_JSON).unwrap();
    let format = parse_player_response(&json_val).unwrap();
    // Prioritizes itag 140
    assert_eq!(format.itag, 140);
    assert!(format.url.is_some());
}

#[test]
fn test_cipher_ops_execution() {
    // 1. Reverse
    let res = execute_cipher_ops("hello", &[CipherOp::Reverse]);
    assert_eq!(res, "olleh");

    // 2. Swap(2)
    let res = execute_cipher_ops("abcde", &[CipherOp::Swap(2)]);
    assert_eq!(res, "cbade");

    // 3. Splice(2)
    let res = execute_cipher_ops("abcde", &[CipherOp::Splice(2)]);
    assert_eq!(res, "cde");

    // 4. Vector S1: Synthetic Mock Pipeline
    // "abcdefghij" -> Swap(2) -> Reverse -> Splice(3) -> "gfedabc"
    let ops = [CipherOp::Swap(2), CipherOp::Reverse, CipherOp::Splice(3)];
    let res = execute_cipher_ops("abcdefghij", &ops);
    assert_eq!(res, "gfedabc");

    // 5. Vector S2: Production YouTube Player Signature
    let s2_input = "2ACFC7A61CA478CD21425E5A57EBD73DDC78E22A.2094302436B2D377D14A3BBA23022D023B8BC25AA";
    let s2_ops = [CipherOp::Reverse, CipherOp::Splice(1)];
    let s2_res = execute_cipher_ops(s2_input, &s2_ops);
    assert_eq!(
        s2_res,
        "A52CB8B320D22032ABB3A41D773D2B6342034902.A22E87CDD37DBE75A5E52412DC874AC16A7CFCA2"
    );
}

#[test]
fn test_base_js_ast_parser() {
    let state = parse_player_script(SAMPLE_BASE_JS).unwrap();

    assert_eq!(state.signature_timestamp, 19842);
    assert_eq!(
        state.cipher_ops,
        vec![CipherOp::Reverse, CipherOp::Swap(5), CipherOp::Splice(3)]
    );
}

#[test]
fn test_descramble_signature_full() {
    let state = parse_player_script(SAMPLE_BASE_JS).unwrap();
    let json_val: serde_json::Value = serde_json::from_str(PLAYER_CIPHER_JSON).unwrap();
    let player_resp: InnerTubePlayerResponse = serde_json::from_value(json_val).unwrap();

    let candidate = select_itag_140_stream(&player_resp, "test_cipher_vid").unwrap();
    match candidate {
        RawStreamCandidate::Cipher {
            cipher_str,
            bitrate_kbps,
            sample_rate_hz,
        } => {
            let stream = resolve_candidate_to_stream(
                RawStreamCandidate::Cipher {
                    cipher_str,
                    bitrate_kbps,
                    sample_rate_hz,
                },
                Some(&state.cipher_ops),
            )
            .unwrap();

            assert_eq!(stream.codec, AudioCodec::Aac);
            assert!(stream.url.starts_with("https://rr1---sn-xxx.googlevideo.com/videoplayback"));
            assert!(stream.url.contains("&sig="));
            assert!(stream.url.contains("n=X_LCxVDjAavgE5t")); // Raw token preserved under honest pass-through
        }
        RawStreamCandidate::Direct { .. } => panic!("Expected Cipher candidate"),
    }
}

#[test]
fn test_descramble_signature_direct_call() {
    let ops = [CipherOp::Reverse, CipherOp::Swap(5), CipherOp::Splice(3)];
    let cipher_str = "s=abcdef1234567890&sp=sig&url=https%3A%2F%2Frr1---sn-xxx.googlevideo.com%2Fvideoplayback%3Fitag%3D140";
    let url = descramble_signature(cipher_str, &ops).unwrap();
    assert!(url.starts_with("https://rr1---sn-xxx.googlevideo.com/videoplayback?itag=140&sig="));
}

#[tokio::test]
async fn test_cipher_engine_caching() {
    let engine = YouTubeCipherEngine::new();
    assert!(engine.get_state().await.is_none());

    let state = PlayerCipherState {
        signature_timestamp: 19842,
        cipher_ops: vec![CipherOp::Reverse],
    };
    engine.set_state(state.clone()).await;

    let cached = engine.get_state().await;
    assert_eq!(cached, Some(state));

    engine.invalidate().await;
    assert!(engine.get_state().await.is_none());
}

#[test]
fn test_n_parameter_throttling_vectors_n1_to_n6() {
    // Under honest pass-through fallback (Defense 2), raw tokens are preserved without corruption
    assert_eq!(transform_n_token("X_LCxVDjAavgE5t"), "X_LCxVDjAavgE5t");
    assert_eq!(transform_n_token("SLp9F5bwjAdhE9F-"), "SLp9F5bwjAdhE9F-");
    assert_eq!(transform_n_token("oBo2h5euWy6osrUt"), "oBo2h5euWy6osrUt");
    assert_eq!(transform_n_token("cu3wyu6LQn2hse"), "cu3wyu6LQn2hse");
    assert_eq!(transform_n_token("wvOFaY-yjgDuIEg5"), "wvOFaY-yjgDuIEg5");
    assert_eq!(transform_n_token("gK15nzVyaXE9RsMP3z"), "gK15nzVyaXE9RsMP3z");
}

#[test]
fn test_n_op_ast_engine_execution() {
    let ops = [
        NOp::Reverse,
        NOp::Swap(0, 3),
        NOp::Slice(0, 10),
        NOp::Rotate(2),
    ];
    let res = execute_n_ops("abcdefghijklmno", &ops);
    assert_eq!(res, "mokjihgfln");
}

#[test]
fn test_transform_stream_url_n() {
    let raw_url = "https://rr1---sn-test.googlevideo.com/videoplayback?expire=1741200000&itag=140&n=X_LCxVDjAavgE5t";
    let transformed = transform_stream_url_n(raw_url);
    assert_eq!(
        transformed,
        "https://rr1---sn-test.googlevideo.com/videoplayback?expire=1741200000&itag=140&n=X_LCxVDjAavgE5t"
    );

    // Explicit NOp execution replaces the n parameter in-place
    let with_ops = transform_stream_url_n_with_ops(raw_url, Some(&[NOp::Reverse]));
    assert!(with_ops.contains("n=t5EgvaAjDVxCL_X"));
    assert!(with_ops.contains("itag=140"));
}

#[test]
fn test_playability_error() {
    let restricted_json = json!({
        "playabilityStatus": {
            "status": "LOGIN_REQUIRED",
            "reason": "Sign in to confirm you’re not a bot"
        }
    });

    let resp: InnerTubePlayerResponse = serde_json::from_value(restricted_json).unwrap();
    let err = select_itag_140_stream(&resp, "blocked_track").unwrap_err();

    match err {
        ProviderError::PlayabilityError { status, reason } => {
            assert_eq!(status, "LOGIN_REQUIRED");
            assert_eq!(reason, "Sign in to confirm you’re not a bot");
        }
        _ => panic!("Expected PlayabilityError variant"),
    }
}
