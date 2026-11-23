//! Transfer settings, persisted next to the rest of this app's state.
//!
//! Every field is optional on disk so a config written by an older build still
//! loads: the missing values come from the platform defaults computed at
//! startup.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub device_name: String,
    pub storage_dir: String,
    pub port: u16,
    pub auto_start: bool,
    /// System notification when a file or message lands, used when the window
    /// is hidden in the tray and the transfer would otherwise go unseen.
    pub notify: bool,
    /// Stable across restarts: mDNS advertises it and chat messages carry it,
    /// so a conversation survives DHCP leases and new sockets.
    pub instance_id: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawConfig {
    device_name: Option<String>,
    storage_dir: Option<String>,
    port: Option<u16>,
    auto_start: Option<bool>,
    notify: Option<bool>,
    instance_id: Option<String>,
}

impl Config {
    pub fn defaults(app: &AppHandle) -> Self {
        Self {
            device_name: default_device_name(),
            storage_dir: default_storage(app),
            port: 8787,
            auto_start: true,
            notify: true,
            instance_id: uuid::Uuid::new_v4().to_string(),
        }
    }

    pub fn load(path: &Path, defaults: Config, old_storage_default: Option<String>) -> Self {
        let raw = std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str::<RawConfig>(&text).ok())
            .unwrap_or_default();
        Self {
            device_name: resolve_device_name(raw.device_name, &defaults),
            storage_dir: resolve_storage_dir(
                raw.storage_dir,
                old_storage_default.as_deref(),
                defaults.storage_dir,
            ),
            port: raw.port.filter(|port| *port > 0).unwrap_or(defaults.port),
            auto_start: raw.auto_start.unwrap_or(defaults.auto_start),
            notify: raw.notify.unwrap_or(defaults.notify),
            instance_id: raw
                .instance_id
                .map(|id| id.trim().to_string())
                .filter(|id| !id.is_empty())
                .unwrap_or(defaults.instance_id),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|error| error.to_string())?;
        std::fs::write(path, text).map_err(|error| error.to_string())
    }
}

/// Where received files go when nothing else will take them.
///
/// The app's own data directory is the one place a phone lets this process
/// write without asking the user for anything, which is why it is also the
/// fallback when a shared folder cannot be claimed.
#[cfg_attr(
    not(any(target_os = "android", target_os = "ios")),
    allow(dead_code)
)]
pub fn private_storage(app: &AppHandle) -> String {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("FileDrop").to_string_lossy().to_string())
        .unwrap_or_else(|_| "FileDrop".to_string())
}

/// Primary shared storage. The settings page asks the platform for the real
/// root when the interface is up; this is the same path for the owner of the
/// device and is what a fresh install defaults to, before anything can be asked.
#[cfg(target_os = "android")]
pub const SHARED_ROOT: &str = "/storage/emulated/0";

fn default_storage(app: &AppHandle) -> String {
    // Files someone sends to a phone are only useful if they can be found, so a
    // fresh install lands in Downloads like every other app; `landing_path`
    // falls back to the private directory if the platform refuses that.
    #[cfg(target_os = "android")]
    {
        // Nothing is asked of `app` here: the default is a shared-storage folder,
        // and the private directory it would give is the fallback, not the
        // destination.
        let _ = app;
        return format!("{SHARED_ROOT}/Download/FileDrop");
    }

    // iOS sandboxes every path the app can write without a runtime permission.
    #[cfg(target_os = "ios")]
    {
        return private_storage(app);
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        if let Ok(home) = app.path().home_dir() {
            return home
                .join("Downloads")
                .join("FileDrop")
                .to_string_lossy()
                .to_string();
        }
        "FileDrop".to_string()
    }
}

fn default_device_name() -> String {
    #[cfg(target_os = "android")]
    {
        if let Some(name) = android_device_name() {
            return name;
        }
    }

    let candidate = std::env::var("COMPUTERNAME")
        .ok()
        .or_else(|| std::env::var("HOSTNAME").ok())
        .or_else(|| std::fs::read_to_string("/proc/sys/kernel/hostname").ok());
    candidate
        .map(|name| name.trim().trim_end_matches('.').to_string())
        .filter(|name| !name.is_empty() && !name.eq_ignore_ascii_case("localhost"))
        .unwrap_or_else(|| FALLBACK_NAME.to_string())
}

/// The name every build fell back to when it could not read one.
const FALLBACK_NAME: &str = "FileDrop";

/// Android has no hostname to read — `/proc/sys/kernel/hostname` is SELinux
/// denied to app processes and neither environment variable is set — so the
/// name comes from the public build properties instead. The marketing name is
/// what the owner sees under 设置 → 关于本机, which is also what they will look
/// for in the device list; brand+model is the fallback when a ROM leaves it
/// empty.
#[cfg(target_os = "android")]
fn android_device_name() -> Option<String> {
    if let Some(market) = system_property("ro.product.marketname")
        .or_else(|| system_property("ro.config.marketing_name"))
    {
        return Some(market);
    }
    match (
        system_property("ro.product.brand"),
        system_property("ro.product.model"),
    ) {
        (Some(brand), Some(model)) => Some(format!("{brand} {model}")),
        (Some(brand), None) => Some(brand),
        (None, Some(model)) => Some(model),
        (None, None) => None,
    }
}

