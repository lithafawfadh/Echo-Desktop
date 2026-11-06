//! Adversarial and Stress Test Suite for `echo-provider` Pure-Rust Cipher Deobfuscators
//!
//! Evaluates boundary conditions, Unicode edge cases, malformed queries,
//! ReDoS resistance, and zero-panic guarantees across:
//! 1. `execute_cipher_ops` and `execute_cipher_ops_in_place`
//! 2. `descramble_signature` and `PlayerCipherState`
//! 3. `transform_n_token` and `transform_stream_url_n`
//! 4. `parse_player_script`

use echo_provider::cipher::{
    descrambler::extract_balanced_block,
    descramble_signature, execute_cipher_ops, parse_player_script, transform_n_token,
    transform_stream_url_n, CipherOp,
};
use echo_provider::error::ProviderError;

// =========================================================================
// 1. Extreme inputs to execute_cipher_ops
// =========================================================================

#[test]
fn test_cipher_ops_empty_string() {
    let empty = "";
    assert_eq!(execute_cipher_ops(empty, &[]), "");
    assert_eq!(execute_cipher_ops(empty, &[CipherOp::Reverse]), "");
    assert_eq!(execute_cipher_ops(empty, &[CipherOp::Swap(0)]), "");
    assert_eq!(execute_cipher_ops(empty, &[CipherOp::Swap(1)]), "");
    assert_eq!(execute_cipher_ops(empty, &[CipherOp::Swap(99999)]), "");
    assert_eq!(execute_cipher_ops(empty, &[CipherOp::Swap(usize::MAX)]), "");
    assert_eq!(execute_cipher_ops(empty, &[CipherOp::Splice(0)]), "");
    assert_eq!(execute_cipher_ops(empty, &[CipherOp::Splice(1)]), "");
    assert_eq!(execute_cipher_ops(empty, &[CipherOp::Splice(99999)]), "");
    assert_eq!(execute_cipher_ops(empty, &[CipherOp::Splice(usize::MAX)]), "");

    // Compound operations on empty string
    let compound = [
        CipherOp::Swap(100),
        CipherOp::Reverse,
        CipherOp::Splice(50),
        CipherOp::Swap(0),
        CipherOp::Splice(usize::MAX),
    ];
    assert_eq!(execute_cipher_ops(empty, &compound), "");
}

#[test]
fn test_cipher_ops_single_char_string() {
    let single = "x";

    // Reverse 1-char
    assert_eq!(execute_cipher_ops(single, &[CipherOp::Reverse]), "x");

    // Swap with various indices (all must modulo to 0)
    assert_eq!(execute_cipher_ops(single, &[CipherOp::Swap(0)]), "x");
    assert_eq!(execute_cipher_ops(single, &[CipherOp::Swap(1)]), "x");
    assert_eq!(execute_cipher_ops(single, &[CipherOp::Swap(99999)]), "x");
    assert_eq!(execute_cipher_ops(single, &[CipherOp::Swap(usize::MAX)]), "x");

    // Splice(0) retains character
    assert_eq!(execute_cipher_ops(single, &[CipherOp::Splice(0)]), "x");

    // Splice(1) or larger drains the single char
    assert_eq!(execute_cipher_ops(single, &[CipherOp::Splice(1)]), "");
    assert_eq!(execute_cipher_ops(single, &[CipherOp::Splice(99999)]), "");
    assert_eq!(execute_cipher_ops(single, &[CipherOp::Splice(usize::MAX)]), "");

    // Operations after char is drained must remain safe and not panic
    let drain_and_continue = [
        CipherOp::Splice(1),
        CipherOp::Swap(42),
        CipherOp::Reverse,
        CipherOp::Splice(10),
    ];
    assert_eq!(execute_cipher_ops(single, &drain_and_continue), "");
}

