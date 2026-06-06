use rozsa_model::oauth::pkce;

#[test]
fn verifier_is_43_chars() {
    let v = pkce::generate_verifier();
    assert_eq!(v.len(), 43);
}

#[test]
fn challenge_is_43_chars() {
    let v = pkce::generate_verifier();
    let c = pkce::generate_challenge(&v);
    assert_eq!(c.len(), 43);
}

#[test]
fn challenge_is_deterministic() {
    let v = "test-verifier-that-is-long-enough-for-pkce";
    let c1 = pkce::generate_challenge(v);
    let c2 = pkce::generate_challenge(v);
    assert_eq!(c1, c2);
}

#[test]
fn different_verifiers_produce_different_challenges() {
    let v1 = pkce::generate_verifier();
    let v2 = pkce::generate_verifier();
    let c1 = pkce::generate_challenge(&v1);
    let c2 = pkce::generate_challenge(&v2);
    assert_ne!(c1, c2);
}
