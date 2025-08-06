use crate::license::{api_client::LicenseApiClient, device, keychain, LicenseState, LicenseStatus};
use crate::AppState;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, Manager};
use tauri_plugin_cache::{CacheExt, SetItemOptions};
use tokio::sync::broadcast;
use tokio::sync::{OnceCell, RwLock};

// Wrapper for cached license status with metadata
#[derive(Serialize, Deserialize, Debug)]
struct CachedLicenseStatus {
    status: LicenseStatus,
    cached_at: DateTime<Utc>,
}

// Constants for cache and grace periods
const OFFLINE_GRACE_PERIOD_DAYS: i64 = 7; // 7 days offline grace for licensed users
const LICENSE_CACHE_KEY: &str = "license_status";
const LAST_VALIDATION_KEY: &str = "last_license_validation";

// Global deduplication map for license status checks
static LICENSE_CHECK_DEDUP: OnceCell<
    Arc<RwLock<HashMap<String, Arc<broadcast::Sender<Result<LicenseStatus, String>>>>>>,
> = OnceCell::const_new();

async fn get_dedup_map(
) -> &'static Arc<RwLock<HashMap<String, Arc<broadcast::Sender<Result<LicenseStatus, String>>>>>> {
    LICENSE_CHECK_DEDUP
        .get_or_init(|| async { Arc::new(RwLock::new(HashMap::new())) })
        .await
}

// Helper function to format duration for logging
fn format_duration(duration: &Duration) -> String {
    let days = duration.num_days();
    let hours = duration.num_hours() % 24;
    let minutes = duration.num_minutes() % 60;

    if days > 0 {
        format!("{} days, {} hours", days, hours)
    } else if hours > 0 {
        format!("{} hours, {} minutes", hours, minutes)
    } else {
        format!("{} minutes", minutes)
    }
}

/// Check the current license status
/// This checks license first (if stored), then falls back to trial
/// Forces fresh check on app start, then uses cache during session
#[tauri::command]
pub async fn check_license_status(app: AppHandle) -> Result<LicenseStatus, String> {
    log::info!("Checking license status");

    // Since we clear cache on app start, we can use simpler logic

    // Deduplication key - use a constant since we're checking the same thing
    const DEDUP_KEY: &str = "license_status_check";

    // Check if there's already an in-flight request
    {
        let dedup_map = get_dedup_map().await.read().await;
        if let Some(sender) = dedup_map.get(DEDUP_KEY) {
            log::info!("License check already in progress, waiting for result...");
            let mut receiver = sender.subscribe();
            drop(dedup_map); // Release the read lock

            // Wait for the in-flight request to complete
            match receiver.recv().await {
                Ok(result) => {
                    log::info!("Received result from in-flight license check");
                    return result;
                }
                Err(_) => {
                    log::warn!("In-flight license check sender dropped, proceeding with new check");
                    // Fall through to perform our own check
                }
            }
        }
    }

    // No in-flight request, we'll be the primary requester
    let (tx, _rx) = broadcast::channel(1);
    let tx = Arc::new(tx);

    // Register our request
    {
        let mut dedup_map = get_dedup_map().await.write().await;
        dedup_map.insert(DEDUP_KEY.to_string(), tx.clone());
    }

    // Perform the actual license check
    let result = check_license_status_impl(app).await;

    // Send result to any waiting threads
    let _ = tx.send(result.clone());

    // Remove from dedup map
    {
        let mut dedup_map = get_dedup_map().await.write().await;
        dedup_map.remove(DEDUP_KEY);
    }

    result
}

