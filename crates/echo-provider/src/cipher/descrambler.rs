use super::ops::{execute_cipher_ops, CipherOp};
use crate::error::ProviderError;
use percent_encoding::percent_decode_str;
use regex::Regex;
use std::collections::HashMap;

/// Parsed cipher deobfuscation state extracted from YouTube's `base.js` player script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerCipherState {
    pub signature_timestamp: u32,
    pub cipher_ops: Vec<CipherOp>,
}

pub const DEFAULT_SIGNATURE_TIMESTAMP: u32 = 19842;

/// Regular expressions for extracting signatureTimestamp (sts) from base.js.
pub const STS_PATTERNS: &[&str] = &[
    r#"signatureTimestamp\s*[:=]\s*(\d+)"#,
    r#"\bsts\s*[:=]\s*(\d+)"#,
];

/// Regular expressions for locating the signature descrambling entrypoint function in base.js.
pub const SIG_ENTRYPOINT_PATTERNS: &[&str] = &[
    r#"\b[cs]\s*&&\s*[adf]\.set\([^,]+\s*,\s*encodeURIComponent\s*\(\s*([a-zA-Z0-9$]+)\("#,
    r#"\b[a-zA-Z0-9$]+\s*&&\s*[a-zA-Z0-9$]+\.set\([^,]+\s*,\s*encodeURIComponent\s*\(\s*([a-zA-Z0-9$]+)\("#,
    r#"\bc\s*&&\s*[a-zA-Z0-9$]+\.set\([^,]+\s*,\s*\([a-zA-Z0-9$]+\s*=\s*([a-zA-Z0-9$]+)\([a-zA-Z0-9$]+\)\)"#,
    r#"([a-zA-Z0-9$]{2,3})\s*=\s*function\([a-zA-Z0-9$]+\)\{[a-zA-Z0-9$]+=[a-zA-Z0-9$]+\.split\(""\);"#,
    r#"(?:var\s+)?([a-zA-Z0-9$]+)\s*=\s*function\([a-zA-Z0-9$]+\)\s*\{\s*[a-zA-Z0-9$]+\s*=\s*[a-zA-Z0-9$]+\.split\(""\);"#,
];

#[derive(Copy, Clone, PartialEq, Eq)]
enum ScanState {
    Code,
    Str(u8),
    LineComment,
    BlockComment,
}

/// Extracts the balanced block between an opening brace '{' at `open_pos` and its matching closing brace '}',
/// safely handling nested braces, string literals ('"', '\'', '`'), escape characters, and comments.
pub fn extract_balanced_block(source: &str, open_pos: usize) -> Option<&str> {
    let bytes = source.as_bytes();
    if open_pos >= bytes.len() || bytes[open_pos] != b'{' {
        return None;
    }
    let mut depth = 0;
    let mut state = ScanState::Code;
    let mut escaped = false;
    let mut i = open_pos;
    let start_content = open_pos + 1;

    while i < bytes.len() {
        let b = bytes[i];
        match state {
            ScanState::Code => {
                if b == b'"' || b == b'\'' || b == b'`' {
                    state = ScanState::Str(b);
                } else if b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                    state = ScanState::LineComment;
                    i += 2;
                    continue;
                } else if b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                    state = ScanState::BlockComment;
                    i += 2;
                    continue;
                } else if b == b'{' {
                    depth += 1;
                } else if b == b'}' {
                    depth -= 1;
                    if depth == 0 {
                        return Some(&source[start_content..i]);
                    }
                }
            }
            ScanState::Str(quote) => {
                if escaped {
                    escaped = false;
                } else if b == b'\\' {
                    escaped = true;
                } else if b == quote {
                    state = ScanState::Code;
                }
            }
            ScanState::LineComment => {
                if b == b'\n' {
                    state = ScanState::Code;
                }
            }
            ScanState::BlockComment => {
                if b == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                    state = ScanState::Code;
                    i += 2;
                    continue;
                }
            }
        }
        i += 1;
    }
    None
}

