//! Shared cryptographic utilities.

use sha2::{Digest, Sha256};

/// Compute a SHA-256 hex digest of `data`.
pub fn sha256_hex(data: &[u8]) -> String {
    use std::fmt::Write;
    let hash = Sha256::digest(data);
    let mut hex = String::with_capacity(hash.len() * 2);
    for b in &hash[..] {
        let _ = write!(hex, "{b:02x}");
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_hex_empty_string() {
        // Known SHA-256 of empty input
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn sha256_hex_hello() {
        assert_eq!(
            sha256_hex(b"hello"),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }
}