/// Internal implementation of license status check
async fn check_license_status_impl(app: AppHandle) -> Result<LicenseStatus, String> {
    let cache = app.cache();

    // Try to get cached status (cache is cleared on app start)
    match cache.get(LICENSE_CACHE_KEY) {
        Ok(Some(cached_json)) => {
            log::info!("📦 Cache hit: Found cached license status");
            log::debug!("Raw cached data: {:?}", cached_json);

            // Try to deserialize as new format first (with metadata)
            match serde_json::from_value::<CachedLicenseStatus>(cached_json.clone()) {
                Ok(cached_with_metadata) => {
                    let mut status = cached_with_metadata.status;
                    let cached_at = cached_with_metadata.cached_at;
                    let elapsed = Utc::now().signed_duration_since(cached_at);

                    log::info!(
                        "Cache hit: New format with metadata - cached {} ago",
                        format_duration(&elapsed)
                    );

                    // For trials, adjust days left based on elapsed time
                    if matches!(status.status, LicenseState::Trial) {
                        if let Some(original_days) = status.trial_days_left {
                            let elapsed_days = elapsed.num_days() as i32;
                            let current_days = (original_days - elapsed_days).max(0);

                            log::info!("Trial days adjustment: {} days cached - {} days elapsed = {} days left",
                                original_days, elapsed_days, current_days);

                            if current_days <= 0 {
                                log::warn!("Cached trial has expired - performing fresh check");
                                // Fall through to fresh check
                            } else {
                                status.trial_days_left = Some(current_days);
                                return Ok(status);
                            }
                        }
                    } else {
                        // Not a trial, return cached status
                        return Ok(status);
                    }
                }
                Err(_) => {
                    // Try old format (backward compatibility)
                    match serde_json::from_value::<LicenseStatus>(cached_json) {
                        Ok(cached_status) => {
                            log::info!(
                                "Cache hit: Old format (no metadata) - Type: {:?}, Days left: {:?}",
                                cached_status.status,
                                cached_status.trial_days_left
                            );

                            // For old format, we can't adjust trial days, so be conservative
                            if matches!(cached_status.status, LicenseState::Trial) {
                                if let Some(days) = cached_status.trial_days_left {
                                    if days <= 1 {
                                        log::warn!("Old format cache with low trial days - performing fresh check");
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
                        }
                        Err(e) => {
                            log::warn!("Cache hit but failed to deserialize: {}", e);
                        }
                    }
                }
            }
        }
        Ok(None) => {
            log::info!("Cache miss: No cached license status found (fresh check after app start)");
        }
        Err(e) => {
            log::warn!("Cache error: Failed to check cache: {}", e);
        }
    }

    // Get device hash
    let device_hash = device::get_device_hash()?;

    // First, check if we have a stored license
    if let Some(license_key) = keychain::get_license(&app)? {
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

                    // Store last successful validation timestamp
                    let validation_time = Utc::now();
                    let _ = cache.set(
                        LAST_VALIDATION_KEY.to_string(),
                        serde_json::to_value(&validation_time).unwrap_or_default(),
                        None,
                    );

                    // Cache for 24 hours for licensed users
                    let wrapped_status = CachedLicenseStatus {
                        status: status.clone(),
                        cached_at: validation_time,
                    };

                    let cache_options = Some(SetItemOptions {
                        ttl: Some(24 * 60 * 60), // 24 hours in seconds
                        compress: None,
                        compression_method: None,
                    });
                    match cache.set(
                        LICENSE_CACHE_KEY.to_string(),
                        serde_json::to_value(&wrapped_status).unwrap_or_default(),
                        cache_options,
                    ) {
                        Ok(_) => log::info!("Cached licensed status for 24 hours with metadata"),
                        Err(e) => log::error!("Failed to cache licensed status: {}", e),
                    }

                    return Ok(status);
                } else {
                    log::warn!("Stored license is invalid: {:?}", response.message);
                    // Remove invalid license from keychain
                    let _ = keychain::delete_license(&app);
                }
            }
            Err(e) => {
                log::error!("Failed to validate license: {}", e);

                // Check for offline grace period
                if let Ok(Some(last_validation_json)) = cache.get(LAST_VALIDATION_KEY) {
                    if let Ok(last_validation) =
                        serde_json::from_value::<DateTime<Utc>>(last_validation_json)
                    {
                        let elapsed = Utc::now().signed_duration_since(last_validation);
                        let grace_period = Duration::days(OFFLINE_GRACE_PERIOD_DAYS);

                        if elapsed < grace_period {
                            log::info!("License validation failed but within {} day grace period (last validated {} ago)",
                                OFFLINE_GRACE_PERIOD_DAYS, format_duration(&elapsed));

                            let status = LicenseStatus {
                                status: LicenseState::Licensed,
                                trial_days_left: None,
                                license_type: Some("pro".to_string()),
                                license_key: Some(license_key),
                                expires_at: None,
                            };

                            // Cache with shorter TTL during offline grace
                            let wrapped_status = CachedLicenseStatus {
                                status: status.clone(),
                                cached_at: Utc::now(),
                            };

                            let cache_options = Some(SetItemOptions {
                                ttl: Some(4 * 60 * 60), // 4 hours during grace period
                                compress: None,
                                compression_method: None,
                            });
                            let _ = cache.set(
                                LICENSE_CACHE_KEY.to_string(),
                                serde_json::to_value(&wrapped_status).unwrap_or_default(),
                                cache_options,
                            );

                            return Ok(status);
                        } else {
                            log::warn!(
                                "License validation failed and grace period expired ({} > {} days)",
                                elapsed.num_days(),
                                OFFLINE_GRACE_PERIOD_DAYS
                            );
                        }
                    }
                }

                // Grace period expired or no last validation found
                // Fall through to trial check
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
                    let wrapped_status = CachedLicenseStatus {
                        status: status.clone(),
                        cached_at: Utc::now(),
                    };

                    let cache_options = Some(SetItemOptions {
                        ttl: Some(6 * 60 * 60), // 6 hours in seconds
                        compress: None,
                        compression_method: None,
                    });
                    match cache.set(
                        LICENSE_CACHE_KEY.to_string(),
                        serde_json::to_value(&wrapped_status).unwrap_or_default(),
                        cache_options,
                    ) {
                        Ok(_) => log::info!(
                            "Cached trial status for 6 hours with metadata - {} days remaining",
                            trial_days_left
                        ),
                        Err(e) => log::error!("Failed to cache trial status: {}", e),
                    }
                } else {
                    log::info!(
                        "Not caching trial status - only {} days remaining (expires soon)",
                        trial_days_left
                    );
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
        keychain::get_license(&app)?.ok_or_else(|| "No license found in keychain".to_string())?;

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

                // Clear cache when license is restored
                invalidate_license_cache(&app).await;

                // Reset recording state when license is restored
                let app_state = app.state::<AppState>();
                if let Err(e) = app_state.recording_state.reset() {
                    log::warn!(
                        "Failed to reset recording state during license restore: {}",
                        e
                    );
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
    if !trimmed_key
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err("License key contains invalid characters".to_string());
    }

    // Reset recording state to Idle when activating license
    // This ensures we're not stuck in Error state from previous license issues
    let app_state = app.state::<AppState>();
    if let Err(e) = app_state.recording_state.reset() {
        log::warn!(
            "Failed to reset recording state during license activation: {}",
            e
        );
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
                keychain::save_license(&app, &license_key)?;

                // Immediately read it back to trigger macOS keychain permission prompt
                // This ensures the user grants permission during activation, not during first recording
                match keychain::get_license(&app)? {
                    Some(_) => log::info!("License saved and verified in keychain"),
                    None => {
                        log::error!("License was saved but could not be read back");
                        return Err("Failed to verify license storage".to_string());
                    }
                }

                log::info!("License activated successfully");

                // Clear cache when license is activated
                invalidate_license_cache(&app).await;

                // Reset recording state when license is successfully activated
                let app_state = app.state::<AppState>();
                if let Err(e) = app_state.recording_state.reset() {
                    log::warn!(
                        "Failed to reset recording state after successful activation: {}",
                        e
                    );
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
                // Return the actual error message from the API
                let error_msg = response
                    .message
                    .unwrap_or_else(|| "Failed to activate license".to_string());
                Err(error_msg)
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
        keychain::get_license(&app)?.ok_or_else(|| "No license found to deactivate".to_string())?;

    let device_hash = device::get_device_hash()?;
    let api_client = LicenseApiClient::new()?;

    match api_client
        .deactivate_license(&license_key, &device_hash)
        .await
    {
        Ok(response) => {
            if response.success {
                // Remove from keychain
                keychain::delete_license(&app)?;

                // Clear cache when license is deactivated
                let cache = app.cache();
                match cache.remove(LICENSE_CACHE_KEY) {
                    Ok(_) => log::info!("Cleared license cache after deactivation"),
                    Err(e) => log::warn!("Failed to clear cache after deactivation: {}", e),
                }
                // Also clear last validation timestamp
                let _ = cache.remove(LAST_VALIDATION_KEY);

                log::info!("License deactivated successfully");

                // Reset recording state when license is deactivated
                let app_state = app.state::<AppState>();
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
            keychain::delete_license(&app)?;

            // Clear cache even on failure
            let cache = app.cache();
            match cache.remove(LICENSE_CACHE_KEY) {
                Ok(_) => log::info!("Cleared license cache after deactivation failure"),
                Err(e) => log::warn!("Failed to clear cache after deactivation failure: {}", e),
            }
            // Also clear last validation timestamp
            let _ = cache.remove(LAST_VALIDATION_KEY);

            // Reset recording state even on API failure (since we removed from keychain)
            let app_state = app.state::<AppState>();
            if let Err(e) = app_state.recording_state.reset() {
                log::warn!(
                    "Failed to reset recording state after deactivation failure: {}",
                    e
                );
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
            .arg("https://voicetypr.com/#pricing")
            .spawn()
            .map_err(|e| format!("Failed to open browser: {}", e))?;
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        
        std::process::Command::new("cmd")
            .args(&["/C", "start", "https://voicetypr.com/#pricing"])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("Failed to open browser: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg("https://voicetypr.com/#pricing")
            .spawn()
            .map_err(|e| format!("Failed to open browser: {}", e))?;
    }

    Ok(())
}

/// Internal function to check license status (for use by other commands)
/// Helper function to invalidate license cache
async fn invalidate_license_cache(app: &AppHandle) {
    let cache = app.cache();
    match cache.remove(LICENSE_CACHE_KEY) {
        Ok(_) => log::info!("License cache invalidated"),
        Err(e) => log::warn!("Failed to invalidate license cache: {}", e),
    }
    // Also remove last validation timestamp when invalidating
    let _ = cache.remove(LAST_VALIDATION_KEY);
}

pub async fn check_license_status_internal(app: &AppHandle) -> Result<LicenseStatus, String> {
    check_license_status(app.clone()).await
}
