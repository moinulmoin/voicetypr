use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct DistributionInfo {
    pub channel: &'static str,
    pub is_store_install: bool,
    pub package_family_name: Option<String>,
}

#[tauri::command]
pub fn get_distribution_info() -> DistributionInfo {
    let package_family_name = package_family_name();
    let is_store_install = package_family_name.is_some();

    distribution_info(package_family_name, is_store_install)
}

pub(crate) fn is_store_install() -> bool {
    package_family_name().is_some()
}

fn distribution_info(
    package_family_name: Option<String>,
    is_store_install: bool,
) -> DistributionInfo {
    DistributionInfo {
        channel: if is_store_install {
            "store_msix"
        } else {
            "direct"
        },
        is_store_install,
        package_family_name,
    }
}

#[cfg(target_os = "windows")]
fn package_family_name() -> Option<String> {
    use windows::ApplicationModel::Package;

    let package = Package::Current().ok()?;
    let id = package.Id().ok()?;
    let family_name = id.FamilyName().ok()?;
    Some(family_name.to_string())
}

#[cfg(not(target_os = "windows"))]
fn package_family_name() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::get_distribution_info;

    #[test]
    fn non_windows_distribution_defaults_to_direct() {
        #[cfg(not(target_os = "windows"))]
        {
            let info = get_distribution_info();
            assert_eq!(info.channel, "direct");
            assert!(!info.is_store_install);
            assert!(info.package_family_name.is_none());
        }
    }
}
