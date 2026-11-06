use serde::{Deserialize, Serialize};

/// Atomic cipher operations used by YouTube player scripts to descramble signatures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CipherOp {
    /// Reverses the order of characters in the signature string in-place
    Reverse,
    /// Swaps the character at index 0 with the character at `pos % len`
    Swap(usize),
    /// Removes (splices) the first `count` characters from the beginning of the signature
    Splice(usize),
}

/// Executes a sequence of cipher operations on a mutable character vector in-place.
/// Completely panic-free with bounds checking and modulo arithmetic.
pub fn execute_cipher_ops_in_place(chars: &mut Vec<char>, ops: &[CipherOp]) {
    let len = chars.len();
    if len == 0 {
        return;
    }

    for op in ops {
        match *op {
            CipherOp::Reverse => {
                chars.reverse();
            }
            CipherOp::Swap(pos) => {
                if !chars.is_empty() {
                    let target = pos % chars.len();
                    chars.swap(0, target);
                }
            }
            CipherOp::Splice(count) => {
                let to_drain = count.min(chars.len());
                chars.drain(..to_drain);
            }
        }
    }
}

/// Executes a sequence of cipher operations on a scrambled signature string and returns the deciphered string.
pub fn execute_cipher_ops(scrambled: &str, ops: &[CipherOp]) -> String {
    let mut chars: Vec<char> = scrambled.chars().collect();
    execute_cipher_ops_in_place(&mut chars, ops);
    chars.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reverse() {
        assert_eq!(execute_cipher_ops("hello", &[CipherOp::Reverse]), "olleh");
        assert_eq!(execute_cipher_ops("", &[CipherOp::Reverse]), "");
    }

    #[test]
    fn test_swap() {
        assert_eq!(execute_cipher_ops("abcde", &[CipherOp::Swap(2)]), "cbade");
        assert_eq!(execute_cipher_ops("abcde", &[CipherOp::Swap(7)]), "cbade"); // 7 % 5 = 2
        assert_eq!(execute_cipher_ops("", &[CipherOp::Swap(2)]), "");
    }

    #[test]
    fn test_splice() {
        assert_eq!(execute_cipher_ops("abcde", &[CipherOp::Splice(2)]), "cde");
        assert_eq!(execute_cipher_ops("abcde", &[CipherOp::Splice(10)]), "");
        assert_eq!(execute_cipher_ops("", &[CipherOp::Splice(2)]), "");
    }

    #[test]
    fn test_vector_s1() {
        // Vector S1: Synthetic Mock Pipeline
        // "abcdefghij" -> Swap(2) -> Reverse -> Splice(3) -> "gfedabc"
        let ops = [CipherOp::Swap(2), CipherOp::Reverse, CipherOp::Splice(3)];
        assert_eq!(execute_cipher_ops("abcdefghij", &ops), "gfedabc");
    }
}
