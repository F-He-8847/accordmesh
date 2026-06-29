use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

const ENVELOPE_VERSION: u8 = 1;

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("ERR_CRYPTO")]
    Crypto,
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("ERR_ENCRYPTED_DATA_CORRUPT")]
    Corrupt,
    #[error("ERR_ENCRYPTED_DATA_VERSION")]
    UnsupportedVersion,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SealedBytes {
    #[serde(default = "legacy_version")]
    pub version: u8,
    #[serde(alias = "nonce")]
    pub nonce_b64: String,
    #[serde(alias = "ciphertext")]
    pub ciphertext_b64: String,
}

fn legacy_version() -> u8 {
    ENVELOPE_VERSION
}

pub fn random_key() -> Zeroizing<Vec<u8>> {
    let mut key = Zeroizing::new(vec![0_u8; 32]);
    OsRng.fill_bytes(&mut key);
    key
}

pub fn seal(key: &[u8], plaintext: &[u8]) -> Result<SealedBytes, CryptoError> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| CryptoError::Crypto)?;
    let mut nonce_bytes = [0_u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|_| CryptoError::Crypto)?;
    Ok(SealedBytes {
        version: ENVELOPE_VERSION,
        nonce_b64: STANDARD.encode(nonce_bytes),
        ciphertext_b64: STANDARD.encode(ciphertext),
    })
}

pub fn open(key: &[u8], sealed: &SealedBytes) -> Result<Vec<u8>, CryptoError> {
    if sealed.version != ENVELOPE_VERSION {
        return Err(CryptoError::UnsupportedVersion);
    }
    let nonce_bytes = STANDARD
        .decode(&sealed.nonce_b64)
        .map_err(|_| CryptoError::Corrupt)?;
    let ciphertext = STANDARD
        .decode(&sealed.ciphertext_b64)
        .map_err(|_| CryptoError::Corrupt)?;
    if nonce_bytes.len() != 12 || ciphertext.len() < 16 {
        return Err(CryptoError::Corrupt);
    }
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| CryptoError::Crypto)?;
    let nonce = Nonce::from_slice(&nonce_bytes);
    cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|_| CryptoError::Corrupt)
}

pub fn to_vec(sealed: &SealedBytes) -> Result<Vec<u8>, CryptoError> {
    Ok(serde_json::to_vec(sealed)?)
}

pub fn from_slice(bytes: &[u8]) -> Result<SealedBytes, CryptoError> {
    let value: serde_json::Value = serde_json::from_slice(bytes)?;
    if value.get("nonce").and_then(|v| v.as_array()).is_some() {
        let nonce = value["nonce"]
            .as_array()
            .ok_or(CryptoError::Corrupt)?
            .iter()
            .map(|v| {
                v.as_u64()
                    .and_then(|n| u8::try_from(n).ok())
                    .ok_or(CryptoError::Corrupt)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let ciphertext = value["ciphertext"]
            .as_array()
            .ok_or(CryptoError::Corrupt)?
            .iter()
            .map(|v| {
                v.as_u64()
                    .and_then(|n| u8::try_from(n).ok())
                    .ok_or(CryptoError::Corrupt)
            })
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(SealedBytes {
            version: ENVELOPE_VERSION,
            nonce_b64: STANDARD.encode(nonce),
            ciphertext_b64: STANDARD.encode(ciphertext),
        });
    }
    Ok(serde_json::from_value(value)?)
}
