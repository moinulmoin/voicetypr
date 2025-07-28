use keyring::Entry;
use std::collections::HashMap;
use std::sync::Mutex;
use once_cell::sync::Lazy;

// Cache to avoid repeated keyring access
static KEYRING_CACHE: Lazy<Mutex<HashMap<String, String>>> = Lazy::new(|| {
    Mutex::new(HashMap::new())
});

const SERVICE_NAME: &str = "VoiceTypr";

#[tauri::command]
pub async fn keyring_set(key: String, value: String) -> Result<(), String> {
    let entry = Entry::new(SERVICE_NAME, &key)
        .map_err(|e| format!("Failed to create keyring entry: {}", e))?;
    
    entry.set_password(&value)
        .map_err(|e| format!("Failed to set password: {}", e))?;
    
    // Update cache
    let mut cache = KEYRING_CACHE.lock().unwrap();
    cache.insert(key.clone(), value);
    
    log::info!("Saved to keyring: {}", key);
    Ok(())
}

#[tauri::command]
pub async fn keyring_get(key: String) -> Result<Option<String>, String> {
    // Check cache first
    {
        let cache = KEYRING_CACHE.lock().unwrap();
        if let Some(value) = cache.get(&key) {
            return Ok(Some(value.clone()));
        }
    }
    
    let entry = Entry::new(SERVICE_NAME, &key)
        .map_err(|e| format!("Failed to create keyring entry: {}", e))?;
    
    match entry.get_password() {
        Ok(password) => {
            // Update cache
            let mut cache = KEYRING_CACHE.lock().unwrap();
            cache.insert(key.clone(), password.clone());
            Ok(Some(password))
        },
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("Failed to get password: {}", e))
    }
}

#[tauri::command]
pub async fn keyring_delete(key: String) -> Result<(), String> {
    let entry = Entry::new(SERVICE_NAME, &key)
        .map_err(|e| format!("Failed to create keyring entry: {}", e))?;
    
    match entry.delete_password() {
        Ok(_) => {
            // Remove from cache
            let mut cache = KEYRING_CACHE.lock().unwrap();
            cache.remove(&key);
            log::info!("Deleted from keyring: {}", key);
            Ok(())
        },
        Err(keyring::Error::NoEntry) => Ok(()), // Already deleted
        Err(e) => Err(format!("Failed to delete password: {}", e))
    }
}

#[tauri::command]
pub async fn keyring_has(key: String) -> Result<bool, String> {
    // Check cache first
    {
        let cache = KEYRING_CACHE.lock().unwrap();
        if cache.contains_key(&key) {
            return Ok(true);
        }
    }
    
    let entry = Entry::new(SERVICE_NAME, &key)
        .map_err(|e| format!("Failed to create keyring entry: {}", e))?;
    
    match entry.get_password() {
        Ok(password) => {
            // Update cache
            let mut cache = KEYRING_CACHE.lock().unwrap();
            cache.insert(key, password);
            Ok(true)
        },
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(e) => Err(format!("Failed to check password: {}", e))
    }
}