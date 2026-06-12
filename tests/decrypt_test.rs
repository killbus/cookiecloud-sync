use cookiecloud_sync::decrypt;

#[test]
fn decrypt_invalid_base64() {
    let result = decrypt::decrypt("uuid", "!!invalid-base64!!", "password", None);
    assert!(result.is_err());
}

#[test]
fn decrypt_legacy_too_short() {
    let result = decrypt::decrypt("uuid", "dG9vIHNob3J0", "password", Some("legacy"));
    assert!(result.is_err());
}

#[test]
fn decrypt_unsupported_crypto_type() {
    let result = decrypt::decrypt("uuid", "dGVzdA==", "password", Some("unknown-type"));
    assert!(result.is_err());
    let err = result.unwrap_err();
    match err {
        decrypt::DecryptError::UnsupportedCryptoType(_) => {}
        other => panic!("expected UnsupportedCryptoType, got {other:?}"),
    }
}

#[test]
fn decrypt_empty_encrypted() {
    let result = decrypt::decrypt("uuid", "", "password", None);
    assert!(result.is_err());
}