/// Parses YouTube player script (`base.js`) AST to extract the signature timestamp
/// and the ordered sequence of `CipherOp` operations.
pub fn parse_player_script(script: &str) -> Result<PlayerCipherState, ProviderError> {
    // 1. Extract signatureTimestamp
    let mut signature_timestamp = DEFAULT_SIGNATURE_TIMESTAMP;
    for pattern in STS_PATTERNS {
        if let Ok(re) = Regex::new(pattern) {
            if let Some(caps) = re.captures(script) {
                if let Some(m) = caps.get(1) {
                    if let Ok(ts) = m.as_str().parse::<u32>() {
                        signature_timestamp = ts;
                        break;
                    }
                }
            }
        }
    }

    // 2. Locate entrypoint function name
    let mut entrypoint_fn: Option<String> = None;
    for pattern in SIG_ENTRYPOINT_PATTERNS {
        if let Ok(re) = Regex::new(pattern) {
            if let Some(caps) = re.captures(script) {
                if let Some(m) = caps.get(1) {
                    entrypoint_fn = Some(m.as_str().to_string());
                    break;
                }
            }
        }
    }

    let fn_name = match entrypoint_fn {
        Some(name) => name,
        None => {
            return Err(ProviderError::CipherError(
                "Failed to locate signature descrambler entrypoint function in player script".into(),
            ));
        }
    };

    // 3. Extract the body of the entrypoint function
    let fn_header_pattern = format!(
        r#"(?:(?:var\s+)?{}\s*=\s*function|function\s+{})(?:\s+[a-zA-Z0-9$]+)?\s*\([^)]*\)\s*\{{"#,
        regex::escape(&fn_name),
        regex::escape(&fn_name)
    );
    let fn_header_re = Regex::new(&fn_header_pattern).map_err(|e| {
        ProviderError::CipherError(format!("Invalid regex for entrypoint header: {}", e))
    })?;

    let fn_match = fn_header_re.find(script).ok_or_else(|| {
        ProviderError::CipherError(format!(
            "Failed to locate function header for entrypoint {}",
            fn_name
        ))
    })?;

    let fn_body = extract_balanced_block(script, fn_match.end() - 1).ok_or_else(|| {
        ProviderError::CipherError(format!(
            "Failed to extract balanced function body for entrypoint {}",
            fn_name
        ))
    })?;

    // 4. Parse method calls in the function body
    // e.g. AB.ro(a, 2); AB["xh"](a, 31); AB.mn(a, 3);
    let call_re = Regex::new(
        r#"([a-zA-Z0-9$]+)(?:\.([a-zA-Z0-9$]+)|\[["']([^"']+)["']\])\([a-zA-Z0-9$]+(?:,\s*(\d+|0x[0-9a-fA-F]+))?\)"#,
    )
    .map_err(|e| ProviderError::CipherError(format!("Invalid call regex: {}", e)))?;

    let mut raw_calls = Vec::new();
    let mut helper_obj_name: Option<String> = None;

    for caps in call_re.captures_iter(fn_body) {
        let obj = caps.get(1).map(|m| m.as_str().to_string());
        let method = caps
            .get(2)
            .or_else(|| caps.get(3))
            .map(|m| m.as_str().to_string());
        let arg = caps
            .get(4)
            .and_then(|m| {
                let s = m.as_str();
                if s.starts_with("0x") || s.starts_with("0X") {
                    usize::from_str_radix(&s[2..], 16).ok()
                } else {
                    s.parse::<usize>().ok()
                }
            })
            .unwrap_or(0);

        if let (Some(obj_name), Some(meth_name)) = (obj, method) {
            if helper_obj_name.is_none() {
                helper_obj_name = Some(obj_name.clone());
            }
            raw_calls.push((obj_name, meth_name, arg));
        }
    }

    if raw_calls.is_empty() {
        return Err(ProviderError::CipherError(format!(
            "No cipher helper calls found in entrypoint {}",
            fn_name
        )));
    }

    let helper_name = helper_obj_name.ok_or_else(|| {
        ProviderError::CipherError("Failed to identify cipher helper object name".into())
    })?;

    // 5. Extract the helper object definition
    let obj_header_pattern = format!(
        r#"(?:var\s+)?{}\s*=\s*\{{"#,
        regex::escape(&helper_name)
    );
    let obj_header_re = Regex::new(&obj_header_pattern).map_err(|e| {
        ProviderError::CipherError(format!("Invalid helper object regex: {}", e))
    })?;

    let obj_match = obj_header_re.find(script).ok_or_else(|| {
        ProviderError::CipherError(format!(
            "Failed to find helper object definition for {}",
            helper_name
        ))
    })?;

    let obj_body = extract_balanced_block(script, obj_match.end() - 1).ok_or_else(|| {
        ProviderError::CipherError(format!(
            "Failed to extract balanced helper object definition for {}",
            helper_name
        ))
    })?;

    // 6. Map each method in the helper object to its operation type
    let mut method_ops = HashMap::new();
    for (_, meth_name, _) in &raw_calls {
        if method_ops.contains_key(meth_name) {
            continue;
        }

        let meth_header_pattern = format!(
            r#"(?:{}|["']{}["'])\s*:\s*function\s*\([^)]*\)\s*\{{"#,
            regex::escape(meth_name),
            regex::escape(meth_name)
        );
        let meth_header_re = Regex::new(&meth_header_pattern).map_err(|e| {
            ProviderError::CipherError(format!("Invalid regex for method header {}: {}", meth_name, e))
        })?;

        let meth_match = meth_header_re.find(obj_body).ok_or_else(|| {
            ProviderError::CipherError(format!(
                "Method {}.{} not found in helper object body",
                helper_name, meth_name
            ))
        })?;

        let meth_body = extract_balanced_block(obj_body, meth_match.end() - 1).ok_or_else(|| {
            ProviderError::CipherError(format!(
                "Failed to extract balanced method body for {}.{}",
                helper_name, meth_name
            ))
        })?;

        if meth_body.contains("reverse") {
            method_ops.insert(meth_name.clone(), MethodType::Reverse);
        } else if meth_body.contains("splice") || meth_body.contains("slice") {
            method_ops.insert(meth_name.clone(), MethodType::Splice);
        } else if meth_body.contains("[0]") || meth_body.contains('%') || meth_body.contains("swap") {
            method_ops.insert(meth_name.clone(), MethodType::Swap);
        } else {
            return Err(ProviderError::CipherError(format!(
                "Unrecognized method body for {}.{}: {}",
                helper_name, meth_name, meth_body
            )));
        }
    }

    // 7. Assemble the final sequence of CipherOp
    let mut cipher_ops = Vec::with_capacity(raw_calls.len());
    for (_, meth_name, arg) in raw_calls {
        match method_ops.get(&meth_name) {
            Some(MethodType::Reverse) => cipher_ops.push(CipherOp::Reverse),
            Some(MethodType::Swap) => cipher_ops.push(CipherOp::Swap(arg)),
            Some(MethodType::Splice) => cipher_ops.push(CipherOp::Splice(arg)),
            None => {
                return Err(ProviderError::CipherError(format!(
                    "Unresolved method type for method {}",
                    meth_name
                )));
            }
        }
    }

    Ok(PlayerCipherState {
        signature_timestamp,
        cipher_ops,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MethodType {
    Reverse,
    Swap,
    Splice,
}

/// Descrambles a YouTube `signatureCipher` (or legacy `cipher`) string,
/// running the deciphering algorithm on parameter `s` and reassembling the direct stream URL.
pub fn descramble_signature(cipher_str: &str, ops: &[CipherOp]) -> Result<String, ProviderError> {
    let mut s: Option<String> = None;
    let mut sp: String = "sig".to_string();
    let mut url: Option<String> = None;

    // Split on '&' and parse key=value query pairs
    for pair in cipher_str.split('&') {
        let mut parts = pair.splitn(2, '=');
        let key = parts.next().unwrap_or("");
        let val = parts.next().unwrap_or("");

        let decoded_val = percent_decode_str(val)
            .decode_utf8()
            .map_err(|e| ProviderError::CipherError(format!("UTF-8 decode error in cipher query: {}", e)))?
            .to_string();

        match key {
            "s" => s = Some(decoded_val),
            "sp" => sp = decoded_val,
            "url" => url = Some(decoded_val),
            _ => {}
        }
    }

    let scrambled_s = s.ok_or_else(|| {
        ProviderError::CipherError("Missing 's' parameter in signatureCipher".into())
    })?;

    let base_url = url.ok_or_else(|| {
        ProviderError::CipherError("Missing 'url' parameter in signatureCipher".into())
    })?;

    // Execute the cipher operations
    let deciphered_sig = execute_cipher_ops(&scrambled_s, ops);

    // Reassemble the complete URL
    let separator = if base_url.contains('?') { '&' } else { '?' };
    let final_url = format!("{base_url}{separator}{sp}={deciphered_sig}");

    Ok(final_url)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_BASE_JS: &str = r#"
var _yt_player = {};
(function(g) {
    g.signatureTimestamp = 19842;
    var kx = function(a) {
        a = a.split("");
        AB.ro(a, 2);
        AB.xh(a, 5);
        AB.mn(a, 3);
        return a.join("");
    };
    var AB = {
        ro: function(a) { a.reverse() },
        xh: function(a, b) { var c = a[0]; a[0] = a[b % a.length]; a[b % a.length] = c; },
        mn: function(a, b) { a.splice(0, b); }
    };
    c && a.set("sig", encodeURIComponent(kx(c)));
})(this);
"#;

    #[test]
    fn test_parse_sample_base_js() {
        let state = parse_player_script(SAMPLE_BASE_JS).expect("Failed to parse sample base.js");
        assert_eq!(state.signature_timestamp, 19842);
        assert_eq!(
            state.cipher_ops,
            vec![CipherOp::Reverse, CipherOp::Swap(5), CipherOp::Splice(3)]
        );
    }

    #[test]
    fn test_descramble_signature_full() {
        let ops = [CipherOp::Reverse, CipherOp::Swap(5), CipherOp::Splice(3)];
        let cipher_str = "s=abcdef1234567890&sp=sig&url=https%3A%2F%2Frr1---sn-xxx.googlevideo.com%2Fvideoplayback%3Fitag%3D140";
        
        let resolved = descramble_signature(cipher_str, &ops).expect("Failed to descramble signature");
        assert!(resolved.starts_with("https://rr1---sn-xxx.googlevideo.com/videoplayback?itag=140&sig="));
    }
}
