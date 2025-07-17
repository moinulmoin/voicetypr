use crate::license::{api_client::LicenseApiClient, device, keychain, LicenseState, LicenseStatus};
// Chrono imports removed - not used in current implementation
use tauri::{AppHandle, Manager};
use tauri_plugin_cache::{CacheExt, SetItemOptions};

/// Check the current license status
/// This checks license first (if stored), then falls back to trial
/// Uses cache with 6-hour interval for trial users, 24-hour for licensed users
#[tauri::command]
pub async fn check_license_status(app: AppHandle) -> Result<LicenseStatus, String> {
    log::info!("Checking license status");
    
    // Try to get cached status first
    let cache = app.cache();
    match cache.get("license_status") {
        Ok(Some(cached_json)) => {
            log::info!("Cache hit: Found cached license status");
            match serde_json::from_value::<LicenseStatus>(cached_json) {
                Ok(cached_status) => {
                    log::info!("Cache hit: Successfully deserialized cached status - Type: {:?}, Days left: {:?}", 
                        cached_status.status, cached_status.trial_days_left);
                    
                    // Double-check: If cached trial has 0 days left, treat as expired
                    if matches!(cached_status.status, LicenseState::Trial) {
                        if let Some(days) = cached_status.trial_days_left {
                            if days <= 0 {
                                log::warn!("Cache hit but trial has expired (0 days left) - ignoring cache");
                                // Fall through to fresh check
                            } else {
                                return Ok(cached_status);
                            }
                        } else {
                            return Ok(cached_status);
                        }
                    } else {
                        // Not a trial, return cached status
                        return Ok(cached_status);
                    }
                },
                Err(e) => {
                    log::warn!("Cache hit but failed to deserialize: {}", e);
                }
            }
        },
        Ok(None) => {
            log::info!("Cache miss: No cached license status found");
        },
        Err(e) => {
            log::warn!("Cache error: Failed to check cache: {}", e);
        }
    }

    // Get device hash
    let device_hash = device::get_device_hash()?;

    // First, check if we have a stored license
    if let Some(license_key) = keychain::get_license()? {
        log::info!("Found stored license, validating...");

        // Try to validate the stored license
        let api_client = LicenseApiClient::new()?;
        let app_version = app.package_info().version.to_string();

        match api_client
            .validate_license(&license_key, &device_hash, Some(&app_version))
            .await
        {
            Ok(response) => {
                if response.data.valid {
                    log::info!("License is valid");
                    let status = LicenseStatus {
                        status: LicenseState::Licensed,
                        trial_days_left: None,
                        license_type: Some("pro".to_string()), // You might want to get this from the API
                        license_key: Some(license_key),
                        expires_at: None,
                    };
                    
                    // Cache for 24 hours for licensed users
                    let cache_options = Some(SetItemOptions {
                        ttl: Some(24 * 60 * 60), // 24 hours in seconds
                        compress: None,
                        compression_method: None,
                    });
                    match cache.set("license_status".to_string(), serde_json::to_value(&status).unwrap_or_default(), cache_options) {
                        Ok(_) => log::info!("Cached licensed status for 24 hours"),
                        Err(e) => log::error!("Failed to cache licensed status: {}", e),
                    }
                    
                    return Ok(status);
                } else {
                    log::warn!("Stored license is invalid: {:?}", response.message);
                    // Remove invalid license from keychain
                    let _ = keychain::delete_license();
                }
            }
            Err(e) => {
                log::error!("Failed to validate license: {}", e);
                // TODO: Implement offline grace period for licensed users
                // Backend has 7-day offline grace configured (CONFIG.offlineGracePeriodDays)
                // Client should store last successful validation timestamp
                // and allow offline usage if within grace period
                // For now, we'll fall through to trial check
            }
        }
    }

    // No valid license found, check trial status
    log::info!("Checking trial status");
    let api_client = LicenseApiClient::new()?;

    match api_client.check_trial(&device_hash).await {
        Ok(response) => {
            if response.data.is_expired {
                log::info!("Trial has expired");
                let status = LicenseStatus {
                    status: LicenseState::Expired,
                    trial_days_left: Some(0),
                    license_type: None,
                    license_key: None,
                    expires_at: None,
                };
                
                // Don't cache expired status - always check
                log::info!("Not caching expired status - will check on every call");
                Ok(status)
            } else {
                // Backend now returns daysLeft!
                let trial_days_left = response.data.days_left.unwrap_or(0).max(0);

                log::info!("Trial is active with {} days left", trial_days_left);
                let status = LicenseStatus {
                    status: LicenseState::Trial,
                    trial_days_left: Some(trial_days_left),
                    license_type: None,
                    license_key: None,
                    expires_at: None,
                };
                
                // Only cache trial status if more than 1 day remaining
                // This prevents caching a trial that's about to expire
                if trial_days_left > 1 {
                    let cache_options = Some(SetItemOptions {
                        ttl: Some(6 * 60 * 60), // 6 hours in seconds
                        compress: None,
                        compression_method: None,
                    });
                    match cache.set("license_status".to_string(), serde_json::to_value(&status).unwrap_or_default(), cache_options) {
                        Ok(_) => log::info!("Cached trial status for 6 hours - {} days remaining", trial_days_left),
                        Err(e) => log::error!("Failed to cache trial status: {}", e),
                    }
                } else {
                    log::info!("Not caching trial status - only {} days remaining (expires soon)", trial_days_left);
                }
                
                Ok(status)
            }
        }
        Err(e) => {
            log::error!("Failed to check trial status: {}", e);
            // Assume no license/trial if we can't check
            let status = LicenseStatus {
                status: LicenseState::None,
                trial_days_left: None,
                license_type: None,
                license_key: None,
                expires_at: None,
            };
            
            // Don't cache None status - always check
            log::info!("Not caching None status - will check on every call");
            Ok(status)
        }
    }
}

