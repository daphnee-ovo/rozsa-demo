use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::Rng;
use sha2::{Digest, Sha256};

/// Generate a PKCE code verifier (43 characters from 32 random bytes).
pub fn generate_verifier() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Generate a PKCE S256 code challenge from a verifier.
pub fn generate_challenge(verifier: &str) -> String {
    let hash = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifier_is_43_chars() {
        let v = generate_verifier();
        assert_eq!(v.len(), 43);
    }

    #[test]
    fn challenge_is_43_chars() {
        let v = generate_verifier();
        let c = generate_challenge(&v);
        assert_eq!(c.len(), 43);
    }

    #[test]
    fn challenge_is_deterministic() {
        let v = "test-verifier-that-is-long-enough-for-pkce";
        let c1 = generate_challenge(v);
        let c2 = generate_challenge(v);
        assert_eq!(c1, c2);
    }

    #[test]
    fn different_verifiers_produce_different_challenges() {
        let v1 = generate_verifier();
        let v2 = generate_verifier();
        let c1 = generate_challenge(&v1);
        let c2 = generate_challenge(&v2);
        assert_ne!(c1, c2);
    }
}
