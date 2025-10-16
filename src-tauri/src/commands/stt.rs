use reqwest::StatusCode;
use tauri::AppHandle;

#[tauri::command]
pub async fn validate_and_cache_soniox_key(api_key: Option<String>, apiKey: Option<String>) -> Result<(), String> {
    let api_key = apiKey.or(api_key).unwrap_or_default();
    if api_key.trim().is_empty() {
        return Err("API key cannot be empty".into());
    }

    // Best-effort validation against a public endpoint; if network fails, return error
    // We do not persist anything here; the frontend stores the key in secure store.
    let client = reqwest::Client::new();
    // Validate against an authenticated endpoint that exists across accounts
    // /v1/models lists available models; returns 200 when the key is valid.
    let url = "https://api.soniox.com/v1/models";

    let resp = client
        .get(url)
        .bearer_auth(api_key.trim())
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    match resp.status() {
        s if s.is_success() => Ok(()),
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            Err("Invalid Soniox API key".to_string())
        }
        other => {
            let body = resp.text().await.unwrap_or_default();
            let snippet: String = body.chars().take(500).collect();
            Err(format!("HTTP {}: {}", other, snippet))
        }
    }
}

/// Optional: clear any future cache (currently no-op)
#[tauri::command]
pub async fn clear_soniox_key_cache(_app: AppHandle) -> Result<(), String> {
    Ok(())
}
