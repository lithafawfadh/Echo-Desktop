use tracing::{debug, warn};
use url::Url;

/// Strongly typed AST operations representing array manipulations performed by
/// YouTube's obfuscated player functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NOp {
    /// Inverts the array of characters in-place.
    Reverse,
    /// Swaps the characters at indices `(i % len, j % len)`.
    Swap(usize, usize),
    /// Removes `count` characters starting at index `start`.
    Splice(usize, usize),
    /// Truncates the array to the range `start..end`.
    Slice(usize, usize),
    /// Rotates characters left by `shift` positions (negative shifts rotate right).
    Rotate(isize),
}

/// Executes a sequence of `NOp` AST instructions on a mutable vector of characters in-place.
pub fn execute_n_ops_in_place(chars: &mut Vec<char>, ops: &[NOp]) {
    let len = chars.len();
    if len == 0 {
        return;
    }
    for op in ops {
        match *op {
            NOp::Reverse => chars.reverse(),
            NOp::Swap(i, j) => {
                if !chars.is_empty() {
                    let a = i % chars.len();
                    let b = j % chars.len();
                    chars.swap(a, b);
                }
            }
            NOp::Splice(start, count) => {
                if start < chars.len() {
                    let end = (start + count).min(chars.len());
                    chars.drain(start..end);
                }
            }
            NOp::Slice(start, end) => {
                let s = start.min(chars.len());
                let e = end.min(chars.len());
                if s < e {
                    *chars = chars[s..e].to_vec();
                }
            }
            NOp::Rotate(shift) => {
                let n = chars.len();
                if n > 0 {
                    let k = ((shift % n as isize + n as isize) as usize) % n;
                    chars.rotate_left(k);
                }
            }
        }
    }
}

/// Executes a sequence of `NOp` operations on a string token and returns the resulting string.
pub fn execute_n_ops(token: &str, ops: &[NOp]) -> String {
    let mut chars: Vec<char> = token.chars().collect();
    execute_n_ops_in_place(&mut chars, ops);
    chars.into_iter().collect()
}

/// Transforms the bandwidth throttling challenge parameter `n` using pure-Rust logic.
///
/// In YouTube's InnerTube player architecture, if dynamic AST operations for `n` are unavailable
/// or unparseable, this performs honest non-destructive pass-through fallback (Defense 2 per SPEC.md).
/// The raw token is preserved as received from YouTube, ensuring Google Video CDN delivers audio at
/// standard throttled rate (~400 kbps, comfortably exceeding the 128 kbps AAC stream requirement)
/// rather than returning HTTP 403 Forbidden on invalid/mangled tokens.
pub fn transform_n_token(token: &str) -> String {
    token.to_string()
}

/// Transforms the `n` query parameter in a Google Video stream URL using default pass-through fallback.
pub fn transform_stream_url_n(raw_url: &str) -> String {
    transform_stream_url_n_with_ops(raw_url, None)
}

/// Transforms the `n` query parameter in a Google Video stream URL in-place.
///
/// This implementation guarantees non-destructive replacement:
/// - If `n` is absent, or if the URL is malformed, the original URL is returned untouched.
/// - If `n` is transformed, ONLY the value token of the first `n` parameter is replaced.
/// - All other parameters (`sparams`, `sig`, `expire`, etc.) and their exact encodings
///   are preserved bit-for-bit, preventing CDN signature invalidation.
pub fn transform_stream_url_n_with_ops(raw_url: &str, ops: Option<&[NOp]>) -> String {
    // 1. Fast-path check: URL must contain the substring "n="
    if !raw_url.contains("n=") {
        return raw_url.to_string();
    }

    // 2. Syntax validation: URL must be syntactically valid per RFC 3986
    if Url::parse(raw_url).is_err() {
        warn!("Malformed stream URL passed to transform_stream_url_n; returning original");
        return raw_url.to_string();
    }

    // 3. Locate query string boundaries [q_start..q_end] within raw_url
    // Query string starts after the first '?' and ends before '#' (fragment) or EOF.
    let q_start = match raw_url.find('?') {
        Some(idx) => idx + 1,
        None => return raw_url.to_string(),
    };

    let q_end = match raw_url[q_start..].find('#') {
        Some(idx) => q_start + idx,
        None => raw_url.len(),
    };

    let query_str = &raw_url[q_start..q_end];

    // 4. Scan for the first parameter where key == "n"
    let mut offset = 0;
    let mut target_val_range = None;

    for part in query_str.split('&') {
        let part_len = part.len();
        if let Some(eq_idx) = part.find('=') {
            let key = &part[..eq_idx];
            if key == "n" {
                let val_start = q_start + offset + eq_idx + 1;
                let val_end = q_start + offset + part_len;
                target_val_range = Some((val_start, val_end));
                break;
            }
        }
        offset += part_len + 1; // +1 for the '&' delimiter
    }

    // 5. If no parameter key was "n", return untouched URL
    let (val_start, val_end) = match target_val_range {
        Some(range) => range,
        None => return raw_url.to_string(),
    };

    let raw_val = &raw_url[val_start..val_end];
    let transformed = match ops {
        Some(op_list) => execute_n_ops(raw_val, op_list),
        None => transform_n_token(raw_val),
    };

    // 6. If transform produced the same token, avoid reallocating or mutating
    if transformed == raw_val {
        debug!("n parameter unchanged by transform; returning original URL");
        return raw_url.to_string();
    }

    debug!(original = %raw_val, transformed = %transformed, "Transformed n throttling parameter");

    // 7. Splice the transformed token into the exact byte location
    let mut result = String::with_capacity(raw_url.len() - (val_end - val_start) + transformed.len());
    result.push_str(&raw_url[..val_start]);
    result.push_str(&transformed);
    result.push_str(&raw_url[val_end..]);
    result
}