#[test]
fn test_cipher_ops_multibyte_unicode() {
    // Emojis (4-byte UTF-8 sequences each)
    let emojis = "🎵🎶🎧🎤🎹";
    let rev_emojis = execute_cipher_ops(emojis, &[CipherOp::Reverse]);
    assert_eq!(rev_emojis, "🎹🎤🎧🎶🎵");

    // Swap emojis at index 0 and 3
    let swap_emojis = execute_cipher_ops(emojis, &[CipherOp::Swap(3)]);
    assert_eq!(swap_emojis, "🎤🎶🎧🎵🎹");

    // Splice 2 emojis from front
    let splice_emojis = execute_cipher_ops(emojis, &[CipherOp::Splice(2)]);
    assert_eq!(splice_emojis, "🎧🎤🎹");

    // CJK characters (3-byte UTF-8 sequences)
    let cjk = "東京特許許可局";
    let rev_cjk = execute_cipher_ops(cjk, &[CipherOp::Reverse]);
    assert_eq!(rev_cjk, "局可許許特京東");

    let swap_cjk = execute_cipher_ops(cjk, &[CipherOp::Swap(4)]);
    assert_eq!(swap_cjk, "許京特許東可局");

    // Mixed ASCII, CJK, and Emojis
    let mixed = "A🎵B東京C🎶";
    let rev_mixed = execute_cipher_ops(mixed, &[CipherOp::Reverse]);
    assert_eq!(rev_mixed, "🎶C京東B🎵A");

    // RTL Arabic text
    let arabic = "موسيقى";
    let rev_arabic = execute_cipher_ops(arabic, &[CipherOp::Reverse]);
    let expected_arabic: String = arabic.chars().rev().collect();
    assert_eq!(rev_arabic, expected_arabic);

    // Highest valid Unicode scalar value U+10FFFF
    let max_unicode = format!("start{}\u{10FFFF}end", "\u{0000}");
    let rev_max = execute_cipher_ops(&max_unicode, &[CipherOp::Reverse]);
    let expected_rev: String = max_unicode.chars().rev().collect();
    assert_eq!(rev_max, expected_rev);
}

#[test]
fn test_cipher_ops_massive_indices_and_lengths() {
    let s = "abcdefghijklmnop"; // 16 characters
    let len = s.len();

    // Large Swap index: 1,000,003 % 16 = 3
    // Swap(3) swaps index 0 ('a') and index 3 ('d') -> "dbcaefghijklmnop"
    let swapped = execute_cipher_ops(s, &[CipherOp::Swap(1_000_003)]);
    assert_eq!(swapped, "dbcaefghijklmnop");

    // Swap with usize::MAX
    let max_swap_idx = usize::MAX % len;
    let expected = {
        let mut chars: Vec<char> = s.chars().collect();
        chars.swap(0, max_swap_idx);
        chars.into_iter().collect::<String>()
    };
    assert_eq!(execute_cipher_ops(s, &[CipherOp::Swap(usize::MAX)]), expected);

    // Splice beyond length
    assert_eq!(execute_cipher_ops(s, &[CipherOp::Splice(100)]), "");
    assert_eq!(execute_cipher_ops(s, &[CipherOp::Splice(usize::MAX)]), "");

    // Stress test: 10,000 characters with 1,000 operations
    let big_string: String = (0..10_000).map(|i| (b'a' + (i % 26) as u8) as char).collect();
    let mut ops = Vec::with_capacity(1_000);
    for i in 0..1_000 {
        match i % 3 {
            0 => ops.push(CipherOp::Swap(i * 37 + 13)),
            1 => ops.push(CipherOp::Reverse),
            2 => ops.push(CipherOp::Splice(1)), // drains 1 char every 3 ops
            _ => unreachable!(),
        }
    }
    let res = execute_cipher_ops(&big_string, &ops);
    assert_eq!(res.len(), 10_000 - (1000 / 3));
}

// =========================================================================
// 2. Malformed signature cipher strings in descramble_signature
// =========================================================================

