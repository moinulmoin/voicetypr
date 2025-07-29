use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose, Engine as _};
use rand::Rng;
use once_cell::sync::OnceCell;
use tauri::{AppHandle, Runtime};
use tauri_plugin_store::StoreExt;
use crate::license::device;

// Encryption key storage - OnceCell ensures thread-safe single initialization
static ENCRYPTION_KEY: OnceCell<[u8; 32]> = OnceCell::new();

/// Initialize the encryption key using the device hash
pub fn initialize_encryption_key() -> Result<(), String> {
    ENCRYPTION_KEY.get_or_try_init(|| {
        // Get the same device hash used for API authentication
        let device_hash = device::get_device_hash()?;
        
        // The device hash is already a SHA256 hash (64 hex chars = 32 bytes)
        // Convert it from hex string to bytes
        let mut key = [0u8; 32];
        hex::decode_to_slice(&device_hash, &mut key)
            .map_err(|_| "Failed to decode device hash")?;
        
        log::info!("Initialized encryption with device-specific key");
        Ok(key)
    }).map(|_| ())
}

/// Check if migration from keychain is needed (for future use)
pub fn check_migration_needed<R: Runtime>(app: &AppHandle<R>) -> bool {
    // Check if secure.dat exists
    let store_exists = app.store("secure.dat").is_ok();
    
    // For now, we don't migrate automatically
    // This is here for future use if needed
    if !store_exists {
        log::debug!("No secure store found, fresh installation");
    }
    
    false
}

/// Encrypt a string value
fn encrypt_value(value: &str) -> Result<String, String> {
    let key = ENCRYPTION_KEY.get()
        .ok_or("Encryption key not initialized")?;
    
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|_| "Failed to create cipher")?;
    
    // Generate random nonce
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    
    // Encrypt
    let ciphertext = cipher
        .encrypt(nonce, value.as_bytes())
        .map_err(|_| "Encryption failed")?;
    
    // Combine nonce and ciphertext
    let mut combined = nonce_bytes.to_vec();
    combined.extend_from_slice(&ciphertext);
    
    // Base64 encode
    Ok(general_purpose::STANDARD.encode(combined))
}

/// Decrypt a string value
fn decrypt_value(encrypted: &str) -> Result<String, String> {
    let key = ENCRYPTION_KEY.get()
        .ok_or("Encryption key not initialized")?;
    
    // Base64 decode
    let combined = general_purpose::STANDARD
        .decode(encrypted)
        .map_err(|_| "Failed to decode encrypted value")?;
    
    if combined.len() < 12 {
        return Err("Invalid encrypted value".to_string());
    }
    
    // Split nonce and ciphertext
    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);
    
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|_| "Failed to create cipher")?;
    
    // Decrypt
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| "Decryption failed")?;
    
    String::from_utf8(plaintext).map_err(|_| "Invalid UTF-8 in decrypted value".to_string())
}

/// Set an encrypted value in the store
pub fn secure_set<R: Runtime>(
    app: &AppHandle<R>,
    key: &str,
    value: &str,
) -> Result<(), String> {
    let encrypted = encrypt_value(value)?;
    
    let store = app.store("secure.dat")
        .map_err(|e| format!("Failed to access store: {}", e))?;
    
    store.set(key, encrypted);
    store.save().map_err(|e| format!("Failed to save store: {}", e))?;
    
    Ok(())
}

/// Get and decrypt a value from the store with corruption recovery
pub fn secure_get<R: Runtime>(
    app: &AppHandle<R>,
    key: &str,
) -> Result<Option<String>, String> {
    // Try to access the store with recovery on failure
    let store = match app.store("secure.dat") {
        Ok(store) => store,
        Err(e) => {
            log::warn!("Store access failed: {}. This is normal on first run.", e);
            // Store doesn't exist or is inaccessible - this is OK, return None
            return Ok(None);
        }
    };
    
    match store.get(key) {
        Some(value) => {
            if let Some(encrypted) = value.as_str() {
                // Try to decrypt, but handle corruption gracefully
                match decrypt_value(encrypted) {
                    Ok(decrypted) => Ok(Some(decrypted)),
                    Err(e) => {
                        log::error!("Decryption failed for key '{}': {}. Data may be corrupted.", key, e);
                        
                        // Delete just this corrupted entry, not the whole store
                        store.delete(key);
                        if let Err(save_err) = store.save() {
                            log::error!("Failed to save store after removing corrupted key: {}", save_err);
                        }
                        
                        // Return None - treat as missing data
                        Ok(None)
                    }
                }
            } else {
                log::error!("Invalid value type in store for key '{}' - expected string", key);
                // Remove the corrupted entry
                store.delete(key);
                let _ = store.save();
                Ok(None)
            }
        }
        None => Ok(None),
    }
}

/// Delete a value from the secure store
pub fn secure_delete<R: Runtime>(
    app: &AppHandle<R>,
    key: &str,
) -> Result<(), String> {
    let store = app.store("secure.dat")
        .map_err(|e| format!("Failed to access store: {}", e))?;
    
    store.delete(key);
    store.save().map_err(|e| format!("Failed to save store: {}", e))?;
    
    Ok(())
}

/// Check if a key exists in the secure store
pub fn secure_has<R: Runtime>(
    app: &AppHandle<R>,
    key: &str,
) -> Result<bool, String> {
    let store = match app.store("secure.dat") {
        Ok(store) => store,
        Err(_) => {
            // Store doesn't exist - key definitely doesn't exist
            return Ok(false);
        }
    };
    
    // Check if key exists AND is valid (can be decrypted)
    Ok(match store.get(key) {
        Some(value) => {
            if let Some(encrypted) = value.as_str() {
                // Only return true if we can successfully decrypt it
                decrypt_value(encrypted).is_ok()
            } else {
                false
            }
        }
        None => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption_decryption() {
        initialize_encryption_key().unwrap();
        
        let original = "my-secret-api-key";
        let encrypted = encrypt_value(original).unwrap();
        let decrypted = decrypt_value(&encrypted).unwrap();
        
        assert_eq!(original, decrypted);
        assert_ne!(original, encrypted);
    }
    
    #[test]
    fn test_different_encryptions() {
        initialize_encryption_key().unwrap();
        
        let original = "test-value";
        let encrypted1 = encrypt_value(original).unwrap();
        let encrypted2 = encrypt_value(original).unwrap();
        
        // Different nonces should produce different ciphertexts
        assert_ne!(encrypted1, encrypted2);
        
        // But both should decrypt to the same value
        assert_eq!(decrypt_value(&encrypted1).unwrap(), original);
        assert_eq!(decrypt_value(&encrypted2).unwrap(), original);
    }
}