/// Deobfuscates a streaming URL by applying the `n`-parameter throttling transform.
pub fn deobfuscate_stream_url(raw_url: &str) -> String {
    transform_stream_url_n(raw_url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_n_token_pass_through_fallback() {
        assert_eq!(transform_n_token("X_LCxVDjAavgE5t"), "X_LCxVDjAavgE5t");
        assert_eq!(transform_n_token("SLp9F5bwjAdhE9F-"), "SLp9F5bwjAdhE9F-");
        assert_eq!(transform_n_token("oBo2h5euWy6osrUt"), "oBo2h5euWy6osrUt");
        assert_eq!(transform_n_token("cu3wyu6LQn2hse"), "cu3wyu6LQn2hse");
        assert_eq!(transform_n_token("wvOFaY-yjgDuIEg5"), "wvOFaY-yjgDuIEg5");
        assert_eq!(transform_n_token("gK15nzVyaXE9RsMP3z"), "gK15nzVyaXE9RsMP3z");
    }

    #[test]
    fn test_n_op_ast_engine() {
        let ops = [
            NOp::Reverse,
            NOp::Swap(0, 3),
            NOp::Slice(0, 10),
            NOp::Rotate(2),
        ];
        let res = execute_n_ops("abcdefghijklmno", &ops);
        // "abcdefghijklmno" -> Reverse -> "onmlkjihgfedcba"
        // Swap(0, 3) -> "lnmokjihgfedcba"
        // Slice(0, 10) -> "lnmokjihgf"
        // Rotate(2) -> "mokjihgfln"
        assert_eq!(res, "mokjihgfln");
    }

    #[test]
    fn test_transform_stream_url_n_with_ops_replacement() {
        let url = "https://rr1---sn-xxx.googlevideo.com/videoplayback?itag=140&n=abcdef&sig=12345";
        let transformed = transform_stream_url_n_with_ops(url, Some(&[NOp::Reverse]));
        assert!(transformed.contains("n=fedcba"));
        assert!(transformed.contains("itag=140"));
        assert!(transformed.contains("sig=12345"));
    }

    #[test]
    fn test_transform_stream_url_n_pass_through_preservation() {
        let url = "https://rr1---sn-xxx.googlevideo.com/videoplayback?itag=140&n=X_LCxVDjAavgE5t&sig=12345";
        let transformed = transform_stream_url_n(url);
        assert_eq!(transformed, url);
    }

    #[test]
    fn test_transform_stream_url_n_missing_n() {
        let url = "https://rr1---sn-xxx.googlevideo.com/videoplayback?itag=140&sig=12345";
        let result = transform_stream_url_n(url);
        assert_eq!(result, url);
    }

    #[test]
    fn test_transform_stream_url_n_non_destructive_query() {
        let false_sub = "https://example.com/videoplayback?param=n=&itag=140";
        let res = transform_stream_url_n(false_sub);
        assert_eq!(res, false_sub);
    }
}
