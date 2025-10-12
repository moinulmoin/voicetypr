use crate::license::device;

/// Returns the stable, privacy-preserving device identifier used for licensing
#[tauri::command]
pub async fn get_device_id() -> Result<String, String> {
    device::get_device_hash()
}
