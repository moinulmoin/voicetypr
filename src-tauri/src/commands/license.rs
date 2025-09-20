use crate::license::{api_client::LicenseApiClient, device, keychain, LicenseState, LicenseStatus};
use crate::AppState;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tauri_plugin_cache::{CacheExt, SetItemOptions};

// Wrapper for cached license status with metadata
#[derive(Serialize, Deserialize, Debug)]
struct CachedLicenseStatus {
    status: LicenseStatus,
    cached_at: DateTime<Utc>,
}

// Constants for cache and grace periods
const OFFLINE_GRACE_PERIOD_DAYS: i64 = 90; // 90 days offline grace for licensed users - generous for legitimate use
const TRIAL_OFFLINE_GRACE_PERIOD_DAYS: i64 = 1; // 1 day offline grace for trial users - prevents abuse while allowing temporary outages
const CACHE_TTL_HOURS: u64 = 8; // 8-hour cache TTL for both licensed and trial users
const LICENSE_CACHE_KEY: &str = "license_status";
const LAST_VALIDATION_KEY: &str = "last_license_validation";
const LAST_TRIAL_VALIDATION_KEY: &str = "last_trial_validation"; // Tracks when trial was last validated online
const TRIAL_EXPIRES_KEY: &str = "trial_expires_at"; // Cache key for trial expiry date


// Error message constants for consistency
const ERR_INVALID_LICENSE: &str = "Invalid license key format";

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

// Helper function to convert hours to days with ceiling
fn hours_to_days(hours: i64) -> i32 {
    (hours as f64 / 24.0).ceil() as i32
}

// Check if we're within the grace period for offline access
fn is_within_grace_period(app: &AppHandle) -> Option<i64> {
    let cache = app.cache();

    if let Ok(Some(timestamp_json)) = cache.get(LAST_VALIDATION_KEY) {
        if let Ok(last_validation) = serde_json::from_value::<DateTime<Utc>>(timestamp_json) {
            let elapsed = Utc::now().signed_duration_since(last_validation);
            let days_elapsed = elapsed.num_days();

            if days_elapsed < OFFLINE_GRACE_PERIOD_DAYS {
                return Some(OFFLINE_GRACE_PERIOD_DAYS - days_elapsed);
            }
        }
    }

    None
}

// Check if grace period timestamp exists (regardless of whether it's valid)
fn has_grace_period_timestamp(app: &AppHandle) -> bool {
    let cache = app.cache();
    cache.get(LAST_VALIDATION_KEY)
        .ok()
        .flatten()
        .is_some()
}

// Check if we're within the trial grace period
fn is_within_trial_grace_period(app: &AppHandle) -> Option<i64> {
    let cache = app.cache();

    if let Ok(Some(timestamp_json)) = cache.get(LAST_TRIAL_VALIDATION_KEY) {
        if let Ok(last_validation) = serde_json::from_value::<DateTime<Utc>>(timestamp_json) {
            let elapsed = Utc::now().signed_duration_since(last_validation);
            let days_elapsed = elapsed.num_days();

            if days_elapsed < TRIAL_OFFLINE_GRACE_PERIOD_DAYS {
                return Some(TRIAL_OFFLINE_GRACE_PERIOD_DAYS - days_elapsed);
            }
        }
    }

    None
}

// Conservative license deletion check - only delete when absolutely certain
fn should_delete_invalid_license(error_msg: &str) -> bool {
    let msg_lower = error_msg.to_lowercase();

    // ONLY delete if the error explicitly says the license is invalid
    // Be extremely conservative - when in doubt, keep the license
    msg_lower.contains("invalid license") ||
    msg_lower.contains("license not found") ||
    msg_lower.contains("license expired") ||
    msg_lower.contains("license revoked")
}

