mod commands;
mod net;
mod shell;
mod transfer;

#[cfg(target_os = "android")]
mod android;

#[cfg(windows)]
mod notify_win;

use commands::{
    chat_add_manual, chat_ensure_peer, chat_list, chat_mark_read, chat_messages,
    chat_send_files, chat_send_text, chat_stage_append, chat_stage_cancel, chat_stage_open,
    open_url, send_stat, send_take, shell_menu_set, shell_menu_state, transfer_open_dir,
    transfer_open_file, transfer_keep_alive, transfer_send, transfer_set_config,
    transfer_battery_request, transfer_battery_state, transfer_set_notify, transfer_start,
    transfer_status, transfer_stop, transfer_storage_roots, transfer_take_shared,
};
use tauri::Manager;
#[cfg(desktop)]
use tauri::WindowEvent;
use transfer::chat::Chats;
use transfer::config::Config;
use transfer::Transfer;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run_app() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default();

    // Explorer's `FileDrop 传送` starts a second process with the chosen files;
    // that hand-off has to reach the instance already sitting in the tray. The
    // plugin runs before the others so a second process stops before setup.
    #[cfg(desktop)]
    {
        builder = builder.plugin(
            tauri_plugin_single_instance::Builder::new()
                .callback(|app, argv, _cwd| {
                    let paths = shell::send_paths(&argv);
                    let count = app
                        .try_state::<shell::SendQueue>()
                        .map(|queue| queue.push(paths.clone()))
                        .unwrap_or(0);
                    if count > 0 {
                        use tauri::Emitter;
                        let _ = app.emit(
                            "send-to-request",
                            serde_json::json!({ "paths": paths, "count": count }),
                        );
                    }
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.unminimize();
                        let _ = window.set_focus();
                    }
                })
                .build(),
        );
    }

    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let handle = app.handle().clone();
            #[cfg(windows)]
            notify_win::claim_app_id();
            let path = transfer::config::config_path(&handle)?;
            let defaults = Config::defaults(&handle);
            // A phone could never show the folder this build used to default
            // to, so installs still carrying it are moved to the shared one.
            #[cfg(target_os = "android")]
            let old_storage_default = Some(transfer::config::private_storage(&handle));
            #[cfg(not(target_os = "android"))]
            let old_storage_default = None;
            let config = Config::load(&path, defaults, old_storage_default);
            let _ = config.save(&path);
            let auto_start = config.auto_start;

            // Chat events must reach the window even when the listener is
            // stopped; outgoing messages are recorded either way.
            transfer::server::attach_app(handle.clone());
            app.manage(Chats::load(transfer::chat::storage_path(&handle)));
            app.manage(Transfer::new(config));
            app.manage(shell::SendQueue::default());

            #[cfg(windows)]
            if let Some(window) = app.get_webview_window("main") {
                clear_window_border(&window);
            }

            // Files handed over before the window existed: the webview drains
            // the queue once it has subscribed, so nothing depends on timing.
            let launched = shell::send_paths(&std::env::args().collect::<Vec<String>>());
            if !launched.is_empty() {
                let count = {
                    let queue = app.state::<shell::SendQueue>();
                    queue.push(launched)
                };
                if count > 0 {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
            }

            #[cfg(desktop)]
            if let Err(error) = setup_tray(app) {
                tracing::warn!("Tray unavailable: {error}");
            }

            if auto_start {
                let handle = handle.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(error) = commands::start_transfer(&handle, None).await {
                        tracing::warn!("FileDrop auto-start failed: {error}");
                    }
                });
            }

            // Sends to a device that was away retry themselves from here, so
            // they still go out when the window is closed or backgrounded.
            transfer::outbox::spawn(handle.clone());

            Ok(())
        })
        .on_window_event(|window, event| {
            #[cfg(desktop)]
            {
                if window.label() != "main" {
                    return;
                }
                // The server is meant to keep receiving while the window is out
                // of the way; the tray is the way back.
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
            #[cfg(mobile)]
            {
                let _ = (window, event);
            }
        })
        .invoke_handler(tauri::generate_handler![
            transfer_start,
            transfer_keep_alive,
            transfer_stop,
            transfer_status,
            transfer_set_config,
            transfer_set_notify,
            transfer_open_dir,
            transfer_open_file,
            transfer_storage_roots,
            transfer_take_shared,
            transfer_battery_state,
            transfer_battery_request,
            transfer_send,
            chat_list,
            chat_messages,
            chat_mark_read,
            chat_ensure_peer,
            chat_add_manual,
            chat_send_text,
            chat_send_files,
            chat_stage_open,
            chat_stage_append,
            chat_stage_cancel,
            send_take,
            send_stat,
            shell_menu_state,
            shell_menu_set,
            open_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running FileDrop");
}

#[cfg(windows)]
/// Windows 11 paints a one-pixel accent-coloured line along the top edge of an
/// active window. FileDrop's own layout already carries the visual weight, so
/// the line only reads as a stray border; `DWMWA_COLOR_NONE` asks DWM to skip
/// it. The attribute applies on the next composition, so a single call after
/// the window exists is enough.
fn clear_window_border(window: &tauri::WebviewWindow) {
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE,
    };
    if let Ok(hwnd) = window.hwnd() {
        let color = DWMWA_COLOR_NONE;
        unsafe {
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_BORDER_COLOR,
                &color as *const u32 as *const core::ffi::c_void,
                std::mem::size_of::<u32>() as u32,
            );
        }
    }
}

#[cfg(desktop)]
fn setup_tray(app: &mut tauri::App) -> tauri::Result<()> {
    use tauri::menu::{MenuBuilder, MenuItemBuilder};
    use tauri::tray::{TrayIconBuilder, TrayIconEvent};

    let show = MenuItemBuilder::with_id("show", "显示 FileDrop").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "退出").build(app)?;
    let menu = MenuBuilder::new(app).items(&[&show, &quit]).build()?;

    TrayIconBuilder::new()
        .tooltip("FileDrop · 局域网文件传送")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::DoubleClick { .. } = event {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .icon(app.default_window_icon().unwrap().clone())
        .build(app)?;

    Ok(())
}