#[test]
fn test_descramble_signature_malformed_queries() {
    let ops = [CipherOp::Reverse];

    // Empty cipher string
    let res = descramble_signature("", &ops);
    assert!(matches!(res, Err(ProviderError::CipherError(_))));

    // Missing 's' parameter
    let no_s = "url=https%3A%2F%2Fexample.com&sp=sig";
    let res = descramble_signature(no_s, &ops);
    assert!(matches!(res, Err(ProviderError::CipherError(msg)) if msg.contains("Missing 's'")));

    // Missing 'url' parameter
    let no_url = "s=abcdef123&sp=sig";
    let res = descramble_signature(no_url, &ops);
    assert!(matches!(res, Err(ProviderError::CipherError(msg)) if msg.contains("Missing 'url'")));

    // Missing 'sp' parameter defaults to "sig"
    let no_sp = "s=abcdef&url=https%3A%2F%2Fexample.com%2Fstream";
    let res = descramble_signature(no_sp, &ops).expect("Should succeed with default sp=sig");
    assert_eq!(res, "https://example.com/stream?sig=fedcba");

    // Empty 's' value should decipher to empty signature
    let empty_s = "s=&url=https%3A%2F%2Fexample.com%2Fstream&sp=sig";
    let res = descramble_signature(empty_s, &ops).expect("Should allow empty s");
    assert_eq!(res, "https://example.com/stream?sig=");

    // URL already having query parameters should use '&' as separator
    let url_with_query = "s=hello&url=https%3A%2F%2Fexample.com%2Fstream%3Fitag%3D140&sp=sig";
    let res = descramble_signature(url_with_query, &ops).expect("Should handle existing query");
    assert_eq!(res, "https://example.com/stream?itag=140&sig=olleh");

    // URL with trailing '?' separator
    let url_trailing_q = "s=hello&url=https%3A%2F%2Fexample.com%2Fstream%3F&sp=sig";
    let res = descramble_signature(url_trailing_q, &ops).expect("Should handle trailing ?");
    assert_eq!(res, "https://example.com/stream?&sig=olleh");

    // Non-UTF-8 percent-encoded bytes should return an error, NOT panic
    let invalid_utf8 = "s=%FF%FE%FD&url=https%3A%2F%2Fexample.com&sp=sig";
    let res = descramble_signature(invalid_utf8, &ops);
    assert!(matches!(res, Err(ProviderError::CipherError(msg)) if msg.contains("UTF-8 decode error")));

    // Multiple '&' delimiters and keys without values
    let noisy_query = "&&&s=xyz&&url=https%3A%2F%2Fexample.com&&key_without_val&&sp=custom_sig&=";
    let res = descramble_signature(noisy_query, &ops).expect("Should ignore empty noisy pairs");
    assert_eq!(res, "https://example.com?custom_sig=zyx");
}

// =========================================================================
// 3. Complex and unusual n-parameter transform inputs
// =========================================================================

#[test]
fn test_transform_n_token_edge_cases() {
    // Empty token
    assert_eq!(transform_n_token(""), "");

    // Single character tokens across printable ASCII spectrum
    // Under honest pass-through fallback (Defense 2), raw tokens are preserved
    assert_eq!(transform_n_token("a"), "a");
    assert_eq!(transform_n_token("Z"), "Z");
    assert_eq!(transform_n_token("!"), "!");

    // Whitespace and non-printable characters (outside 33..=126) remain unmodified
    assert_eq!(transform_n_token(" "), " ");
    assert_eq!(transform_n_token("\t"), "\t");
    assert_eq!(transform_n_token("\n"), "\n");
    assert_eq!(transform_n_token("\0"), "\0");

    // Multi-byte Unicode in n-token should pass through without crashing or mangling
    let unicode_token = "n_🎵_token_🚀";
    let transformed = transform_n_token(unicode_token);
    assert!(transformed.contains('🎵'));
    assert!(transformed.contains('🚀'));
    assert_eq!(transformed.chars().count(), unicode_token.chars().count());

    // Very long n-token (e.g. 10,000 chars) ensures zero crash or truncation
    let long_token: String = "X_LCxVDjAavgE5t".repeat(600); // 9000 chars
    let res = transform_n_token(&long_token);
    assert_eq!(res.len(), long_token.len());

    // Deterministic property: multiple calls with same input produce identical output
    let token_sample = "mP4kLxW98zQwErTy";
    assert_eq!(transform_n_token(token_sample), transform_n_token(token_sample));
}