#[cfg(target_os = "android")]
fn system_property(key: &str) -> Option<String> {
    use std::ffi::CString;
    use std::os::raw::{c_char, c_int};

    extern "C" {
        fn __system_property_get(name: *const c_char, value: *mut c_char) -> c_int;
    }

    // The callee never writes more than PROP_VALUE_MAX bytes, so a wider buffer
    // only costs nothing on old releases and picks up the longer values newer
    // ones allow.
    let mut buf = vec![0u8; 256];
    let name = CString::new(key).ok()?;
    let length =
        unsafe { __system_property_get(name.as_ptr(), buf.as_mut_ptr() as *mut c_char) };
    if length <= 0 {
        return None;
    }
    let value = String::from_utf8_lossy(&buf[..length as usize])
        .trim()
        .to_string();
    // "unknown" is what an unconfigured partition reports, not a device name.
    if value.is_empty() || value.eq_ignore_ascii_case("unknown") || value == "null" {
        return None;
    }
    Some(value)
}

/// A stored name this app wrote for a device it could not read, now that there
/// is a real one to put in its place. Only a placeholder counts: a name the user
/// typed stays theirs even if it happens to match the derived one.
#[cfg(any(test, target_os = "android"))]
fn is_stale_placeholder(stored: &str, derived: &str) -> bool {
    stored == FALLBACK_NAME && derived != FALLBACK_NAME
}

fn resolve_device_name(stored: Option<String>, defaults: &Config) -> String {
    let stored = stored
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty());
    match stored {
        Some(stored) => {
            // Only phones shipped with the placeholder: the desktop always could
            // read a hostname, so "FileDrop" there is somebody's choice.
            #[cfg(target_os = "android")]
            if is_stale_placeholder(&stored, &defaults.device_name) {
                return defaults.device_name.clone();
            }
            stored
        }
        None => defaults.device_name.clone(),
    }
}

/// Where received files go, honouring a folder the user chose.
///
/// The folder this used to default to on a phone — the app's own data directory —
/// could not be picked through the interface, so an install still carrying it
/// never expressed a choice. Those move to the shared folder, which is the one
/// the user can find; a value somebody typed or picked is kept as it is.
fn resolve_storage_dir(
    stored: Option<String>,
    old_default: Option<&str>,
    new_default: String,
) -> String {
    match stored
        .map(|dir| dir.trim().to_string())
        .filter(|dir| !dir.is_empty())
    {
        Some(dir) if old_default.is_some_and(|old| old == dir) => new_default,
        Some(dir) => dir,
        None => new_default,
    }
}

pub fn config_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|error| format!("没有配置目录：{error}"))?;
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    Ok(dir.join("filedrop.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_placeholder_name_gives_way_to_a_real_one() {
        assert!(is_stale_placeholder("FileDrop", "荣耀 Magic6 Pro"));
    }

    #[test]
    fn a_name_somebody_chose_is_left_alone() {
        assert!(!is_stale_placeholder("客厅的手机", "荣耀 Magic6 Pro"));
        assert!(!is_stale_placeholder("FileDrop", "FileDrop"));
    }

    #[test]
    fn a_blank_stored_name_falls_back_to_the_derived_one() {
        let defaults = Config {
            device_name: "荣耀 Magic6 Pro".into(),
            storage_dir: String::new(),
            port: 8787,
            auto_start: true,
            notify: true,
            instance_id: "id".into(),
        };
        assert_eq!(
            resolve_device_name(Some("   ".into()), &defaults),
            "荣耀 Magic6 Pro"
        );
        assert_eq!(resolve_device_name(None, &defaults), "荣耀 Magic6 Pro");
    }

    #[test]
    fn an_old_private_default_gives_way_to_the_shared_folder() {
        let old = "/data/user/0/com.filedrop.app/files/FileDrop";
        assert_eq!(
            resolve_storage_dir(
                Some(old.into()),
                Some(old),
                "/storage/emulated/0/Download/FileDrop".into()
            ),
            "/storage/emulated/0/Download/FileDrop"
        );
    }

    #[test]
    fn a_folder_somebody_chose_is_left_alone() {
        assert_eq!(
            resolve_storage_dir(
                Some("/storage/emulated/0/Download/工作".into()),
                Some("/data/user/0/old"),
                "/storage/emulated/0/Download/FileDrop".into()
            ),
            "/storage/emulated/0/Download/工作"
        );
        assert_eq!(
            resolve_storage_dir(
                Some("  /storage/emulated/0/Documents  ".into()),
                Some("/data/user/0/old"),
                "/new".into()
            ),
            "/storage/emulated/0/Documents"
        );
    }

    #[test]
    fn a_blank_stored_folder_falls_back_to_the_new_default() {
        assert_eq!(
            resolve_storage_dir(Some("   ".into()), Some("/data/old"), "/new".into()),
            "/new"
        );
        assert_eq!(
            resolve_storage_dir(None, Some("/data/old"), "/new".into()),
            "/new"
        );
    }
}
