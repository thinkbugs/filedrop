//! LAN file transfer: a Snapdrop-compatible page a phone opens after scanning
//! the QR code, and the desktop appearing in its peer list as `rtcSupported:
//! false`, so a phone that picks the desktop relays its file over the WebSocket
//! the page already speaks. `/upload` is the plain multipart door used by other
//! FileDrop instances and by anything that would rather POST.
//!
//! The server itself is `server.rs`; this module is the owner the UI talks to.

pub mod chat;
pub mod client;
pub mod config;
pub mod discovery;
pub mod outbox;
pub mod server;
pub mod stage;

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use discovery::FileDiscovery;
use server::Started;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceivedFile {
    pub name: String,
    pub path: String,
    pub bytes: u64,
    /// Milliseconds since the Unix epoch, for "3 分钟前".
    pub at: u64,
}

pub struct Transfer {
    started: Mutex<Option<Started>>,
    /// Advertised while the server runs; dropping it withdraws the service.
    discovery: Mutex<Option<FileDiscovery>>,
    /// Newest first. Session-only: the folder on disk is the record.
    recent: Mutex<Vec<ReceivedFile>>,
    config: Mutex<config::Config>,
    last_error: Mutex<Option<String>>,
    /// Auto-start and the first UI render can both call start; one bind at a time.
    start_guard: tokio::sync::Mutex<()>,
}

impl Transfer {
    pub fn new(config: config::Config) -> Self {
        Self {
            started: Mutex::new(None),
            discovery: Mutex::new(None),
            recent: Mutex::new(Vec::new()),
            config: Mutex::new(config),
            last_error: Mutex::new(None),
            start_guard: tokio::sync::Mutex::new(()),
        }
    }

    pub fn is_running(&self) -> bool {
        self.started
            .lock()
            .map(|slot| slot.is_some())
            .unwrap_or(false)
    }

    pub fn url(&self) -> Option<String> {
        self.started
            .lock()
            .ok()
            .and_then(|slot| slot.as_ref().map(|started| started.url.clone()))
    }

    pub fn config(&self) -> config::Config {
        self.config
            .lock()
            .map(|config| config.clone())
            .unwrap_or_else(|_| config::Config {
                device_name: "FileDrop".to_string(),
                storage_dir: "FileDrop".to_string(),
                port: 8787,
                auto_start: true,
                notify: true,
                instance_id: uuid::Uuid::new_v4().to_string(),
            })
    }

    pub fn set_config(&self, next: config::Config) {
        if let Ok(mut config) = self.config.lock() {
            *config = next;
        }
    }

    pub fn start_guard(&self) -> &tokio::sync::Mutex<()> {
        &self.start_guard
    }

    pub fn set_started(&self, started: Started) {
        if let Ok(mut slot) = self.started.lock() {
            *slot = Some(started);
        }
    }

    pub fn take(&self) -> Option<Started> {
        if let Ok(mut slot) = self.discovery.lock() {
            slot.take();
        }
        self.started.lock().ok().and_then(|mut slot| slot.take())
    }

    pub fn set_discovery(&self, discovery: FileDiscovery) {
        if let Ok(mut slot) = self.discovery.lock() {
            *slot = Some(discovery);
        }
    }

    /// Other FileDrop instances that advertise the file service.
    pub fn peers(&self) -> Vec<discovery::Peer> {
        self.discovery
            .lock()
            .ok()
            .and_then(|slot| slot.as_ref().map(FileDiscovery::peers))
            .unwrap_or_default()
    }

    pub fn set_error(&self, error: Option<String>) {
        if let Ok(mut slot) = self.last_error.lock() {
            *slot = error;
        }
    }

    pub fn last_error(&self) -> Option<String> {
        self.last_error.lock().ok().and_then(|slot| slot.clone())
    }

    pub fn record(&self, name: String, path: String, bytes: u64) {
        let at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|since| since.as_millis() as u64)
            .unwrap_or(0);
        if let Ok(mut recent) = self.recent.lock() {
            recent.insert(
                0,
                ReceivedFile {
                    name,
                    path,
                    bytes,
                    at,
                },
            );
            recent.truncate(100);
        }
    }

    pub fn recent(&self) -> Vec<ReceivedFile> {
        self.recent
            .lock()
            .map(|recent| recent.clone())
            .unwrap_or_default()
    }
}

/// One received file, from any of the three doors (relay, text, multipart): keep
/// it for the page's list and tell the window.
pub fn notify_file(app: &tauri::AppHandle, name: String, path: String, bytes: u64) {
    use tauri::{Emitter, Manager};
    if let Some(transfer) = app.try_state::<Transfer>() {
        transfer.record(name.clone(), path.clone(), bytes);
    }
    let _ = app.emit(
        "transfer-file",
        serde_json::json!({ "name": name, "path": path, "bytes": bytes }),
    );
}