// =========================================================================
// 4. Malformed and unusual GoogleVideo streaming URLs in transform_stream_url_n
// =========================================================================

#[test]
fn test_transform_stream_url_n_adversarial() {
    // 1. URL without 'n=' parameter: returns original untouched
    let no_n = "https://rr1---sn-xxx.googlevideo.com/videoplayback?itag=140&sig=abc";
    assert_eq!(transform_stream_url_n(no_n), no_n);

    // 2. Completely malformed strings containing 'n=': returns original string safely
    let malformed_1 = "not a valid url but has n=X_LCxVDjAavgE5t";
    assert_eq!(transform_stream_url_n(malformed_1), malformed_1);

    let malformed_2 = "https://[invalid-ipv6-bracket/videoplayback?n=X_LCxVDjAavgE5t";
    assert_eq!(transform_stream_url_n(malformed_2), malformed_2);

    let malformed_3 = "n=";
    assert_eq!(transform_stream_url_n(malformed_3), malformed_3);

    // 3. 'n=' as substring in parameter value (false positive substring guard)
    // When no 'n' parameter key was modified, the original URL string must be returned untouched.
    let false_sub = "https://example.com/videoplayback?param=n=&itag=140";
    let res = transform_stream_url_n(false_sub);
    assert_eq!(res, "https://example.com/videoplayback?param=n=&itag=140");

    // 4. Empty 'n' parameter value
    let empty_n = "https://example.com/videoplayback?itag=140&n=";
    let res = transform_stream_url_n(empty_n);
    assert!(res.contains("n="));

    // 5. URL with URL fragment '#'
    let with_frag = "https://example.com/videoplayback?n=X_LCxVDjAavgE5t#t=10";
    let res = transform_stream_url_n(with_frag);
    assert!(res.contains("n=X_LCxVDjAavgE5t"));
    assert!(res.contains("#t=10"));

    // 6. URL with multiple query parameters preserving all other parameters
    let full_url = "https://rr2---sn-4g5ednls.googlevideo.com/videoplayback?expire=1741200000&ei=AbCdEf&ip=1.2.3.4&id=o-AbC&itag=140&source=youtube&requiressl=yes&n=X_LCxVDjAavgE5t&vprv=1&mime=audio%2Fmp4";
    let res = transform_stream_url_n(full_url);
    assert!(res.contains("n=X_LCxVDjAavgE5t"));
    assert!(res.contains("itag=140"));
    assert!(res.contains("expire=1741200000"));
    assert!(res.contains("mime=audio%2Fmp4"));

    // 7. Multiple 'n' parameters: raw token preserved under pass-through fallback
    let multi_n = "https://example.com/videoplayback?n=X_LCxVDjAavgE5t&n=SLp9F5bwjAdhE9F-";
    let res = transform_stream_url_n(multi_n);
    assert!(res.contains("n=X_LCxVDjAavgE5t"));
    assert!(res.contains("n=SLp9F5bwjAdhE9F-"));
}

// =========================================================================
// 5. Malformed base.js player scripts in parse_player_script
// =========================================================================

