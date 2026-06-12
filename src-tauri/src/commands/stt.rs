use tauri::AppHandle;

#[tauri::command]
#[allow(non_snake_case)]
pub async fn validate_stt_key(
    provider: String,
    api_key: Option<String>,
    apiKey: Option<String>,
) -> Result<(), String> {
    let key = apiKey.or(api_key).unwrap_or_default();
    let p = crate::cloud_stt::CloudProvider::from_id(&provider)
        .ok_or_else(|| format!("Unknown cloud STT provider: {}", provider))?;
    p.validate_key(&key).await
}

#[tauri::command]
pub async fn clear_stt_key_cache(_app: AppHandle, _provider: String) -> Result<(), String> {
    Ok(())
}
