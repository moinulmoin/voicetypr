use keyring::Entry;

const SERVICE_NAME: &str = "VoiceTypr";
const LICENSE_KEY_NAME: &str = "license";

/// Save a license key to the system keychain
pub fn save_license(key: &str) -> Result<(), String> {
    match Entry::new(SERVICE_NAME, LICENSE_KEY_NAME) {
        Ok(entry) => {
            match entry.set_password(key) {
                Ok(()) => {
                    log::info!("License saved to keychain successfully");
                    Ok(())
                }
                Err(e) => {
                    log::error!("Failed to save license to keychain: {}", e);
                    // Provide user-friendly error message
                    match e {
                        keyring::Error::NoStorageAccess(_) => {
                            Err("Access to secure storage denied. Please grant permission and try again.".to_string())
                        }
                        keyring::Error::PlatformFailure(_) => {
                            Err("Platform keychain error. Please check your system keychain settings.".to_string())
                        }
                        _ => Err(format!("Failed to save license securely: {}", e))
                    }
                }
            }
        }
        Err(e) => {
            log::error!("Failed to access keychain: {}", e);
            Err("Unable to access secure storage. Please check your system settings.".to_string())
        }
    }
}

/// Get the stored license key from the system keychain
pub fn get_license() -> Result<Option<String>, String> {
    let entry = Entry::new(SERVICE_NAME, LICENSE_KEY_NAME)
        .map_err(|e| format!("Failed to create keychain entry: {}", e))?;

    match entry.get_password() {
        Ok(password) => {
            log::info!("License retrieved from keychain");
            Ok(Some(password))
        }
        Err(keyring::Error::NoEntry) => {
            log::debug!("No license found in keychain");
            Ok(None)
        }
        Err(e) => {
            log::error!("Failed to get license from keychain: {}", e);
            Err(format!("Failed to retrieve license from keychain: {}", e))
        }
    }
}

/// Delete the stored license key from the system keychain
pub fn delete_license() -> Result<(), String> {
    let entry = Entry::new(SERVICE_NAME, LICENSE_KEY_NAME)
        .map_err(|e| format!("Failed to create keychain entry: {}", e))?;

    match entry.delete_password() {
        Ok(()) => {
            log::info!("License deleted from keychain");
            Ok(())
        }
        Err(keyring::Error::NoEntry) => {
            log::debug!("No license to delete from keychain");
            Ok(())
        }
        Err(e) => {
            log::error!("Failed to delete license from keychain: {}", e);
            Err(format!("Failed to delete license from keychain: {}", e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keychain_operations() {
        // Note: These tests might fail in CI environments without keychain access

        // Test saving
        let test_key = "test_license_key_12345";
        match save_license(test_key) {
            Ok(()) => {
                // Test retrieval
                match get_license() {
                    Ok(Some(key)) => assert_eq!(key, test_key),
                    Ok(None) => panic!("License should have been saved"),
                    Err(e) => eprintln!("Keychain test skipped: {}", e),
                }

                // Test deletion
                let _ = delete_license();
            }
            Err(e) => {
                eprintln!("Keychain test skipped: {}", e);
            }
        }
    }
}