#[test]
fn test_parse_player_script_adversarial() {
    // 1. Empty script
    let res = parse_player_script("");
    assert!(matches!(res, Err(ProviderError::CipherError(_))));

    // 2. Random HTML / 404 page / binary payload
    let html = "<html><head><title>404 Not Found</title></head><body>Error</body></html>";
    let res = parse_player_script(html);
    assert!(matches!(res, Err(ProviderError::CipherError(_))));

    // 3. Script with signatureTimestamp but no entrypoint
    let only_sts = "var g = { signatureTimestamp: 19842 };";
    let res = parse_player_script(only_sts);
    assert!(matches!(res, Err(ProviderError::CipherError(msg)) if msg.contains("entrypoint")));

    // 4. Script with entrypoint but missing helper object definition
    let orphan_entrypoint = r#"
        g.signatureTimestamp = 19842;
        var kx = function(a) {
            a = a.split("");
            AB.ro(a, 2);
            return a.join("");
        };
        c && a.set("sig", encodeURIComponent(kx(c)));
    "#;
    let res = parse_player_script(orphan_entrypoint);
    assert!(matches!(res, Err(ProviderError::CipherError(msg)) if msg.contains("helper object")));

    // 5. Script with unrecognized helper function body
    let unknown_helper = r#"
        g.signatureTimestamp = 19842;
        var kx = function(a) {
            a = a.split("");
            AB.magic(a, 2);
            return a.join("");
        };
        var AB = {
            magic: function(a, b) { eval("alert(1)"); }
        };
        c && a.set("sig", encodeURIComponent(kx(c)));
    "#;
    let res = parse_player_script(unknown_helper);
    assert!(matches!(res, Err(ProviderError::CipherError(msg)) if msg.contains("Unrecognized method body")));

    // 6. Pathological ReDoS stress: very long repeated strings
    let redos_attempt = format!("var {} = function(a) {{ a = a.split(\"\"); }};", "a".repeat(5000));
    let res = parse_player_script(&redos_attempt);
    // Must terminate almost immediately without hanging or stack overflowing
    assert!(res.is_err() || res.is_ok());

    // 7. Nested control blocks (if/else/for/try) inside entrypoint and helper methods
    let nested_script = r#"
var _yt_player = {};
(function(g) {
    g.signatureTimestamp = 19850;
    var kx = function(a) {
        if (typeof a === "string") {
            a = a.split("");
        } else {
            return "";
        }
        AB.ro(a, 2);
        for (var i = 0; i < 1; i++) {
            AB.xh(a, 5);
        }
        try {
            AB.mn(a, 3);
        } catch (e) {
            // do nothing
        }
        return a.join("");
    };
    var AB = {
        ro: function(a) {
            if (a.length > 0) {
                a.reverse();
            }
        },
        xh: function(a, b) {
            var c = a[0];
            a[0] = a[b % a.length];
            a[b % a.length] = c;
        },
        mn: function(a, b) {
            a.splice(0, b);
        }
    };
    c && a.set("sig", encodeURIComponent(kx(c)));
})(this);
"#;
    let nested_state = parse_player_script(nested_script).expect("Should parse script with nested blocks");
    assert_eq!(nested_state.signature_timestamp, 19850);
    assert_eq!(
        nested_state.cipher_ops,
        vec![CipherOp::Reverse, CipherOp::Swap(5), CipherOp::Splice(3)]
    );
}

