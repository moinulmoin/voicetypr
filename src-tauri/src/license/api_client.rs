use super::types::*;
use reqwest;
use serde_json::json;
use std::time::Duration;
// use tokio::time::sleep; // TODO: Implement retry logic

const API_TIMEOUT: Duration = Duration::from_secs(30);

// TODO: Implement retry logic
#[allow(dead_code)]
const MAX_RETRIES: u32 = 3;
#[allow(dead_code)]
const INITIAL_RETRY_DELAY: Duration = Duration::from_millis(500);

fn get_api_base_url() -> String {
    #[cfg(debug_assertions)]
    {
        std::env::var("VOICETYPR_API_URL")
            .unwrap_or_else(|_| "http://localhost:3000/api/v1".to_string())
    }
    #[cfg(not(debug_assertions))]
    {
        "https://voicetypr.com/api/v1".to_string()
    }
}

pub struct LicenseApiClient {
    client: reqwest::Client,
}

impl LicenseApiClient {
    pub fn new() -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .timeout(API_TIMEOUT)
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

        Ok(Self { client })
    }

    /// Check trial status for a device
    pub async fn check_trial(&self, device_hash: &str) -> Result<TrialCheckResponse, String> {
        let url = format!("{}/trial/check", get_api_base_url());

        let response = self
            .client
            .post(&url)
            .json(&json!({
                "deviceHash": device_hash
            }))
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        if response.status().is_success() {
            response
                .json::<TrialCheckResponse>()
                .await
                .map_err(|e| format!("Failed to parse response: {}", e))
        } else {
            let error: ApiError = response.json().await.unwrap_or(ApiError {
                success: false,
                error: "unknown_error".to_string(),
                message: "Failed to check trial status".to_string(),
            });
            Err(error.message)
        }
    }

    /// Validate a license key
    pub async fn validate_license(
        &self,
        license_key: &str,
        device_hash: &str,
        app_version: Option<&str>,
    ) -> Result<LicenseValidateResponse, String> {
        let url = format!("{}/license/validate", get_api_base_url());

        let mut body = json!({
            "licenseKey": license_key,
            "deviceHash": device_hash
        });

        if let Some(version) = app_version {
            body["appVersion"] = json!(version);
        }

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        if response.status().is_success() {
            response
                .json::<LicenseValidateResponse>()
                .await
                .map_err(|e| format!("Failed to parse response: {}", e))
        } else {
            let error: ApiError = response.json().await.unwrap_or(ApiError {
                success: false,
                error: "unknown_error".to_string(),
                message: "Failed to validate license".to_string(),
            });
            Err(error.message)
        }
    }

    /// Activate a license key on a device
    pub async fn activate_license(
        &self,
        license_key: &str,
        device_hash: &str,
    ) -> Result<LicenseActivateResponse, String> {
        let url = format!("{}/license/activate", get_api_base_url());

        let response = self
            .client
            .post(&url)
            .json(&json!({
                "licenseKey": license_key,
                "deviceHash": device_hash
            }))
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        if response.status().is_success() {
            response
                .json::<LicenseActivateResponse>()
                .await
                .map_err(|e| format!("Failed to parse response: {}", e))
        } else if response.status() == 409 {
            // Conflict - license already activated on another device
            let error: ApiError = response.json().await.unwrap_or(ApiError {
                success: false,
                error: "license_already_activated".to_string(),
                message: "License is already activated on another device".to_string(),
            });
            Ok(LicenseActivateResponse {
                success: false,
                data: None,
                error: Some(error.error),
                message: Some(error.message),
            })
        } else {
            let error: ApiError = response.json().await.unwrap_or(ApiError {
                success: false,
                error: "unknown_error".to_string(),
                message: "Failed to activate license".to_string(),
            });
            Err(error.message)
        }
    }

    /// Deactivate a license from a device
    pub async fn deactivate_license(
        &self,
        license_key: &str,
        device_hash: &str,
    ) -> Result<LicenseDeactivateResponse, String> {
        let url = format!("{}/license/deactivate", get_api_base_url());

        let response = self
            .client
            .post(&url)
            .json(&json!({
                "licenseKey": license_key,
                "deviceHash": device_hash
            }))
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        if response.status().is_success() {
            response
                .json::<LicenseDeactivateResponse>()
                .await
                .map_err(|e| format!("Failed to parse response: {}", e))
        } else {
            let error: ApiError = response.json().await.unwrap_or(ApiError {
                success: false,
                error: "unknown_error".to_string(),
                message: "Failed to deactivate license".to_string(),
            });
            Err(error.message)
        }
    }
}

impl Default for LicenseApiClient {
    fn default() -> Self {
        Self::new().expect("Failed to create API client")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_client_creation() {
        let client = LicenseApiClient::new();
        assert!(client.is_ok());
    }
}