/// Restore a license from keychain and validate it
#[tauri::command]
pub async fn restore_license(app: AppHandle) -> Result<LicenseStatus, String> {
    log::info!("Attempting to restore license");

    // Check if we have a stored license
    let license_key =
        keychain::get_license()?.ok_or_else(|| "No license found in keychain".to_string())?;

    let device_hash = device::get_device_hash()?;
    let api_client = LicenseApiClient::new()?;
    let app_version = app.package_info().version.to_string();

    // Try to validate the license
    match api_client
        .validate_license(&license_key, &device_hash, Some(&app_version))
        .await
    {
        Ok(response) => {
            if response.data.valid {
                log::info!("License restored successfully");
                
                // Reset recording state when license is restored
                let app_state = app.state::<crate::AppState>();
                if let Err(e) = app_state.recording_state.reset() {
                    log::warn!("Failed to reset recording state during license restore: {}", e);
                } else {
                    log::info!("Reset recording state to Idle during license restore");
                }
                
                Ok(LicenseStatus {
                    status: LicenseState::Licensed,
                    trial_days_left: None,
                    license_type: Some("pro".to_string()),
                    license_key: Some(license_key),
                    expires_at: None,
                })
            } else {
                // License is not valid for this device, try to activate it
                log::info!("License not valid for this device, attempting activation");
                activate_license_internal(license_key, app).await
            }
        }
        Err(e) => {
            log::error!("Failed to validate license: {}", e);
            Err(format!("Failed to restore license: {}", e))
        }
    }
}

/// Activate a new license key
#[tauri::command]
pub async fn activate_license(
    license_key: String,
    app: AppHandle,
) -> Result<LicenseStatus, String> {
    log::info!("Activating license");
    
    // Validate license key format (basic validation)
    let trimmed_key = license_key.trim();
    if trimmed_key.is_empty() {
        return Err("License key cannot be empty".to_string());
    }
    
    // Basic format validation: alphanumeric with hyphens, reasonable length
    if trimmed_key.len() < 10 || trimmed_key.len() > 100 {
        return Err("Invalid license key format".to_string());
    }
    
    // Check for valid characters (alphanumeric, hyphens, underscores)
    if !trimmed_key.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return Err("License key contains invalid characters".to_string());
    }
    
    // Reset recording state to Idle when activating license
    // This ensures we're not stuck in Error state from previous license issues
    let app_state = app.state::<crate::AppState>();
    if let Err(e) = app_state.recording_state.reset() {
        log::warn!("Failed to reset recording state during license activation: {}", e);
    } else {
        log::info!("Reset recording state to Idle during license activation");
    }
    
    activate_license_internal(trimmed_key.to_string(), app).await
}

