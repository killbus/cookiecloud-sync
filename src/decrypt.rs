#[cfg(test)]
use aes::cipher::BlockEncryptMut;
use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;
type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;
#[cfg(test)]
type Aes128CbcEnc = cbc::Encryptor<aes::Aes128>;
#[cfg(test)]
type Aes256CbcEnc = cbc::Encryptor<aes::Aes256>;

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CookieData {
    pub domain: String,
    pub name: String,
    pub value: String,
    pub path: Option<String>,
    pub secure: Option<bool>,
    pub http_only: Option<bool>,
    pub same_site: Option<String>,
    pub expiration_date: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DecryptedData {
    pub cookie_data: std::collections::HashMap<String, Vec<CookieData>>,
    pub local_storage_data:
        Option<std::collections::HashMap<String, std::collections::HashMap<String, String>>>,
    pub update_time: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct EncryptedResponse {
    pub encrypted: String,
    #[serde(rename = "crypto_type")]
    pub crypto_type: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum DecryptError {
    #[error("invalid ciphertext: {0}")]
    InvalidCiphertext(String),
    #[error("unsupported crypto type: {0}")]
    UnsupportedCryptoType(String),
    #[error("decryption failed")]
    DecryptionFailed,
}

#[derive(Debug, thiserror::Error)]
#[cfg(test)]
pub enum EncryptError {
    #[error("encryption failed")]
    EncryptionFailed,
    #[error("RNG error: {0}")]
    RngError(String),
}

fn md5_hex_prefix(uuid: &str, password: &str) -> Zeroizing<Vec<u8>> {
    let mut input = format!("{uuid}-{password}");
    let hash = Md5::digest(input.as_bytes());
    let mut s = String::with_capacity(32);
    for b in &hash {
        use std::fmt::Write;
        write!(s, "{b:02x}").unwrap();
    }
    let key: Zeroizing<Vec<u8>> = Zeroizing::new(s.as_bytes()[..16].to_vec());
    input.zeroize();
    key
}

fn evp_bytes_to_key(password: &[u8], salt: &[u8]) -> (Zeroizing<Vec<u8>>, Zeroizing<Vec<u8>>) {
    let mut derived = Zeroizing::new(Vec::new());
    let mut prev: Vec<u8> = Vec::new();
    while derived.len() < 48 {
        let mut hasher = Md5::new();
        hasher.update(&prev);
        hasher.update(password);
        hasher.update(salt);
        prev = hasher.finalize().to_vec();
        derived.extend_from_slice(&prev);
    }
    prev.zeroize();
    let key = Zeroizing::new(derived[..32].to_vec());
    let iv = Zeroizing::new(derived[32..48].to_vec());
    (key, iv)
}

fn decrypt_fixed_iv(encrypted_data: &[u8], key: &[u8]) -> Result<Vec<u8>, DecryptError> {
    let iv = [0u8; 16];
    let mut buf = encrypted_data.to_vec();
    let pt = Aes128CbcDec::new(key.into(), &iv.into())
        .decrypt_padded_mut::<Pkcs7>(&mut buf)
        .map_err(|_| DecryptError::DecryptionFailed)?;
    let result = pt.to_vec();
    buf.zeroize();
    Ok(result)
}

fn decrypt_legacy(encrypted_data: &[u8], password: &[u8]) -> Result<Vec<u8>, DecryptError> {
    if encrypted_data.len() < 16 {
        return Err(DecryptError::InvalidCiphertext("data too short".into()));
    }
    if &encrypted_data[..8] != b"Salted__" {
        return Err(DecryptError::InvalidCiphertext(
            "missing Salted__ prefix".into(),
        ));
    }
    let salt = &encrypted_data[8..16];
    let ct = &encrypted_data[16..];
    let (key, iv) = evp_bytes_to_key(password, salt);
    let mut buf = ct.to_vec();
    let pt = Aes256CbcDec::new(key.as_slice().into(), iv.as_slice().into())
        .decrypt_padded_mut::<Pkcs7>(&mut buf)
        .map_err(|_| DecryptError::DecryptionFailed)?;
    let result = pt.to_vec();
    buf.zeroize();
    Ok(result)
}

#[cfg(test)]
fn encrypt_fixed_iv(plaintext: &[u8], key: &[u8]) -> Result<Vec<u8>, EncryptError> {
    let iv = [0u8; 16];
    let bs = 16usize;
    let mut buf = vec![0u8; plaintext.len() + bs];
    buf[..plaintext.len()].copy_from_slice(plaintext);
    let ct = Aes128CbcEnc::new(key.into(), &iv.into())
        .encrypt_padded_mut::<Pkcs7>(&mut buf, plaintext.len())
        .map_err(|_| EncryptError::EncryptionFailed)?;
    Ok(ct.to_vec())
}

#[cfg(test)]
fn encrypt_legacy(plaintext: &[u8], password: &[u8]) -> Result<Vec<u8>, EncryptError> {
    let salt = generate_random_salt()?;
    let bs = 16usize;
    let (key, iv) = evp_bytes_to_key(password, &salt);
    let mut buf = vec![0u8; plaintext.len() + bs];
    buf[..plaintext.len()].copy_from_slice(plaintext);
    let ct = Aes256CbcEnc::new(key.as_slice().into(), iv.as_slice().into())
        .encrypt_padded_mut::<Pkcs7>(&mut buf, plaintext.len())
        .map_err(|_| EncryptError::EncryptionFailed)?;
    let mut result = Vec::with_capacity(16 + ct.len());
    result.extend_from_slice(b"Salted__");
    result.extend_from_slice(&salt);
    result.extend_from_slice(ct);
    Ok(result)
}

#[cfg(test)]
fn generate_random_salt() -> Result<[u8; 8], EncryptError> {
    let mut salt = [0u8; 8];
    getrandom::getrandom(&mut salt).map_err(|e| EncryptError::RngError(e.to_string()))?;
    Ok(salt)
}

#[cfg(test)]
pub fn encrypt(
    uuid: &str,
    plaintext: &str,
    password: &str,
    crypto_type: Option<&str>,
) -> Result<String, EncryptError> {
    let pt_bytes = plaintext.as_bytes();
    let ct = crypto_type.unwrap_or("legacy");
    let encrypted_bytes = match ct.to_ascii_lowercase().as_str() {
        "aes-128-cbc-fixed" => {
            let key = md5_hex_prefix(uuid, password);
            encrypt_fixed_iv(pt_bytes, &key)?
        }
        "legacy" => {
            let passphrase = md5_hex_prefix(uuid, password);
            encrypt_legacy(pt_bytes, &passphrase)?
        }
        _ => return Err(EncryptError::EncryptionFailed),
    };
    Ok(base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        &encrypted_bytes,
    ))
}

pub fn decrypt(
    uuid: &str,
    encrypted: &str,
    password: &str,
    crypto_type: Option<&str>,
) -> Result<DecryptedData, DecryptError> {
    let encrypted_bytes =
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encrypted)
            .map_err(|e| DecryptError::InvalidCiphertext(e.to_string()))?;

    let ct = crypto_type.unwrap_or("legacy");
    let plaintext = match ct.to_ascii_lowercase().as_str() {
        "aes-128-cbc-fixed" => {
            let key = md5_hex_prefix(uuid, password);
            decrypt_fixed_iv(&encrypted_bytes, &key)?
        }
        "legacy" => {
            let passphrase = md5_hex_prefix(uuid, password);
            decrypt_legacy(&encrypted_bytes, &passphrase)?
        }
        _ => return Err(DecryptError::UnsupportedCryptoType(ct.to_string())),
    };

    serde_json::from_slice(&plaintext).map_err(|_| DecryptError::DecryptionFailed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_key_length() {
        let key = md5_hex_prefix("test-uuid", "test-password");
        assert_eq!(key.len(), 16);
    }

    #[test]
    fn derive_key_deterministic() {
        let key1 = md5_hex_prefix("test-uuid", "test-password");
        let key2 = md5_hex_prefix("test-uuid", "test-password");
        assert_eq!(*key1, *key2);
    }

    #[test]
    fn derive_key_differs_by_input() {
        let key1 = md5_hex_prefix("uuid1", "pass1");
        let key2 = md5_hex_prefix("uuid2", "pass2");
        assert_ne!(*key1, *key2);
    }

    #[test]
    fn evp_bytes_to_key_lengths() {
        let (key, iv) = evp_bytes_to_key(b"password", b"12345678");
        assert_eq!(key.len(), 32);
        assert_eq!(iv.len(), 16);
    }

    #[test]
    fn evp_bytes_to_key_deterministic() {
        let (k1, i1) = evp_bytes_to_key(b"password", b"12345678");
        let (k2, i2) = evp_bytes_to_key(b"password", b"12345678");
        assert_eq!(*k1, *k2);
        assert_eq!(*i1, *i2);
    }

    #[test]
    fn evp_bytes_to_key_differs_by_salt() {
        let (k1, _) = evp_bytes_to_key(b"password", b"aaaaaaaa");
        let (k2, _) = evp_bytes_to_key(b"password", b"bbbbbbbb");
        assert_ne!(*k1, *k2);
    }

    #[test]
    fn decrypt_fixed_iv_invalid_ciphertext() {
        let result = decrypt_fixed_iv(b"too short", &[0u8; 16]);
        assert!(result.is_err());
    }

    #[test]
    fn decrypt_fixed_iv_invalid_key() {
        let ct = vec![0u8; 32];
        let result = decrypt_fixed_iv(&ct, &[0u8; 16]);
        assert!(result.is_err());
    }

    #[test]
    fn decrypt_legacy_short_data() {
        let passphrase = md5_hex_prefix("uuid", "pwd");
        let result = decrypt_legacy(b"too short", &passphrase);
        assert!(result.is_err());
        match result {
            Err(DecryptError::InvalidCiphertext(msg)) => assert!(msg.contains("short")),
            _ => panic!("expected InvalidCiphertext error"),
        }
    }

    #[test]
    fn decrypt_unknown_crypto_type() {
        let result = decrypt("uuid", "dGVzdA==", "password", Some("foobar"));
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(DecryptError::UnsupportedCryptoType(_))
        ));
    }

    #[test]
    fn decrypt_defaults_to_fixed_iv() {
        let result = decrypt("uuid", "", "password", None);
        assert!(result.is_err());
        assert!(!matches!(
            result,
            Err(DecryptError::UnsupportedCryptoType(_))
        ));
    }

    #[test]
    fn roundtrip_fixed_iv() {
        let data = r#"{"cookie_data":{"example.com":[{"domain":".example.com","name":"session","value":"abc123","path":"/","secure":true,"httpOnly":true,"sameSite":"Lax","expiration_date":1777777777.0}]}}"#;
        let ct = encrypt("my-uuid", data, "my-password", Some("aes-128-cbc-fixed")).unwrap();
        let decrypted = decrypt("my-uuid", &ct, "my-password", Some("aes-128-cbc-fixed")).unwrap();
        let expected: DecryptedData = serde_json::from_str(data).unwrap();
        assert_eq!(decrypted.cookie_data, expected.cookie_data);
    }

    #[test]
    fn roundtrip_legacy() {
        let data = r#"{"cookie_data":{"example.com":[{"domain":".example.com","name":"session","value":"abc123","path":"/","secure":true,"httpOnly":true,"sameSite":"Lax","expiration_date":1777777777.0}]}}"#;
        let ct = encrypt("my-uuid", data, "my-password", Some("legacy")).unwrap();
        let decrypted = decrypt("my-uuid", &ct, "my-password", Some("legacy")).unwrap();
        let expected: DecryptedData = serde_json::from_str(data).unwrap();
        assert_eq!(decrypted.cookie_data, expected.cookie_data);
    }

    #[test]
    fn roundtrip_fixed_iv_default() {
        let data =
            r#"{"cookie_data":{"test.dev":[{"domain":".test.dev","name":"token","value":"xyz"}]}}"#;
        let ct = encrypt("uuid-1", data, "pass-1", None).unwrap();
        let decrypted = decrypt("uuid-1", &ct, "pass-1", None).unwrap();
        let expected: DecryptedData = serde_json::from_str(data).unwrap();
        assert_eq!(decrypted.cookie_data, expected.cookie_data);
    }

    #[test]
    fn known_answer_fixed_iv() {
        let uuid = "test-uuid-kat";
        let password = "test-password-kat";
        let expected_plaintext =
            r#"{"cookie_data":{"kat.dev":[{"domain":".kat.dev","name":"test","value":"hello"}]}}"#;
        let ct = "eSRyCMQwPxZqEfBnFmjhWNzXcYitTVhlv0rgNRE+WdjsHSD+IYUjA2fBgj0+4rIBaludMdjiTI/sGpZzUItIl+4E7S+sXWK0reQoMihlcsFQ8O+dOLN1NOA4RkF4iQg4";
        let decrypted = decrypt(uuid, ct, password, Some("aes-128-cbc-fixed")).unwrap();
        let expected: DecryptedData = serde_json::from_str(expected_plaintext).unwrap();
        assert_eq!(decrypted.cookie_data, expected.cookie_data);
    }

    #[test]
    fn md5_hex_prefix_matches_js() {
        let key = md5_hex_prefix("test-uuid", "test-password");
        let hex_str = std::str::from_utf8(&key).unwrap();
        assert_eq!(hex_str.len(), 16);
        for c in hex_str.chars() {
            assert!(c.is_ascii_hexdigit());
        }
    }

    #[test]
    fn encrypt_decrypt_roundtrip_multi_domain() {
        let data = r#"{"cookie_data":{"a.com":[{"domain":"a.com","name":"x","value":"1"}],"b.com":[{"domain":"b.com","name":"y","value":"2"}]}}"#;
        for ct in ["aes-128-cbc-fixed", "legacy"] {
            let ciphertext = encrypt("uuid", data, "pwd", Some(ct)).unwrap();
            let decrypted = decrypt("uuid", &ciphertext, "pwd", Some(ct)).unwrap();
            assert_eq!(decrypted.cookie_data.len(), 2);
            assert_eq!(decrypted.cookie_data["a.com"][0].value, "1");
            assert_eq!(decrypted.cookie_data["b.com"][0].value, "2");
        }
    }
}