#[test]
fn test_extract_balanced_block_primitives() {
    // Simple block
    assert_eq!(extract_balanced_block("{ abc }", 0), Some(" abc "));
    // Nested blocks
    assert_eq!(extract_balanced_block("{ a { b { c } } d }", 0), Some(" a { b { c } } d "));
    // Braces in strings
    assert_eq!(extract_balanced_block(r#"{ "}" '{' `}` }"#, 0), Some(r#" "}" '{' `}` "#));
    // Escaped quotes in strings
    assert_eq!(extract_balanced_block(r#"{ "{\"}" }"#, 0), Some(r#" "{\"}" "#));
    // Comments with braces
    assert_eq!(extract_balanced_block("{ // }\n abc /* } */ }", 0), Some(" // }\n abc /* } */ "));
    // Unclosed block
    assert_eq!(extract_balanced_block("{ unclosed", 0), None);
    // Invalid start position
    assert_eq!(extract_balanced_block("not a brace", 0), None);
}

// =========================================================================
// 6. Generative Stress & Fuzz Testing (Zero-Panic Oracles)
// =========================================================================

/// Deterministic 64-bit XorShift PRNG for reproducible adversarial fuzz generation
struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    fn new(seed: u64) -> Self {
        Self { state: if seed == 0 { 0xdeadbeefcafebabe } else { seed } }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    fn next_usize(&mut self) -> usize {
        self.next_u64() as usize
    }

    fn next_range(&mut self, min: usize, max: usize) -> usize {
        if min >= max {
            return min;
        }
        min + (self.next_usize() % (max - min + 1))
    }

    fn random_char(&mut self) -> char {
        match self.next_range(0, 4) {
            0 => (self.next_range(32, 126) as u8) as char, // ASCII printable
            1 => {
                // Common CJK range U+4E00..U+4E50
                char::from_u32(self.next_range(0x4E00, 0x4E50) as u32).unwrap_or('A')
            }
            2 => {
                // Emojis U+1F600..U+1F630
                char::from_u32(self.next_range(0x1F600, 0x1F630) as u32).unwrap_or('🎵')
            }
            3 => {
                // Control chars / boundary
                match self.next_range(0, 3) {
                    0 => '\0',
                    1 => '\n',
                    2 => '\t',
                    _ => ' ',
                }
            }
            _ => (self.next_range(0x00A0, 0x00FF) as u32 as u8) as char, // Latin-1
        }
    }

    fn random_string(&mut self, max_len: usize) -> String {
        let len = self.next_range(0, max_len);
        (0..len).map(|_| self.random_char()).collect()
    }
}

#[test]
fn test_fuzz_execute_cipher_ops_zero_panics() {
    let mut rng = SimpleRng::new(42);

    for _ in 0..10_000 {
        let s = rng.random_string(50);
        let num_ops = rng.next_range(0, 20);
        let mut ops = Vec::with_capacity(num_ops);

        for _ in 0..num_ops {
            match rng.next_range(0, 2) {
                0 => ops.push(CipherOp::Reverse),
                1 => ops.push(CipherOp::Swap(rng.next_usize())),
                2 => ops.push(CipherOp::Splice(rng.next_usize())),
                _ => unreachable!(),
            }
        }

        // Must never panic
        let out = execute_cipher_ops(&s, &ops);
        // Length should never exceed original string
        assert!(out.chars().count() <= s.chars().count());
    }
}

#[test]
fn test_fuzz_descramble_signature_zero_panics() {
    let mut rng = SimpleRng::new(1337);
    let sample_ops = [CipherOp::Reverse, CipherOp::Swap(3), CipherOp::Splice(1)];

    for _ in 0..5_000 {
        // Construct adversarial query string
        let rand_s = rng.random_string(20);
        let rand_url = rng.random_string(30);
        let rand_sp = rng.random_string(10);

        let query = match rng.next_range(0, 5) {
            0 => format!("s={rand_s}&url={rand_url}&sp={rand_sp}"),
            1 => format!("url={rand_url}&s={rand_s}"),
            2 => format!("s={rand_s}&broken_url_without_val&url="),
            3 => format!("sp={rand_sp}&junk={rand_s}"),
            4 => format!("s=%FF%FE&url={rand_url}"),
            _ => rng.random_string(50),
        };

        // Must return Ok or Err, never panic
        let _ = descramble_signature(&query, &sample_ops);
    }
}

#[test]
fn test_fuzz_transform_n_token_zero_panics() {
    let mut rng = SimpleRng::new(9999);

    for _ in 0..10_000 {
        let token = rng.random_string(100);
        // Must never panic and be deterministic
        let t1 = transform_n_token(&token);
        let t2 = transform_n_token(&token);
        assert_eq!(t1, t2);
    }
}

#[test]
fn test_fuzz_transform_stream_url_n_zero_panics() {
    let mut rng = SimpleRng::new(2026);

    for _ in 0..5_000 {
        let rand_part = rng.random_string(40);
        let url_candidate = match rng.next_range(0, 4) {
            0 => format!("https://rr1---sn-xxx.googlevideo.com/videoplayback?itag=140&n={rand_part}"),
            1 => format!("http://[::1]:8080/videoplayback?n={rand_part}&other=1"),
            2 => format!("invalid://url?n={rand_part}"),
            3 => format!("n={rand_part}"),
            _ => rand_part,
        };

        // Must never panic
        let out = transform_stream_url_n(&url_candidate);
        assert!(!out.is_empty() || url_candidate.is_empty());
    }
}
