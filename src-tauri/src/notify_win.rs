//! Windows toasts that react to a click.
//!
//! `tauri-plugin-notification` shows a toast through notify-rust, but on
//! Windows nothing ever waits for the activation event, so clicking one was a
//! no-op — the only way into the conversation was to find the window yourself.
//! This shows the toast on a worker thread and waits for the click there.

use tauri::{AppHandle, Emitter, Manager};

/// The identifier the plugin registers notifications under; using the same one
/// keeps FileDrop as the toast's source instead of "PowerShell".
const APP_ID: &str = "com.filedrop.app";

/// Make this process own [`APP_ID`].
///
/// The installer's shortcut carries the id, so toasts only worked when the app
/// was started from it — launched straight from the exe (autostart, a debug
/// run) the toasts were dropped by the shell without a word. Setting it on the
/// process makes every launch path equal.
pub fn claim_app_id() {
    use windows::core::HSTRING;
    use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;
    if let Err(error) =
        unsafe { SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(APP_ID)) }
    {
        tracing::warn!("Could not claim the notification app id: {error}");
    }
}

pub fn show(app: &AppHandle, title: String, body: String, peer_id: Option<String>) {
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("filedrop-toast".into())
        .spawn(move || {
            let handle = match notify_rust::Notification::new()
                .app_id(APP_ID)
                .summary(&title)
                .body(&body)
                .show()
            {
                Ok(handle) => handle,
                Err(error) => {
                    tracing::warn!("Toast failed: {error}");
                    return;
                }
            };
            tracing::info!("Toast shown: {title}");
            // Blocks until the user acts or the toast goes away; a toast is
            // worth one short-lived thread.
            let _ = handle.wait_for_response(move |response: &notify_rust::NotificationResponse| {
                tracing::info!("Toast response: {response:?}");
                if !matches!(response, notify_rust::NotificationResponse::Default) {
                    return;
                }
                let app = app.clone();
                let ui = app.clone();
                let peer = peer_id.clone();
                let _ = app.run_on_main_thread(move || {
                    if let Some(window) = ui.get_webview_window("main") {
                        let _ = window.unminimize();
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                    if let Some(peer) = peer {
                        let _ = ui.emit("notification-open", serde_json::json!({ "peerId": peer }));
                    }
                });
            });
        });
    if let Err(error) = spawned {
        tracing::warn!("Notification thread failed: {error}");
    }
}
