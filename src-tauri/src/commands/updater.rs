use serde::Serialize;
use tauri::{AppHandle, Url};
use tauri_plugin_store::StoreExt;
use tauri_plugin_updater::{Updater, UpdaterExt};

use crate::commands::distribution;

pub const STABLE_UPDATE_ENDPOINT: &str =
    "https://github.com/moinulmoin/voicetypr/releases/latest/download/latest.json";
pub const BETA_UPDATE_ENDPOINT: &str =
    "https://github.com/moinulmoin/voicetypr/releases/download/beta/latest.json";

static UPDATE_OPERATION: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateChannel {
    Stable,
    Beta,
}

impl UpdateChannel {
    pub fn from_stored(value: Option<&str>) -> Self {
        match value {
            Some("beta") => Self::Beta,
            _ => Self::Stable,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Beta => "beta",
        }
    }

    const fn endpoint(self) -> &'static str {
        match self {
            Self::Stable => STABLE_UPDATE_ENDPOINT,
            Self::Beta => BETA_UPDATE_ENDPOINT,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AppUpdateInfo {
    pub version: String,
    pub body: String,
    pub channel: &'static str,
}

fn selected_channel(app: &AppHandle) -> Result<UpdateChannel, String> {
    let store = app.store("settings").map_err(|error| error.to_string())?;
    let stored = store
        .get("update_channel")
        .and_then(|value| value.as_str().map(str::to_owned));
    Ok(UpdateChannel::from_stored(stored.as_deref()))
}

fn updater_for_channel(app: &AppHandle, channel: UpdateChannel) -> Result<Updater, String> {
    let endpoint = channel
        .endpoint()
        .parse::<Url>()
        .map_err(|error| format!("Invalid {} update endpoint: {error}", channel.as_str()))?;

    app.updater_builder()
        .endpoints(vec![endpoint])
        .map_err(|error| error.to_string())?
        .build()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn check_for_app_update(app: AppHandle) -> Result<Option<AppUpdateInfo>, String> {
    if distribution::is_store_install() {
        return Ok(None);
    }

    let _operation = UPDATE_OPERATION.lock().await;

    let channel = selected_channel(&app)?;
    let update = updater_for_channel(&app, channel)?
        .check()
        .await
        .map_err(|error| error.to_string())?;

    Ok(update.map(|update| AppUpdateInfo {
        version: update.version,
        body: update.body.unwrap_or_default(),
        channel: channel.as_str(),
    }))
}

#[tauri::command]
pub async fn install_app_update(app: AppHandle, expected_version: String) -> Result<(), String> {
    if distribution::is_store_install() {
        return Err("Updates are managed by Microsoft Store".to_string());
    }

    let _operation = UPDATE_OPERATION.lock().await;

    let channel = selected_channel(&app)?;
    let update = updater_for_channel(&app, channel)?
        .check()
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "No update is currently available".to_string())?;

    if update.version != expected_version {
        return Err(format!(
            "Available update changed from {expected_version} to {}; check again before installing",
            update.version
        ));
    }

    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{UpdateChannel, BETA_UPDATE_ENDPOINT, STABLE_UPDATE_ENDPOINT};

    #[test]
    fn stored_channel_defaults_to_stable() {
        assert_eq!(UpdateChannel::from_stored(None), UpdateChannel::Stable);
        assert_eq!(
            UpdateChannel::from_stored(Some("stable")),
            UpdateChannel::Stable
        );
        assert_eq!(
            UpdateChannel::from_stored(Some("unexpected")),
            UpdateChannel::Stable
        );
    }

    #[test]
    fn beta_channel_requires_exact_beta_value() {
        assert_eq!(
            UpdateChannel::from_stored(Some("beta")),
            UpdateChannel::Beta
        );
        assert_eq!(UpdateChannel::Beta.as_str(), "beta");
    }

    #[test]
    fn channels_use_isolated_manifests() {
        assert_eq!(UpdateChannel::Stable.endpoint(), STABLE_UPDATE_ENDPOINT);
        assert_eq!(UpdateChannel::Beta.endpoint(), BETA_UPDATE_ENDPOINT);
        assert_ne!(
            UpdateChannel::Stable.endpoint(),
            UpdateChannel::Beta.endpoint()
        );
    }
}
