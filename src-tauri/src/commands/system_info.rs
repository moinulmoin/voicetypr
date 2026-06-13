use serde::Serialize;
use sysinfo::System;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemSpecs {
    pub os_name: String,
    pub os_version: String,
    pub kernel_version: String,
    pub arch: String,
    pub cpu_brand: String,
    pub cpu_cores: u32,
    pub total_memory_mb: u64,
    pub gpus: Vec<String>,
}

#[tauri::command]
pub fn get_system_specs() -> SystemSpecs {
    let mut sys = System::new_all();
    sys.refresh_memory();
    sys.refresh_cpu_all();

    let os_name = System::name().unwrap_or_else(|| "Unknown".into());
    let os_version = System::long_os_version()
        .or_else(System::os_version)
        .unwrap_or_else(|| "Unknown".into());
    let kernel_version = System::kernel_version().unwrap_or_else(|| "Unknown".into());
    let arch = System::cpu_arch();
    let cpu_brand = sys
        .cpus()
        .first()
        .map(|c| c.brand().trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Unknown".into());
    let cpu_cores = System::physical_core_count()
        .map(|c| c as u32)
        .unwrap_or_else(|| sys.cpus().len() as u32);
    let total_memory_mb = sys.total_memory() / 1_048_576;

    SystemSpecs {
        os_name,
        os_version,
        kernel_version,
        arch,
        cpu_brand,
        cpu_cores,
        total_memory_mb,
        gpus: detect_gpus(),
    }
}

#[cfg(target_os = "windows")]
fn detect_gpus() -> Vec<String> {
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};

    let mut gpus = Vec::new();
    unsafe {
        let factory: IDXGIFactory1 = match CreateDXGIFactory1() {
            Ok(f) => f,
            Err(e) => {
                log::warn!("DXGI factory create failed: {:?}", e);
                return gpus;
            }
        };
        let mut index = 0u32;
        loop {
            let adapter = match factory.EnumAdapters(index) {
                Ok(a) => a,
                Err(_) => break,
            };
            if let Ok(desc) = adapter.GetDesc() {
                let name = String::from_utf16_lossy(&desc.Description);
                let name = name.trim_end_matches('\u{0}').trim().to_string();
                if !name.is_empty() {
                    gpus.push(name);
                }
            }
            index += 1;
        }
    }
    gpus
}

#[cfg(not(target_os = "windows"))]
fn detect_gpus() -> Vec<String> {
    Vec::new()
}