/// Internal function to handle license activation
async fn activate_license_internal(
    license_key: String,
    app: AppHandle,
) -> Result<LicenseStatus, String> {
    let device_hash = device::get_device_hash()?;
    let api_client = LicenseApiClient::new()?;

    match api_client
        .activate_license(&license_key, &device_hash)
        .await
    {
        Ok(response) => {
            if response.success {
                // Save the license to keychain
                keychain::save_license(&license_key)?;

                log::info!("License activated successfully");
                
                // Clear cache when license is activated
                let cache = app.cache();
                match cache.remove("license_status") {
                    Ok(_) => log::info!("Cleared license cache after activation"),
                    Err(e) => log::warn!("Failed to clear cache after activation: {}", e),
                }
                
                // Reset recording state when license is successfully activated
                let app_state = app.state::<crate::AppState>();
                if let Err(e) = app_state.recording_state.reset() {
                    log::warn!("Failed to reset recording state after successful activation: {}", e);
                } else {
                    log::info!("Reset recording state to Idle after successful activation");
                }
                
                Ok(LicenseStatus {
                    status: LicenseState::Licensed,
                    trial_days_left: None,
                    license_type: Some("pro".to_string()),
                    license_key: Some(license_key),
                    expires_at: None,
                })
            } else {
                let error_msg = response
                    .message
                    .unwrap_or_else(|| "Failed to activate license".to_string());

                if response.error == Some("license_already_activated".to_string()) {
                    Err("This license is already activated on another device".to_string())
                } else {
                    Err(error_msg)
                }
            }
        }
        Err(e) => {
            log::error!("Failed to activate license: {}", e);
            Err(format!("Failed to activate license: {}", e))
        }
    }
}

/// Deactivate the current license
#[tauri::command]
pub async fn deactivate_license(app: AppHandle) -> Result<(), String> {
    log::info!("Deactivating license");

    // Get the stored license
    let license_key =
        keychain::get_license()?.ok_or_else(|| "No license found to deactivate".to_string())?;

    let device_hash = device::get_device_hash()?;
    let api_client = LicenseApiClient::new()?;

    match api_client
        .deactivate_license(&license_key, &device_hash)
        .await
    {
        Ok(response) => {
            if response.success {
                // Remove from keychain
                keychain::delete_license()?;
                
                // Clear cache when license is deactivated
                let cache = app.cache();
                match cache.remove("license_status") {
                    Ok(_) => log::info!("Cleared license cache after deactivation"),
                    Err(e) => log::warn!("Failed to clear cache after deactivation: {}", e),
                }
                
                log::info!("License deactivated successfully");
                
                // Reset recording state when license is deactivated
                let app_state = app.state::<crate::AppState>();
                if let Err(e) = app_state.recording_state.reset() {
                    log::warn!("Failed to reset recording state after deactivation: {}", e);
                } else {
                    log::info!("Reset recording state to Idle after deactivation");
                }
                
                Ok(())
            } else {
                let error_msg = response
                    .message
                    .unwrap_or_else(|| "Failed to deactivate license".to_string());
                Err(error_msg)
            }
        }
        Err(e) => {
            log::error!("Failed to deactivate license: {}", e);
            // Even if API fails, remove from keychain
            keychain::delete_license()?;
            
            // Clear cache even on failure
            let cache = app.cache();
            match cache.remove("license_status") {
                Ok(_) => log::info!("Cleared license cache after deactivation failure"),
                Err(e) => log::warn!("Failed to clear cache after deactivation failure: {}", e),
            }
            
            // Reset recording state even on API failure (since we removed from keychain)
            let app_state = app.state::<crate::AppState>();
            if let Err(e) = app_state.recording_state.reset() {
                log::warn!("Failed to reset recording state after deactivation failure: {}", e);
            } else {
                log::info!("Reset recording state to Idle after deactivation failure");
            }
            
            Err(format!("Failed to deactivate license: {}", e))
        }
    }
}

/// Open the purchase page in the default browser
#[tauri::command]
pub async fn open_purchase_page() -> Result<(), String> {
    log::info!("Opening purchase page");

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("https://voicetypr.com/pricing")
            .spawn()
            .map_err(|e| format!("Failed to open browser: {}", e))?;
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(&["/C", "start", "https://voicetypr.com/pricing"])
            .spawn()
            .map_err(|e| format!("Failed to open browser: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg("https://voicetypr.com/pricing")
            .spawn()
            .map_err(|e| format!("Failed to open browser: {}", e))?;
    }

    Ok(())
}

/// Internal function to check license status (for use by other commands)
pub async fn check_license_status_internal(app: &AppHandle) -> Result<LicenseStatus, String> {
    check_license_status(app.clone()).await
}
