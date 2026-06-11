use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};

type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;
type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;

#[derive(Debug, Deserialize, Serialize)]
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

#[derive(Debug, Deserialize, Serialize)]
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

#[derive(Debug)]
pub enum DecryptError {
    InvalidCiphertext(String),
    PaddingError,
    JsonError(serde_json::Error),
    UnsupportedCryptoType(String),
}

impl std::fmt::Display for DecryptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecryptError::InvalidCiphertext(msg) => write!(f, "invalid ciphertext: {msg}"),
            DecryptError::PaddingError => write!(f, "padding error"),
            DecryptError::JsonError(e) => write!(f, "JSON error: {e}"),
            DecryptError::UnsupportedCryptoType(t) => write!(f, "unsupported crypto type: {t}"),
        }
    }
}

impl std::error::Error for DecryptError {}

fn derive_key(uuid: &str, password: &str) -> Vec<u8> {
    let input = format!("{uuid}-{password}");
    let hash = Md5::digest(input.as_bytes());
    hex::encode(hash).as_bytes()[..16].to_vec()
}

fn evp_bytes_to_key(password: &[u8], salt: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let mut derived = Vec::new();
    let mut prev = Vec::new();
    while derived.len() < 48 {
        let mut hasher = Md5::new();
        hasher.update(&prev);
        hasher.update(password);
        hasher.update(salt);
        prev = hasher.finalize().to_vec();
        derived.extend_from_slice(&prev);
    }
    let key = derived[..32].to_vec();
    let iv = derived[32..48].to_vec();
    (key, iv)
}

fn decrypt_fixed_iv(encrypted_data: &[u8], key: &[u8]) -> Result<Vec<u8>, DecryptError> {
    let iv = [0u8; 16];
    let mut buf = encrypted_data.to_vec();
    let pt = Aes128CbcDec::new(key.into(), &iv.into())
        .decrypt_padded_mut::<Pkcs7>(&mut buf)
        .map_err(|_| DecryptError::PaddingError)?;
    Ok(pt.to_vec())
}

fn decrypt_legacy(encrypted_data: &[u8], password: &[u8]) -> Result<Vec<u8>, DecryptError> {
    if encrypted_data.len() < 16 {
        return Err(DecryptError::InvalidCiphertext("data too short".into()));
    }
    let salt = &encrypted_data[8..16];
    let ct = &encrypted_data[16..];
    let (key, iv) = evp_bytes_to_key(password, salt);
    let mut buf = ct.to_vec();
    let pt = Aes256CbcDec::new(key.as_slice().into(), iv.as_slice().into())
        .decrypt_padded_mut::<Pkcs7>(&mut buf)
        .map_err(|_| DecryptError::PaddingError)?;
    Ok(pt.to_vec())
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

    let plaintext = match crypto_type.unwrap_or("aes-128-cbc-fixed") {
        "aes-128-cbc-fixed" => {
            let key = derive_key(uuid, password);
            decrypt_fixed_iv(&encrypted_bytes, &key)?
        }
        "legacy" => decrypt_legacy(&encrypted_bytes, password.as_bytes())?,
        t => return Err(DecryptError::UnsupportedCryptoType(t.to_string())),
    };

    serde_json::from_slice(&plaintext).map_err(DecryptError::JsonError)
}
