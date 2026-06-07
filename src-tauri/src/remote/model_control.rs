//! Remote model control helpers for password-gated sharing servers.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::async_runtime::RwLock as AsyncRwLock;
use tauri::{AppHandle, Emitter, Manager};

use super::lifecycle::RemoteServerManager;
use super::server::{RemoteModelControlSnapshot, ShareableRemoteModelInfo};
use super::transcription::SharedServerState;
use crate::commands::remote::resolve_shareable_model_config;
use crate::parakeet::ParakeetManager;
use crate::whisper::manager::WhisperManager;
use tokio::sync::Mutex as AsyncMutex;

static MODEL_CONTROL_ENABLED: AtomicBool = AtomicBool::new(false);

pub fn set_model_control_enabled(enabled: bool) {
    MODEL_CONTROL_ENABLED.store(enabled, Ordering::SeqCst);
}

pub fn is_model_control_enabled() -> bool {
    MODEL_CONTROL_ENABLED.load(Ordering::SeqCst)
}

#[cfg(test)]
pub fn reset_model_control_enabled_for_tests() {
    MODEL_CONTROL_ENABLED.store(false, Ordering::SeqCst);
}

pub async fn list_shareable_remote_models(app: &AppHandle) -> Vec<ShareableRemoteModelInfo> {
    let mut models = Vec::new();

    let whisper_state = app.state::<AsyncRwLock<WhisperManager>>();
    {
        let mut guard = whisper_state.write().await;
        guard.refresh_downloaded_status();
        for (id, info) in guard.get_models_status() {
            if !info.downloaded {
                continue;
            }
            models.push(ShareableRemoteModelInfo {
                id,
                display_name: info.display_name,
                engine: "whisper".to_string(),
                recommended: Some(info.recommended),
                speed_score: Some(info.speed_score),
                accuracy_score: Some(info.accuracy_score),
            });
        }
    }

    if let Some(parakeet_manager) = app.try_state::<ParakeetManager>() {
        for status in parakeet_manager.list_models() {
            if !status.downloaded {
                continue;
            }
            models.push(ShareableRemoteModelInfo {
                id: status.name.clone(),
                display_name: status.display_name.clone(),
                engine: "parakeet".to_string(),
                recommended: Some(status.recommended),
                speed_score: Some(status.speed_score),
                accuracy_score: Some(status.accuracy_score),
            });
        }
    }

    models.sort_by(|left, right| {
        left.engine
            .cmp(&right.engine)
            .then(left.display_name.cmp(&right.display_name))
    });
    models
}

fn current_model_info(
    model_name: &str,
    engine: &str,
    available: &[ShareableRemoteModelInfo],
) -> ShareableRemoteModelInfo {
    available
        .iter()
        .find(|model| model.id == model_name && model.engine == engine)
        .cloned()
        .unwrap_or_else(|| ShareableRemoteModelInfo {
            id: model_name.to_string(),
            display_name: model_name.to_string(),
            engine: engine.to_string(),
            recommended: None,
            speed_score: None,
            accuracy_score: None,
        })
}

pub async fn build_remote_model_control_snapshot(
    app: &AppHandle,
    shared_state: &SharedServerState,
) -> Result<RemoteModelControlSnapshot, String> {
    let available = list_shareable_remote_models(app).await;
    let model_name = shared_state.get_model_name();
    let engine = shared_state.get_engine();
    let current = current_model_info(&model_name, &engine, &available);

    Ok(RemoteModelControlSnapshot { current, available })
}

pub async fn update_remote_shared_model(
    app: &AppHandle,
    shared_state: &SharedServerState,
    model_id: &str,
    engine: &str,
) -> Result<RemoteModelControlSnapshot, String> {
    let (model_path, resolved_engine) =
        resolve_shareable_model_config(app, model_id, engine).await?;

    shared_state.update_model(
        model_id.to_string(),
        model_path.clone(),
        resolved_engine.clone(),
    );

    if let Some(server_manager) = app.try_state::<AsyncMutex<RemoteServerManager>>() {
        let mut manager = server_manager.lock().await;
        if manager.is_running() {
            manager.update_model(model_path, model_id.to_string(), resolved_engine.clone());
        }
    }

    let snapshot = build_remote_model_control_snapshot(app, shared_state).await?;

    let _ = app.emit(
        "sharing-status-changed",
        serde_json::json!({
            "refresh": true,
            "model_name": model_id,
        }),
    );
    let _ = app.emit(
        "remote-shared-model-changed",
        serde_json::json!({
            "model": model_id,
            "engine": resolved_engine,
        }),
    );

    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_model_info_falls_back_when_not_in_available_list() {
        let available = vec![ShareableRemoteModelInfo {
            id: "base.en".to_string(),
            display_name: "Base (English)".to_string(),
            engine: "whisper".to_string(),
            recommended: Some(false),
            speed_score: Some(8),
            accuracy_score: Some(5),
        }];

        let current = current_model_info("large-v3", "whisper", &available);
        assert_eq!(current.id, "large-v3");
        assert_eq!(current.display_name, "large-v3");
        assert_eq!(current.engine, "whisper");
        assert!(current.recommended.is_none());
    }
}
