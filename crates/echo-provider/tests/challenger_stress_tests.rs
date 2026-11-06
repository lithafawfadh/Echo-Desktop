//! Challenger Empirical Stress Test Suite for `echo-provider` Cipher Deobfuscators
//!
//! Written by challenger_m1_it2_1 to empirically stress-test:
//! 1. `transform_stream_url_n` query string non-destructive preservation,
//!    parameter value false-positive rejection (`param=n=`), and allocation behavior.
//! 2. `NOp` AST instruction execution and pass-through fallback with extreme inputs
//!    (empty, 1-char, Unicode, overflow-inducing indices, extreme rotations).
//! 3. Balanced brace extraction on complex minified JavaScript strings.

use echo_provider::cipher::{
    descrambler::extract_balanced_block,
    execute_n_ops, transform_n_token,
    transform_stream_url_n, transform_stream_url_n_with_ops, NOp,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

// Custom thread-local counting allocator to empirically observe heap allocations during URL transforms
struct CountingAlloc;

thread_local! {
    static TRACK_THIS_THREAD: Cell<bool> = const { Cell::new(false) };
    static THREAD_ALLOC_COUNT: Cell<usize> = const { Cell::new(0) };
}

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = TRACK_THIS_THREAD.try_with(|track| {
            if track.get() {
                let _ = THREAD_ALLOC_COUNT.try_with(|count| {
                    count.set(count.get() + 1);
                });
            }
        });
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

fn start_thread_alloc_tracking() {
    THREAD_ALLOC_COUNT.with(|c| c.set(0));
    TRACK_THIS_THREAD.with(|t| t.set(true));
}

fn stop_thread_alloc_tracking() -> usize {
    TRACK_THIS_THREAD.with(|t| t.set(false));
    THREAD_ALLOC_COUNT.with(|c| c.get())
}

// =========================================================================
// 1. Stress Tests for `transform_stream_url_n`
// =========================================================================

#[test]
fn test_transform_stream_url_n_param_equals_n_equals() {
    // Case 1: param=n= where 'n=' is in the value, not key
    let url1 = "https://rr1---sn-xxx.googlevideo.com/videoplayback?sparams=expire%2Cid%2Citag&param=n=&itag=140";
    let res1 = transform_stream_url_n(url1);
    assert_eq!(res1, url1, "URL with param=n= must return bit-for-bit identical string");
    assert!(!res1.contains("%3D"), "Must not encode '=' into '%3D'");

    // Case 2: multiple '=' characters in parameter value
    let url2 = "https://example.com/watch?token=xyz==&session=n=&itag=140";
    let res2 = transform_stream_url_n(url2);
    assert_eq!(res2, url2);

    // Case 3: key ends with n= (e.g. key is 'my_n=')
    let url3 = "https://example.com/stream?my_n=val&n_param=n=&other=1";
    let res3 = transform_stream_url_n(url3);
    assert_eq!(res3, url3);

    // Case 4: URL path contains 'n='
    let url4 = "https://example.com/api/n=/endpoint?format=mp4&itag=140";
    let res4 = transform_stream_url_n(url4);
    assert_eq!(res4, url4);

    // Case 5: 'n=' in fragment
    let url5 = "https://example.com/stream?itag=140#param=n=";
    let res5 = transform_stream_url_n(url5);
    assert_eq!(res5, url5);
}

#[test]
fn test_transform_stream_url_n_without_n_identity_and_allocations() {
    // Test URLs that do not contain 'n=' at all
    let urls = [
        "https://rr1---sn-xxx.googlevideo.com/videoplayback?itag=140&sig=abcdef12345",
        "https://example.com/stream",
        "https://example.com/stream?",
        "https://example.com/stream?a=1&b=2&c=3&sparams=expire%2Cid",
    ];

    for &url in &urls {
        start_thread_alloc_tracking();
        let res = transform_stream_url_n(url);
        let allocs = stop_thread_alloc_tracking();

        // 1. Bit-for-bit identity
        assert_eq!(res, url, "Must return exact unchanged URL");

        // 2. Allocation empirical observation:
        // Because the function returns an owned `String`, `url.to_string()` allocates exactly 1 String buffer.
        // It avoids parsing into Vec<(Cow, Cow)> or query pairs maps (which would be 10+ allocations).
        println!("Allocations for URL without 'n=' ({} bytes): {}", url.len(), allocs);
        assert_eq!(allocs, 1, "Fast-path without 'n=' should only allocate 1 String for the return value, observed {}", allocs);
    }
}

#[test]
fn test_transform_stream_url_n_with_real_n_token_replacement() {
    // Real n-token replaced with ops
    let url = "https://rr1---sn-xxx.googlevideo.com/videoplayback?expire=1741200000&ei=AbCdEf&itag=140&n=abcdef&sig=MEQCIF%2Fxyz%3D%3D&sparams=expire%2Cid%2Citag";
    
    // Reverse operation on 'abcdef' -> 'fedcba'
    let transformed = transform_stream_url_n_with_ops(url, Some(&[NOp::Reverse]));
    
    // Verify n replaced
    assert!(transformed.contains("n=fedcba"), "n token must be reversed");
    assert!(!transformed.contains("n=abcdef"), "old n token must not exist");

    // Verify all other parameters preserved bit-for-bit including percent-encodings
    assert!(transformed.contains("expire=1741200000"));
    assert!(transformed.contains("ei=AbCdEf"));
    assert!(transformed.contains("itag=140"));
    assert!(transformed.contains("sig=MEQCIF%2Fxyz%3D%3D"), "sig with %2F and %3D%3D must remain intact");
    assert!(transformed.contains("sparams=expire%2Cid%2Citag"), "sparams with %2C must remain intact");

    // Position verification: structure before and after n= must match exactly
    let prefix = "https://rr1---sn-xxx.googlevideo.com/videoplayback?expire=1741200000&ei=AbCdEf&itag=140&n=";
    let suffix = "&sig=MEQCIF%2Fxyz%3D%3D&sparams=expire%2Cid%2Citag";
    assert_eq!(transformed, format!("{prefix}fedcba{suffix}"));

    // Default pass-through fallback with real n=<token>
    let pass_through = transform_stream_url_n(url);
    assert_eq!(pass_through, url, "Default pass-through must preserve original URL exactly");
}

#[test]
fn test_transform_stream_url_n_boundary_positions() {
    // 1. n is the first query parameter
    let url_first = "https://example.com/stream?n=first_token&other=123";
    let res_first = transform_stream_url_n_with_ops(url_first, Some(&[NOp::Reverse]));
    assert_eq!(res_first, "https://example.com/stream?n=nekot_tsrif&other=123");

    // 2. n is the last query parameter
    let url_last = "https://example.com/stream?other=123&n=last_token";
    let res_last = transform_stream_url_n_with_ops(url_last, Some(&[NOp::Reverse]));
    assert_eq!(res_last, "https://example.com/stream?other=123&n=nekot_tsal");

    // 3. n is the sole query parameter
    let url_sole = "https://example.com/stream?n=only_token";
    let res_sole = transform_stream_url_n_with_ops(url_sole, Some(&[NOp::Reverse]));
    assert_eq!(res_sole, "https://example.com/stream?n=nekot_ylno");

    // 4. n with empty value
    let url_empty_n = "https://example.com/stream?itag=140&n=&other=abc";
    let res_empty_n = transform_stream_url_n_with_ops(url_empty_n, Some(&[NOp::Reverse]));
    assert_eq!(res_empty_n, url_empty_n);

    // 5. URL with query and fragment
    let url_frag = "https://example.com/stream?n=token123&itag=140#t=100";
    let res_frag = transform_stream_url_n_with_ops(url_frag, Some(&[NOp::Reverse]));
    assert_eq!(res_frag, "https://example.com/stream?n=321nekot&itag=140#t=100");
}

// =========================================================================
// 2. Stress Tests for `NOp` AST Execution and Fallback
// =========================================================================

#[test]
fn test_nop_ast_extreme_inputs_empty_and_single() {
    // Empty input with all NOp variants
    let all_ops = [
        NOp::Reverse,
        NOp::Swap(0, 0),
        NOp::Swap(100, 200),
        NOp::Swap(usize::MAX, usize::MAX),
        NOp::Splice(0, 0),
        NOp::Splice(0, 10),
        NOp::Slice(0, 0),
        NOp::Slice(0, 10),
        NOp::Rotate(0),
        NOp::Rotate(1),
        NOp::Rotate(-1),
        NOp::Rotate(isize::MAX),
        NOp::Rotate(isize::MIN),
    ];
    for op in all_ops {
        assert_eq!(execute_n_ops("", &[op]), "", "Empty string must remain empty for {:?}", op);
    }

    // Single character input
    let single = "Q";
    assert_eq!(execute_n_ops(single, &[NOp::Reverse]), "Q");
    assert_eq!(execute_n_ops(single, &[NOp::Swap(0, 0)]), "Q");
    assert_eq!(execute_n_ops(single, &[NOp::Swap(5, 10)]), "Q");
    assert_eq!(execute_n_ops(single, &[NOp::Swap(usize::MAX, usize::MAX)]), "Q");
    assert_eq!(execute_n_ops(single, &[NOp::Splice(0, 0)]), "Q");
    assert_eq!(execute_n_ops(single, &[NOp::Splice(0, 1)]), "");
    assert_eq!(execute_n_ops(single, &[NOp::Splice(0, 100)]), "");
    assert_eq!(execute_n_ops(single, &[NOp::Rotate(1)]), "Q");
    assert_eq!(execute_n_ops(single, &[NOp::Rotate(-1)]), "Q");
    assert_eq!(execute_n_ops(single, &[NOp::Rotate(isize::MAX)]), "Q");
    assert_eq!(execute_n_ops(single, &[NOp::Rotate(isize::MIN)]), "Q");
}

#[test]
fn test_nop_ast_unicode_stress() {
    // 1. Japanese text
    let jp = "音楽プレイヤー";
    let rev_jp = execute_n_ops(jp, &[NOp::Reverse]);
    let expected_jp: String = jp.chars().rev().collect();
    assert_eq!(rev_jp, expected_jp);

    // 2. Emojis (4 bytes each)
    let emojis = "🔥🎵🎧🎹🚀";
    let swapped = execute_n_ops(emojis, &[NOp::Swap(0, 4)]);
    assert_eq!(swapped, "🚀🎵🎧🎹🔥");

    let rotated = execute_n_ops(emojis, &[NOp::Rotate(1)]);
    assert_eq!(rotated, "🎵🎧🎹🚀🔥");

    let rotated_right = execute_n_ops(emojis, &[NOp::Rotate(-1)]);
    assert_eq!(rotated_right, "🚀🔥🎵🎧🎹");

    // 3. Multi-byte RTL Arabic
    let arabic = "صوت";
    let rev_ar = execute_n_ops(arabic, &[NOp::Reverse]);
    let expected: String = arabic.chars().rev().collect();
    assert_eq!(rev_ar, expected);

    // 4. Unicode scalar boundaries
    let high = "\u{10FFFF}\u{10000}\u{0000}A";
    let rev_high = execute_n_ops(high, &[NOp::Reverse]);
    let expected_high: String = high.chars().rev().collect();
    assert_eq!(rev_high, expected_high);
}

#[test]
fn test_nop_ast_rotations() {
    let s = "abcdefgh"; // len 8
    // Rotate left 1
    assert_eq!(execute_n_ops(s, &[NOp::Rotate(1)]), "bcdefgha");
    // Rotate left 8 (full cycle)
    assert_eq!(execute_n_ops(s, &[NOp::Rotate(8)]), "abcdefgh");
    // Rotate left 9 (equivalent to 1)
    assert_eq!(execute_n_ops(s, &[NOp::Rotate(9)]), "bcdefgha");
    // Rotate right 1 (shift -1)
    assert_eq!(execute_n_ops(s, &[NOp::Rotate(-1)]), "habcdefg");
    // Rotate right 8
    assert_eq!(execute_n_ops(s, &[NOp::Rotate(-8)]), "abcdefgh");
    // Extreme shifts: isize::MAX and isize::MIN must not panic
    let _ = execute_n_ops(s, &[NOp::Rotate(isize::MAX)]);
    let _ = execute_n_ops(s, &[NOp::Rotate(isize::MIN)]);
}

#[test]
fn test_nop_ast_splice_and_slice_bounds() {
    let s = "0123456789";
    // Splice from middle
    assert_eq!(execute_n_ops(s, &[NOp::Splice(3, 4)]), "012789");
    // Splice starting out of bounds
    assert_eq!(execute_n_ops(s, &[NOp::Splice(20, 5)]), "0123456789");
    // Splice count exceeding remaining length
    assert_eq!(execute_n_ops(s, &[NOp::Splice(5, 50)]), "01234");
    // Slice sub-range
    assert_eq!(execute_n_ops(s, &[NOp::Slice(2, 6)]), "2345");
    // Slice out of bounds end
    assert_eq!(execute_n_ops(s, &[NOp::Slice(8, 20)]), "89");
    // Slice start >= end (no-op)
    assert_eq!(execute_n_ops(s, &[NOp::Slice(5, 5)]), "0123456789");
    assert_eq!(execute_n_ops(s, &[NOp::Slice(8, 3)]), "0123456789");
}

#[test]
fn test_nop_ast_potential_overflow_conditions() {
    let s = "abcdef";
    // Swap with usize::MAX
    let res = execute_n_ops(s, &[NOp::Swap(usize::MAX, usize::MAX)]);
    assert_eq!(res, "abcdef");

    // Slice with usize::MAX
    let res = execute_n_ops(s, &[NOp::Slice(0, usize::MAX)]);
    assert_eq!(res, "abcdef");

    // Splice starting at 0 with usize::MAX
    let res = execute_n_ops(s, &[NOp::Splice(0, usize::MAX)]);
    assert_eq!(res, "");

    // Splice with large count that doesn't overflow (usize::MAX - 10) succeeds
    let res_safe = execute_n_ops(s, &[NOp::Splice(1, usize::MAX - 10)]);
    assert_eq!(res_safe, "a");

    // EMPIRICAL BUG REPRODUCTION:
    // Splice starting at 1 with count = usize::MAX triggers unchecked arithmetic overflow:
    // `(start + count)` in n_transform.rs:38 panics with 'attempt to add with overflow'.
    let panic_res = std::panic::catch_unwind(|| {
        execute_n_ops(s, &[NOp::Splice(1, usize::MAX)])
    });
    assert!(
        panic_res.is_err(),
        "CONFIRMED BUG: NOp::Splice(1, usize::MAX) panics on unchecked arithmetic overflow (start + count)"
    );
}

// =========================================================================
// 3. Stress Tests for Balanced Brace Extraction
// =========================================================================

#[test]
fn test_extract_balanced_block_complex_minified_js() {
    // 1. Complex nested function with control flow and string literals containing braces
    let js_fn = r#"{
        if (a === "{") { return "}"; }
        for (var i = 0; i < 5; i++) {
            try {
                a = a.split("");
                AB.ro(a, 2);
            } catch(e) {
                console.log("error: }");
            }
        }
        return a.join("");
    }"#;
    let extracted = extract_balanced_block(js_fn, 0);
    assert!(extracted.is_some());
    let body = extracted.unwrap();
    assert!(body.contains(r#"if (a === "{")"#));
    assert!(body.contains("AB.ro(a, 2);"));
    assert!(body.ends_with(r#"return a.join("");
    "#));

    // 2. Helper object with quotes, template strings, comments and escaped quotes
    let helper_obj = r#"{
        ro: function(a) { a.reverse(); /* reverse } comment */ },
        xh: function(a, b) {
            // single-line } comment
            var quote = "escaped \"} quote";
            var tmpl = `template } brace`;
            var c = a[0]; a[0] = a[b % a.length]; a[b % a.length] = c;
        },
        mn: function(a, b) { a.splice(0, b); }
    }"#;
    let extracted_obj = extract_balanced_block(helper_obj, 0);
    assert!(extracted_obj.is_some());
    let obj_body = extracted_obj.unwrap();
    assert!(obj_body.contains("ro: function"));
    assert!(obj_body.contains("xh: function"));
    assert!(obj_body.contains("mn: function"));

    // 3. Deeply nested braces (depth = 100)
    let deep_open = "{".repeat(100);
    let deep_close = "}".repeat(100);
    let deep_script = format!("{deep_open} inner_code {deep_close}");
    let extracted_deep = extract_balanced_block(&deep_script, 0);
    assert!(extracted_deep.is_some());
    assert!(extracted_deep.unwrap().contains("inner_code"));

    // 4. Adversarial unclosed brace
    let unclosed = "{ var a = 1; if (a) { return a; ";
    assert_eq!(extract_balanced_block(unclosed, 0), None);

    // 5. Unclosed string quote
    let unclosed_str = r#"{ var s = "unclosed string; }"#;
    assert_eq!(extract_balanced_block(unclosed_str, 0), None);

    // 6. Non-brace open position
    let not_brace = "var x = { a: 1 };";
    assert_eq!(extract_balanced_block(not_brace, 0), None);
    assert!(extract_balanced_block(not_brace, 8).is_some());
}

#[test]
fn test_pass_through_fallback_oracle() {
    let test_tokens = [
        "X_LCxVDjAavgE5t",
        "SLp9F5bwjAdhE9F-",
        "oBo2h5euWy6osrUt",
        "cu3wyu6LQn2hse",
        "wvOFaY-yjgDuIEg5",
        "gK15nzVyaXE9RsMP3z",
        "",
        "a",
        "1234567890",
        "!@#$%^&*()_+-=",
        "Unicode_🎵_Token_🚀",
    ];

    for &token in &test_tokens {
        let transformed = transform_n_token(token);
        assert_eq!(transformed, token, "Pass-through fallback must return identical token without modification");
    }
}
