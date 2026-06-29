use std::path::Path;

use argon2::password_hash::SaltString;
use argon2::{Algorithm, Argon2, Params, Version};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

use crate::crypto::{open, random_key, seal, SealedBytes};

#[derive(Debug, Serialize, Deserialize)]
pub struct VaultRecord {
    pub kdf: String,
    pub salt_b64: String,
    pub wrapped_master_key: SealedBytes,
}

pub fn create_vault_record(
    data_dir: &Path,
    password: &str,
) -> Result<Zeroizing<Vec<u8>>, std::io::Error> {
    let vault_dir = data_dir.join("vault");
    std::fs::create_dir_all(&vault_dir)?;
    let vault_path = vault_dir.join("vault.json");
    if vault_path.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "ERR_VAULT_ALREADY_EXISTS",
        ));
    }
    let salt = SaltString::generate(&mut OsRng);
    let mut derived_key = derive_key(password, salt.as_str().as_bytes())
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "kdf"))?;
    let master_key = random_key();
    let wrapped_master_key = seal(&derived_key, &master_key)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "seal"))?;
    derived_key.zeroize();
    let record = VaultRecord {
        kdf: "argon2id".to_string(),
        salt_b64: STANDARD.encode(salt.as_str().as_bytes()),
        wrapped_master_key,
    };
    let bytes = serde_json::to_vec_pretty(&record)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "json"))?;
    let temporary = vault_dir.join("vault.json.tmp");
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(temporary, vault_path)?;
    Ok(master_key)
}

pub fn load_vault_record(data_dir: &Path) -> Result<VaultRecord, std::io::Error> {
    let bytes = std::fs::read(data_dir.join("vault").join("vault.json"))?;
    let mut value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "vault"))?;
    if let Some(wrapped) = value.get_mut("wrapped_master_key") {
        let normalized = crate::crypto::from_slice(
            &serde_json::to_vec(wrapped).map_err(|_| std::io::ErrorKind::InvalidData)?,
        )
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "vault"))?;
        *wrapped = serde_json::to_value(normalized).map_err(|_| std::io::ErrorKind::InvalidData)?;
    }
    serde_json::from_value(value)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "vault"))
}

pub fn unlock_master_key(record: &VaultRecord, password: &str) -> Result<Zeroizing<Vec<u8>>, ()> {
    let salt = STANDARD.decode(&record.salt_b64).map_err(|_| ())?;
    let mut derived_key = derive_key(password, &salt).map_err(|_| ())?;
    let master = open(&derived_key, &record.wrapped_master_key)
        .map(Zeroizing::new)
        .map_err(|_| ());
    derived_key.zeroize();
    master
}

fn derive_key(password: &str, salt: &[u8]) -> Result<Vec<u8>, ()> {
    let params = Params::new(19_456, 2, 1, Some(32)).map_err(|_| ())?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut out = vec![0_u8; 32];
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut out)
        .map_err(|_| ())?;
    Ok(out)
}