/// Check the current license status
/// This checks license first (if stored), then falls back to trial
/// Forces fresh check on app start, then uses cache during session
#[tauri::command]
pub async fn check_license_status(app: AppHandle) -> Result<LicenseStatus, String> {
    log::info!("Checking license status");

    // Directly call the implementation - Tauri handles concurrent command execution
    check_license_status_impl(app).await
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
                            // Use ceiling for consistent day calculation (matches offline validation)
                            let hours_elapsed = elapsed.num_hours();
                            let elapsed_days = hours_to_days(hours_elapsed);
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
                        ttl: Some(CACHE_TTL_HOURS * 60 * 60), // 8 hours in seconds
                        compress: None,
                        compression_method: None,
                    });
                    match cache.set(
                        LICENSE_CACHE_KEY.to_string(),
                        serde_json::to_value(&wrapped_status).unwrap_or_default(),
                        cache_options,
                    ) {
                        Ok(_) => log::info!("Cached licensed status for {} hours with metadata", CACHE_TTL_HOURS),
                        Err(e) => log::error!("Failed to cache licensed status: {}", e),
                    }

                    return Ok(status);
                } else {
                    log::warn!("Stored license is invalid: {:?}", response.message);
                    // Only delete if we're certain the license is invalid
                    if let Some(ref msg) = response.message {
                        if should_delete_invalid_license(msg) {
                            log::info!("Removing invalid license from keychain");
                            let _ = keychain::delete_license(&app);
                        } else {
                            log::warn!("License validation failed but keeping stored license (error might be temporary)");
                        }
                    }
                }
            }
            Err(e) => {
                log::error!("Failed to validate license: {}", e);

                // Check if we're within the offline grace period
                if let Some(days_remaining) = is_within_grace_period(&app) {
                    log::info!("API unavailable but within {}-day grace period. {} days remaining",
                             OFFLINE_GRACE_PERIOD_DAYS, days_remaining);

                    // Warn if approaching limit
                    if days_remaining <= 7 {
                        log::warn!("⚠️ Only {} days of offline access remaining. Please connect to internet soon.",
                                 days_remaining);
                    }

                    let status = LicenseStatus {
                        status: LicenseState::Licensed,
                        trial_days_left: None,
                        license_type: Some("pro".to_string()),
                        license_key: Some(license_key),
                        expires_at: Some(format!("{} days offline remaining", days_remaining)),
                    };

                    // Cache with 8-hour TTL during grace period
                    let wrapped_status = CachedLicenseStatus {
                        status: status.clone(),
                        cached_at: Utc::now(),
                    };

                    let cache_options = Some(SetItemOptions {
                        ttl: Some(CACHE_TTL_HOURS * 60 * 60), // 8 hours
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
                    // Check if we have a timestamp at all before assuming grace period expired
                    if has_grace_period_timestamp(&app) {
                        // Timestamp exists and grace period expired
                        log::error!("Grace period of {} days has expired. License requires online validation.",
                                  OFFLINE_GRACE_PERIOD_DAYS);
                        // DO NOT DELETE THE LICENSE! User paid for it and might just be offline temporarily
                        // Clear the timestamp so next successful validation starts fresh grace period
                        let _ = cache.remove(LAST_VALIDATION_KEY);
                        return Err("Grace period expired. Please connect to internet to revalidate your license.".to_string());
                    } else {
                        // No timestamp exists - this is the first offline attempt
                        log::warn!("No previous online validation found. Initial online validation required.");
                        // DON'T delete the license - it may still be valid
                        return Err("Initial online validation required. Please connect to internet.".to_string());
                    }
                }
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

                // Set last successful trial validation timestamp for grace period tracking
                let validation_time = Utc::now();
                if let Err(e) = cache.set(
                    LAST_TRIAL_VALIDATION_KEY.to_string(),
                    serde_json::to_value(&validation_time).unwrap_or_default(),
                    None, // No TTL for validation timestamp
                ) {
                    log::warn!("Failed to set trial validation timestamp: {}", e);
                } else {
                    log::info!("Set trial validation timestamp for grace period tracking");
                }

                // Cache the trial expiry date from server for offline validation
                if let Some(expires_at) = &response.data.expires_at {
                    log::info!("Caching trial expiry date from server: {}", expires_at);
                    let cache_options = Some(SetItemOptions {
                        ttl: Some(CACHE_TTL_HOURS * 60 * 60), // 8 hours TTL for trial cache
                        compress: None,
                        compression_method: None,
                    });
                    match cache.set(
                        TRIAL_EXPIRES_KEY.to_string(),
                        serde_json::to_value(expires_at).unwrap_or_default(),
                        cache_options,
                    ) {
                        Ok(_) => log::info!("Cached trial expiry date for offline validation"),
                        Err(e) => log::error!("Failed to cache trial expiry date: {}", e),
                    }
                } else {
                    // No expires_at means something is wrong with device/trial creation
                    // Don't provide offline support - force online validation
                    log::error!("API returned trial without expires_at - this indicates a server issue. No offline support.");
                }

                // Only cache trial status if more than 1 day remaining
                // This prevents caching a trial that's about to expire
                if trial_days_left > 1 {
                    let wrapped_status = CachedLicenseStatus {
                        status: status.clone(),
                        cached_at: Utc::now(),
                    };

                    let cache_options = Some(SetItemOptions {
                        ttl: Some(CACHE_TTL_HOURS * 60 * 60), // 8 hours in seconds
                        compress: None,
                        compression_method: None,
                    });
                    match cache.set(
                        LICENSE_CACHE_KEY.to_string(),
                        serde_json::to_value(&wrapped_status).unwrap_or_default(),
                        cache_options,
                    ) {
                        Ok(_) => log::info!(
                            "Cached trial status for {} hours - {} days remaining",
                            CACHE_TTL_HOURS, trial_days_left
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

            // FIRST: Check if we have a cached trial expiry date
            if let Ok(Some(expires_json)) = cache.get(TRIAL_EXPIRES_KEY) {
                if let Ok(expires_at_str) = serde_json::from_value::<String>(expires_json) {
                    if let Ok(expires_at) = DateTime::parse_from_rfc3339(&expires_at_str) {
                        let expires_utc = expires_at.with_timezone(&Utc);
                        let now = Utc::now();

                        // Check if trial has already expired
                        if now >= expires_utc {
                            log::info!("Trial has expired (was valid until {}). No need to check grace period.", expires_at_str);
                            // Clear the expired trial cache
                            let _ = cache.remove(TRIAL_EXPIRES_KEY);
                            let _ = cache.remove(LAST_TRIAL_VALIDATION_KEY);

                            return Ok(LicenseStatus {
                                status: LicenseState::Expired,
                                trial_days_left: Some(0),
                                license_type: None,
                                license_key: None,
                                expires_at: None,
                            });
                        }

                        // Trial is still valid, now check grace period
                        if let Some(grace_days_remaining) = is_within_trial_grace_period(&app) {
                            log::info!("Trial still valid AND within {}-day grace period. {} days grace remaining",
                                     TRIAL_OFFLINE_GRACE_PERIOD_DAYS, grace_days_remaining);

                            // Calculate actual trial days left
                            let hours_left = (expires_utc - now).num_hours();
                            let days_left = hours_to_days(hours_left).max(0);

                            log::info!("Offline trial access: {} trial days left (expires: {})",
                                     days_left, expires_at_str);

                            let status = LicenseStatus {
                                status: if days_left > 0 { LicenseState::Trial } else { LicenseState::Expired },
                                trial_days_left: Some(days_left.max(0)),
                                license_type: None,
                                license_key: None,
                                expires_at: Some(format!("Offline grace: {} day remaining", grace_days_remaining)),
                            };

                            return Ok(status);
                        } else {
                            log::error!("Trial is valid but grace period expired. Requires online validation.");
                            // Don't delete anything - user just needs to reconnect
                            return Err("Trial grace period expired. Please connect to internet to continue trial.".to_string());
                        }
                    } else {
                        log::warn!("Failed to parse cached trial expiry date: {}", expires_at_str);
                    }
                }
            } else {
                log::warn!("No cached trial expiry date found");
            }

            // No cached expiry or couldn't parse it - check grace period anyway (backward compatibility)
            if let Some(_grace_days_remaining) = is_within_trial_grace_period(&app) {
                log::warn!("No cached expiry but within grace period. This shouldn't happen normally.");
                return Err("Trial validation failed but grace period active. Please reconnect to internet.".to_string());
            }

            // Check if we have a cached license status to fall back on
            if let Ok(Some(cached_json)) = cache.get(LICENSE_CACHE_KEY) {
                if let Ok(cached) = serde_json::from_value::<CachedLicenseStatus>(cached_json) {
                    let age = Utc::now().signed_duration_since(cached.cached_at);

                    // Use cached status if it's less than cache TTL
                    if age < Duration::hours(CACHE_TTL_HOURS as i64) {
                        log::info!("API unavailable, using cached license status from {} ago",
                                 format_duration(&age));

                        // For trial users, adjust days remaining based on cache age
                        let mut status = cached.status.clone();
                        if status.status == LicenseState::Trial {
                            if let Some(days) = status.trial_days_left {
                                // Use ceiling for consistent day calculation
                                let hours_elapsed = age.num_hours();
                                let days_elapsed = hours_to_days(hours_elapsed);
                                status.trial_days_left = Some((days - days_elapsed).max(0));
                            }
                        }

                        return Ok(status);
                    }
                }
            }

            // No cached trial data - return no license status
            log::info!("No cached trial data - returning no license status");
            let status = LicenseStatus {
                status: LicenseState::None,
                trial_days_left: None,
                license_type: None,
                license_key: None,
                expires_at: None,
            };

            // Don't cache None status - always check
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

                // Set last validation timestamp for grace period tracking
                let validation_time = Utc::now();
                let cache = app.cache();
                if let Err(e) = cache.set(
                    LAST_VALIDATION_KEY.to_string(),
                    serde_json::to_value(&validation_time).unwrap_or_default(),
                    None, // No TTL for validation timestamp
                ) {
                    log::warn!("Failed to set last validation timestamp during restore: {}", e);
                }

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
        return Err(ERR_INVALID_LICENSE.to_string());
    }

    // Check for VoiceTypr license format: must start with VT and contain hyphens
    if !trimmed_key.starts_with("VT") || !trimmed_key.contains('-') {
        return Err(ERR_INVALID_LICENSE.to_string());
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
    let app_version = app.package_info().version.to_string();

    match api_client
        .activate_license(&license_key, &device_hash, Some(&app_version))
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

                // Set last validation timestamp for grace period tracking
                let validation_time = Utc::now();
                let cache = app.cache();
                if let Err(e) = cache.set(
                    LAST_VALIDATION_KEY.to_string(),
                    serde_json::to_value(&validation_time).unwrap_or_default(),
                    None, // No TTL for validation timestamp
                ) {
                    log::warn!("Failed to set last validation timestamp: {}", e);
                }

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
                // Clear validation timestamp when deactivating - this is intentional removal
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
            // NEVER delete license on deactivation failure!
            // If deactivation fails, the server still thinks the license is active
            // User should be able to retry deactivation when they have proper connectivity
            log::warn!("Deactivation failed. License remains in keychain. Please retry when connected.");

            // Clear cache even on failure
            let cache = app.cache();
            match cache.remove(LICENSE_CACHE_KEY) {
                Ok(_) => log::info!("Cleared license cache after deactivation failure"),
                Err(e) => log::warn!("Failed to clear cache after deactivation failure: {}", e),
            }
            // Keep the last validation timestamp even on failure
            // The deactivation failed, so everything should remain as-is

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
    // Keep the validation timestamp - we're only clearing the cached status,
    // not the validation history. The timestamp is important for grace period.
    // Clear trial cache as well when license state changes
    let _ = cache.remove(TRIAL_EXPIRES_KEY);
}

pub async fn check_license_status_internal(app: &AppHandle) -> Result<LicenseStatus, String> {
    check_license_status(app.clone()).await
}
