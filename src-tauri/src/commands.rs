//! Commands behind the FileDrop window: start/stop the LAN transfer server,
//! hand out its QR code, remember where incoming files land, and push a file to
//! another FileDrop.

use std::path::PathBuf;
use std::time::Instant;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tracing::info;

use crate::transfer::{
    chat::{self, ChatMessage, ChatPeer, Chats},
    client,
    config::{self, Config},
    discovery::{FileDiscovery, Peer},
    outbox, server, stage, ReceivedFile, Transfer,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferStatus {
    pub running: bool,
    pub url: Option<String>,
    pub qr: Option<String>,
    pub device_name: String,
    pub storage_dir: String,
    pub port: u16,
    /// Stable identity other devices file this one's messages under.
    pub instance_id: String,
    /// Other FileDrop instances that can receive a file right now.
    pub peers: Vec<Peer>,
    pub auto_start: bool,
    /// System notification on an inbound file or message.
    pub notify: bool,
    pub mobile: bool,
    pub platform: String,
    pub lan_ip: String,
    pub version: String,
    pub last_error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferSnapshot {
    pub status: TransferStatus,
    pub received: Vec<ReceivedFile>,
}

async fn snapshot(app: &AppHandle, transfer: &Transfer) -> TransferSnapshot {
    let config = transfer.config();
    let url = transfer.url();
    if let Some(chats) = app.try_state::<Chats>() {
        let observed: Vec<(String, String, String, u16)> = transfer
            .peers()
            .iter()
            .map(|peer| (peer.id.clone(), peer.name.clone(), peer.host.clone(), peer.port))
            .collect();
        chats.sync_discovery(&observed);
    }
    let qr = url
        .as_deref()
        .and_then(|url| server::generate_qr_code(url).ok());
    TransferSnapshot {
        status: TransferStatus {
            running: transfer.is_running(),
            url,
            qr,
            device_name: config.device_name,
            storage_dir: config.storage_dir,
            port: config.port,
            instance_id: config.instance_id,
            peers: transfer.peers(),
            auto_start: config.auto_start,
            notify: config.notify,
            mobile: is_mobile(),
            platform: std::env::consts::OS.to_string(),
            lan_ip: crate::net::lan_ip(),
            version: app.package_info().version.to_string(),
            last_error: transfer.last_error(),
        },
        received: transfer.recent(),
    }
}

/// The static web app the phone opens. In a packaged build it is a bundle
/// resource; during development it is read from the source tree.
fn assets_dir(app: &AppHandle) -> Result<PathBuf, String> {
    // On Android the bundle resources are packed into the APK's asset store,
    // which a plain filesystem server cannot reach, so the page is unpacked to
    // the app data dir and served from there.
    #[cfg(target_os = "android")]
    {
        return unpack_web_assets(app);
    }

    #[cfg(not(target_os = "android"))]
    {
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Ok(dir) = app.path().resource_dir() {
            candidates.push(dir.join("assets").join("web"));
            candidates.push(dir.join("web"));
        }
        candidates.push(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("assets")
                .join("web"),
        );
        for candidate in candidates {
            if candidate.join("index.html").exists() {
                return Ok(candidate);
            }
        }
        Err("找不到文件传送页面资源（assets/web）".to_string())
    }
}

/// The transfer page, compiled into the binary because the APK's asset store is
/// opaque to `std::fs` at runtime.
#[cfg(target_os = "android")]
static WEB_ASSETS: include_dir::Dir<'_> =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/assets/web");

/// Writes the embedded page under the app data dir and returns that directory.
/// Re-unpacked on every start (a handful of KB) so the served page tracks app
/// updates instead of a stale first-run copy.
#[cfg(target_os = "android")]
fn unpack_web_assets(app: &AppHandle) -> Result<PathBuf, String> {
    fn write_dir(dir: &include_dir::Dir, dest: &std::path::Path) -> Result<(), String> {
        std::fs::create_dir_all(dest).map_err(|error| format!("无法创建页面目录：{error}"))?;
        for entry in dir.entries() {
            match entry {
                include_dir::DirEntry::File(file) => {
                    let path = dest.join(file.path());
                    if let Some(parent) = path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    std::fs::write(&path, file.contents())
                        .map_err(|error| format!("无法写入页面文件：{error}"))?;
                }
                include_dir::DirEntry::Dir(sub) => write_dir(sub, dest)?,
            }
        }
        Ok(())
    }

    let dest = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("没有应用数据目录：{error}"))?
        .join("web");
    write_dir(&WEB_ASSETS, &dest)?;
    if !dest.join("index.html").exists() {
        return Err("找不到文件传送页面资源（assets/web）".to_string());
    }
    Ok(dest)
}

/// Shared by the command and by auto-start; both can run at once, so the
/// transfer owns a start lock and re-checks `running` after taking it.
pub async fn start_transfer(
    app: &AppHandle,
    port: Option<u16>,
) -> Result<TransferSnapshot, String> {
    let transfer = app.state::<Transfer>();
    let _guard = transfer.start_guard().lock().await;
    if transfer.is_running() {
        return Ok(snapshot(app, &transfer).await);
    }

    let mut config = transfer.config();
    if let Some(port) = port {
        if port == 0 {
            return Err("端口必须在 1 到 65535 之间".to_string());
        }
        config.port = port;
    }
    if config.device_name.trim().is_empty() {
        return Err("设备名称不能为空".to_string());
    }

    let assets = assets_dir(app)?;
    let chats = app.state::<Chats>().inner().clone();
    let started = match server::start_snapdrop_server(
        config.port,
        assets,
        app.clone(),
        config.clone(),
        chats,
    )
    .await
    {
        Ok(started) => started,
        Err(error) => {
            transfer.set_error(Some(error.clone()));
            return Err(error);
        }
    };
    config.port = started.port;

    let instance = config.instance_id.clone();
    let discovery_warning = match FileDiscovery::start(&instance, &config.device_name, started.port) {
        Ok(discovery) => {
            transfer.set_discovery(discovery);
            None
        }
        // The page still works without peer discovery; say so instead of failing.
        Err(error) => {
            info!("Peer discovery unavailable: {error}");
            Some(format!("设备自动发现不可用，可用 IP:端口 手动添加：{error}"))
        }
    };

    if let Ok(path) = config::config_path(app) {
        let _ = config.save(&path);
    }
    transfer.set_config(config);
    transfer.set_started(started);
    transfer.set_error(discovery_warning);
    let url = transfer.url().unwrap_or_default();
    info!("FileDrop ready at {url}");

    // Android freezes a backgrounded process (and its sockets) unless a
    // foreground service keeps it resident, so receiving works with the screen
    // off just like on the desktop. Losing this only costs background receive;
    // the server itself is already up.
    #[cfg(target_os = "android")]
    keep_resident(app);

    Ok(snapshot(app, &transfer).await)
}

/// Ask the platform to hold this process alive for as long as the server runs.
///
/// The service intent has to be posted through the webview's JNI handle, and
/// auto-start fires while `setup` is still building the activity, when there is
/// no webview yet. Retrying is the difference between "the device drops off the
/// LAN when the app leaves the foreground" and not.
#[cfg(target_os = "android")]
fn keep_resident(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        for _ in 0..60 {
            let running = app
                .try_state::<Transfer>()
                .map(|transfer| transfer.is_running())
                .unwrap_or(false);
            if !running {
                return;
            }
            if crate::android::start_background_service(&app).is_ok() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        info!("Background receive service never became available");
    });
}

#[tauri::command]
pub async fn transfer_start(
    app: AppHandle,
    port: Option<u16>,
) -> Result<TransferSnapshot, String> {
    start_transfer(&app, port).await
}

/// Re-assert the resident service after the app came back to the foreground.
/// Android can end it while the app is away; the server is still listening, so
/// without this the device simply stops being discoverable until the next start.
#[tauri::command]
pub async fn transfer_keep_alive(
    app: AppHandle,
    transfer: State<'_, Transfer>,
) -> Result<(), String> {
    if !transfer.is_running() {
        return Ok(());
    }
    #[cfg(target_os = "android")]
    crate::android::start_background_service(&app)?;
    #[cfg(not(target_os = "android"))]
    let _ = app;
    Ok(())
}

#[tauri::command]
pub async fn transfer_stop(
    app: AppHandle,
    transfer: State<'_, Transfer>,
) -> Result<TransferSnapshot, String> {
    if let Some(started) = transfer.take() {
        started.stop();
        info!("FileDrop stopped");
    }
    #[cfg(target_os = "android")]
    if let Err(error) = crate::android::stop_background_service(&app) {
        info!("Background receive service stop failed: {error}");
    }
    transfer.set_error(None);
    Ok(snapshot(&app, &transfer).await)
}

#[tauri::command]
pub async fn transfer_status(
    app: AppHandle,
    transfer: State<'_, Transfer>,
) -> Result<TransferSnapshot, String> {
    Ok(snapshot(&app, &transfer).await)
}

#[tauri::command]
pub async fn transfer_set_config(
    app: AppHandle,
    transfer: State<'_, Transfer>,
    device_name: String,
    storage_dir: String,
    port: u16,
    auto_start: bool,
) -> Result<TransferSnapshot, String> {
    let name = device_name.trim();
    if name.is_empty() {
        return Err("设备名称不能为空".to_string());
    }
    if storage_dir.trim().is_empty() {
        return Err("保存目录不能为空".to_string());
    }
    if port == 0 {
        return Err("端口必须在 1 到 65535 之间".to_string());
    }
    if transfer.is_running() {
        return Err("传送已开启，先停止再修改设置。".to_string());
    }
    let current = transfer.config();
    let next = Config {
        device_name: name.to_string(),
        storage_dir: storage_dir.trim().to_string(),
        port,
        auto_start,
        notify: current.notify,
        instance_id: current.instance_id,
    };
    let path = config::config_path(&app)?;
    next.save(&path)?;
    transfer.set_config(next);
    transfer.set_error(None);
    Ok(snapshot(&app, &transfer).await)
}

/// The folders the settings page can offer on this platform.
///
/// A phone cannot browse the filesystem with a dialog, so the interface builds
/// its choices out of these two anchors: everything under `sharedRoot` is a
/// folder the gallery can show (the transfer server claims the file through
/// MediaStore), and `privateDir` is the one place that always works.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageRoots {
    pub shared_root: String,
    pub private_dir: String,
}

#[tauri::command]
pub async fn transfer_storage_roots(app: AppHandle) -> Result<StorageRoots, String> {
    #[cfg(target_os = "android")]
    {
        let shared_root = crate::android::external_root(&app)
            .unwrap_or_else(|_| config::SHARED_ROOT.to_string());
        return Ok(StorageRoots {
            shared_root,
            private_dir: config::private_storage(&app),
        });
    }

    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Err("该平台不需要选择保存位置".to_string())
    }
}

/// Files another app shared into FileDrop, as paths the send sheet can use.
///
/// The share sheet only hands out `content://` URIs, which this process cannot
/// open by path and which die with the activity, so the activity copies them
/// into the app's share inbox first. Taking moves them aside, which makes the
/// hand-over once-only and leaves the paths valid for the actual send.
#[tauri::command]
pub async fn transfer_take_shared(app: AppHandle) -> Result<Vec<String>, String> {
    #[cfg(target_os = "android")]
    {
        let inbox = PathBuf::from(crate::android::share_inbox(&app)?);
        let Ok(entries) = std::fs::read_dir(&inbox) else {
            return Ok(Vec::new());
        };
        let mut taken = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let handled = inbox.join("handled");
            std::fs::create_dir_all(&handled).map_err(|error| error.to_string())?;
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let mut target = handled.join(&name);
            if target.exists() {
                target = handled.join(format!("{}-{name}", uuid::Uuid::new_v4()));
            }
            std::fs::rename(&path, &target).map_err(|error| error.to_string())?;
            taken.push(target.to_string_lossy().into_owned());
        }
        taken.sort();
        return Ok(taken);
    }

    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Ok(Vec::new())
    }
}

/// Whether this device lets the receiver use the network with the screen off.
///
/// Doze suspends networking for apps that are not exempted, foregound services
/// and wake locks included — the other side then sees "online" and cannot
/// connect, so the settings page offers the exemption as a real toggle.
#[tauri::command]
pub async fn transfer_battery_state(app: AppHandle) -> Result<bool, String> {
    #[cfg(target_os = "android")]
    {
        return crate::android::battery_exempt(&app);
    }

    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Ok(true)
    }
}

#[tauri::command]
pub async fn transfer_battery_request(app: AppHandle) -> Result<bool, String> {
    #[cfg(target_os = "android")]
    {
        return crate::android::request_battery_exemption(&app);
    }

    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Ok(true)
    }
}

#[tauri::command]
pub async fn transfer_open_dir(app: AppHandle, path: String) -> Result<(), String> {
    let dir = PathBuf::from(&path);

    #[cfg(target_os = "android")]
    {
        // The folder is created by MediaStore when the first file lands in it,
        // so on a phone this only ever shows something that already exists.
        let _ = dir;
        if crate::android::open_folder(&app, &path)? {
            return Ok(());
        }
        Err(format!("这台手机没有可以显示该文件夹的应用，文件保存在：{path}"))
    }

    #[cfg(target_os = "ios")]
    {
        let _ = app;
        Err(format!("移动端文件保存在应用目录：{}", dir.to_string_lossy()))
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let _ = app;
        if !dir.exists() {
            std::fs::create_dir_all(&dir).map_err(|error| format!("无法创建目录：{error}"))?;
        }
        let mut command = if cfg!(target_os = "windows") {
            let mut command = std::process::Command::new("explorer");
            command.arg(&dir);
            command
        } else if cfg!(target_os = "macos") {
            let mut command = std::process::Command::new("open");
            command.arg(&dir);
            command
        } else {
            let mut command = std::process::Command::new("xdg-open");
            command.arg(&dir);
            command
        };
        command
            .spawn()
            .map_err(|error| format!("无法打开目录：{error}"))?;
        Ok(())
    }
}

#[tauri::command]
pub async fn transfer_open_file(app: AppHandle, path: String) -> Result<(), String> {
    let file = PathBuf::from(&path);
    if !file.is_file() {
        // Outgoing files are uploaded from a staged copy that is released once
        // the peer has it, so the path on an old card can legitimately be gone;
        // say that instead of a bare "not here".
        return Err("本机已经没有这个文件了：发出的文件在传输完成后会清理本地副本".to_string());
    }

    #[cfg(target_os = "android")]
    {
        crate::android::open_file(&app, path)
    }

    #[cfg(target_os = "ios")]
    {
        let _ = app;
        Err(format!(
            "移动端请从应用目录中查看：{}",
            file.to_string_lossy()
        ))
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let _ = app;
        // `start` needs a shell, and a shell shows a console window for the
        // split second it lives — the user sees a terminal flash and then the
        // file. Spawning it without a console leaves only the file.
        #[cfg(target_os = "windows")]
        let mut command = {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            let mut command = std::process::Command::new("cmd");
            command.args(["/C", "start", "", &path]);
            command.creation_flags(CREATE_NO_WINDOW);
            command
        };
        #[cfg(target_os = "macos")]
        let mut command = {
            let mut command = std::process::Command::new("open");
            command.arg(&file);
            command
        };
        #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
        let mut command = {
            let mut command = std::process::Command::new("xdg-open");
            command.arg(&file);
            command
        };
        command
            .spawn()
            .map_err(|error| format!("无法打开文件：{error}"))?;
        Ok(())
    }
}

/// Open a link in the system browser. Used by the update section, so the scheme
/// is pinned to http/https rather than handed to the shell unchecked.
#[tauri::command]
pub async fn open_url(app: AppHandle, url: String) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("只允许打开 http/https 链接".to_string());
    }

    #[cfg(target_os = "android")]
    {
        crate::android::open_url(&app, url)
    }

    #[cfg(target_os = "ios")]
    {
        let _ = app;
        Err(format!("移动端请复制链接到浏览器打开：{url}"))
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let _ = app;
        let mut command = if cfg!(target_os = "windows") {
            let mut command = std::process::Command::new("explorer");
            command.arg(&url);
            command
        } else if cfg!(target_os = "macos") {
            let mut command = std::process::Command::new("open");
            command.arg(&url);
            command
        } else {
            let mut command = std::process::Command::new("xdg-open");
            command.arg(&url);
            command
        };
        command
            .spawn()
            .map_err(|error| format!("无法打开链接：{error}"))?;
        Ok(())
    }
}

/// Send one or more files to another FileDrop. Progress is emitted per file so
/// the row the user started stays the only thing that moves.
#[tauri::command]
pub async fn transfer_send(
    app: AppHandle,
    host: String,
    port: u16,
    paths: Vec<String>,
) -> Result<usize, String> {
    let host = host.trim().to_string();
    if host.is_empty() {
        return Err("缺少对方地址".to_string());
    }
    if port == 0 {
        return Err("对方端口无效".to_string());
    }
    if paths.is_empty() {
        return Ok(0);
    }

    let target = format!("{host}:{port}");
    let count = paths.len();
    let mut sent = 0usize;

    for (index, path) in paths.iter().enumerate() {
        let file = PathBuf::from(path);
        if !file.is_file() {
            return Err(format!("不是文件：{}", file.display()));
        }
        let total = tokio::fs::metadata(&file)
            .await
            .map_err(|error| format!("读取文件失败：{error}"))?
            .len();
        let name = file
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "file".to_string());

        let _ = app.emit(
            "transfer-send-progress",
            serde_json::json!({
                "target": target,
                "name": name,
                "index": index + 1,
                "count": count,
                "sent": 0,
                "total": total,
                "progress": 0.0
            }),
        );

        let mut last_emit = Instant::now();
        let result = client::send_file(&host, port, &file, None, None, |bytes| {
            let now = Instant::now();
            if now.duration_since(last_emit).as_millis() >= 120 || bytes >= total {
                last_emit = now;
                let progress = if total > 0 {
                    (bytes as f64 / total as f64).min(1.0)
                } else {
                    1.0
                };
                let _ = app.emit(
                    "transfer-send-progress",
                    serde_json::json!({
                        "target": target,
                        "name": name,
                        "index": index + 1,
                        "count": count,
                        "sent": bytes,
                        "total": total,
                        "progress": progress
                    }),
                );
            }
        })
        .await;

        match result {
            Ok(_) => sent += 1,
            Err(error) => return Err(format!("{name}：{error}")),
        }
    }

    info!("Sent {sent} file(s) to {target}");
    Ok(sent)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub kind: String,
    pub last_at: u64,
    pub preview: String,
    pub last_kind: String,
    pub unread: u32,
    pub online: bool,
    /// Sends still waiting for this device to come back.
    pub queued: u32,
}

fn conversation(peer: ChatPeer, online: bool, queued: u32) -> Conversation {
    Conversation {
        id: peer.id,
        name: peer.name,
        host: peer.host,
        port: peer.port,
        kind: peer.kind,
        last_at: peer.last_at,
        preview: peer.preview,
        last_kind: peer.last_kind,
        unread: peer.unread,
        online,
        queued,
    }
}

#[tauri::command]
pub async fn chat_list(
    transfer: State<'_, Transfer>,
    chats: State<'_, Chats>,
) -> Result<Vec<Conversation>, String> {
    let peers = transfer.peers();
    let observed: Vec<(String, String, String, u16)> = peers
        .iter()
        .map(|peer| (peer.id.clone(), peer.name.clone(), peer.host.clone(), peer.port))
        .collect();
    chats.sync_discovery(&observed);

    let mut list = Vec::new();
    for peer in chats.list() {
        let queued = chats.queued_count(&peer.id) as u32;
        let online = if peer.kind == chat::KIND_BROWSER {
            server::browser_online(&peer.id).await
        } else {
            peers.iter().any(|candidate| candidate.id == peer.id)
        };
        list.push(conversation(peer, online, queued));
    }
    Ok(list)
}

#[tauri::command]
pub async fn chat_messages(
    chats: State<'_, Chats>,
    peer_id: String,
) -> Result<Vec<ChatMessage>, String> {
    Ok(chats.messages(&peer_id))
}

#[tauri::command]
pub async fn chat_mark_read(chats: State<'_, Chats>, peer_id: String) -> Result<(), String> {
    chats.mark_read(&peer_id);
    Ok(())
}

/// Opens (or refreshes) the conversation for a discovered device.
#[tauri::command]
pub async fn chat_ensure_peer(
    chats: State<'_, Chats>,
    id: String,
    name: String,
    host: String,
    port: u16,
) -> Result<Conversation, String> {
    if host.trim().is_empty() {
        return Err("缺少设备地址".to_string());
    }
    let peer = chats.ensure_peer(&id, name.trim(), host.trim(), port, chat::KIND_DEVICE);
    let queued = chats.queued_count(&peer.id) as u32;
    Ok(conversation(peer, true, queued))
}

/// Adds a device by `IP:port`; the health probe turns the address into the
/// remote instance id when it is reachable, so replies land in the same row.
#[tauri::command]
pub async fn chat_add_manual(
    chats: State<'_, Chats>,
    host: String,
    port: u16,
) -> Result<Conversation, String> {
    let host = host.trim().to_string();
    if host.is_empty() {
        return Err("请输入对方地址，例如 192.168.1.8".to_string());
    }
    if port == 0 {
        return Err("端口需要在 1 到 65535 之间".to_string());
    }
    let probe = client::probe(&host, port).await;
    let (id, name, endpoint, online) = match probe {
        Some((id, name, service_port)) => (
            id,
            if name.trim().is_empty() {
                host.clone()
            } else {
                name
            },
            if service_port > 0 { service_port } else { port },
            true,
        ),
        None => (format!("manual:{host}:{port}"), host.clone(), port, false),
    };
    let peer = chats.ensure_peer(&id, &name, &host, endpoint, chat::KIND_DEVICE);
    Ok(conversation(peer, online, 0))
}

#[tauri::command]
pub async fn chat_send_text(
    app: AppHandle,
    transfer: State<'_, Transfer>,
    chats: State<'_, Chats>,
    peer_id: String,
    text: String,
) -> Result<ChatMessage, String> {
    let text = text.trim_end().to_string();
    if text.trim().is_empty() {
        return Err("消息不能为空".to_string());
    }
    if text.chars().count() > 10_000 {
        return Err("消息太长了".to_string());
    }
    let peer = chats.peer(&peer_id).ok_or("这台设备还没有会话")?;
    if peer.kind != chat::KIND_BROWSER && (peer.host.trim().is_empty() || peer.port == 0) {
        return Err("还没有这台设备的地址，等它重新出现或手动添加".to_string());
    }

    // The bubble is written before the socket is opened, so a peer that is away
    // costs the sender nothing but a small clock on the message.
    let message = ChatMessage::text(&peer, "out", text.clone());
    let item = chat::QueuedItem {
        message_id: message.id.clone(),
        peer_id: peer.id.clone(),
        kind: "text".to_string(),
        text,
        path: String::new(),
        at: message.at,
    };
    let staged = stage::staged_dir(&app);
    let (queued, evicted) = chats.enqueue(message, item.clone());
    for evicted in evicted {
        stage::release(&staged, &evicted.path);
    }
    chat::broadcast(&chats, &queued);
    let delivered = outbox::send_now(&chats, &transfer, &item, &staged).await;
    Ok(delivered.unwrap_or(queued))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendOutcome {
    /// Bytes that reached the peer during this call.
    pub sent: usize,
    /// Files parked in the outbox because the device is away.
    pub queued: usize,
}

/// Sends files to a FileDrop device. The bubble is recorded before the upload
/// starts so progress has somewhere to land; a failure marks the same bubble,
/// and a device that is simply away keeps the bubble queued instead.
#[tauri::command]
pub async fn chat_send_files(
    app: AppHandle,
    transfer: State<'_, Transfer>,
    chats: State<'_, Chats>,
    peer_id: String,
    paths: Vec<String>,
) -> Result<SendOutcome, String> {
    let peer = chats.peer(&peer_id).ok_or("这台设备还没有会话")?;
    if peer.kind == chat::KIND_BROWSER {
        return Err("网页设备暂时只能接收文字；文件请在该手机上选择后发到这台电脑".to_string());
    }
    if peer.host.trim().is_empty() || peer.port == 0 {
        return Err("还没有这台设备的地址，等它重新出现或手动添加".to_string());
    }
    if paths.is_empty() {
        return Ok(SendOutcome { sent: 0, queued: 0 });
    }

    let config = transfer.config();
    let staged = stage::staged_dir(&app);
    let identity = client::SendIdentity {
        id: &config.instance_id,
        name: &config.device_name,
        port: config.port,
    };
    let mut sent = 0usize;
    let mut queued = 0usize;

    for path in paths {
        let file = PathBuf::from(&path);
        if !file.is_file() {
            return Err(format!("不是文件：{}", file.display()));
        }
        let total = tokio::fs::metadata(&file)
            .await
            .map_err(|error| format!("读取文件失败：{error}"))?
            .len();
        let name = file
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "file".to_string());

        let message = ChatMessage::file(&peer, "out", name.clone(), path.clone(), total);
        let message_id = message.id.clone();

        if !outbox::reachable(&transfer, &peer).await {
            let item = chat::QueuedItem {
                message_id,
                peer_id: peer.id.clone(),
                kind: "file".to_string(),
                text: String::new(),
                path,
                at: message.at,
            };
            let (queued_message, evicted) = chats.enqueue(message, item);
            for evicted in evicted {
                stage::release(&staged, &evicted.path);
            }
            chat::broadcast(&chats, &queued_message);
            queued += 1;
            continue;
        }

        chats.record(message.clone());
        chat::broadcast(&chats, &message);

        let mut last_emit = Instant::now();
        let result =
            client::send_file(&peer.host, peer.port, &file, Some(&identity), Some(&message_id), |bytes| {
            let now = Instant::now();
            if now.duration_since(last_emit).as_millis() >= 120 || bytes >= total {
                last_emit = now;
                chat::emit_file_progress(&message_id, bytes, total);
            }
        })
        .await;

        match result {
            Ok(bytes) => {
                chat::emit_file_progress(&message_id, bytes, total);
                // A phone staged a copy to reach this call; the peer owns it now.
                stage::release(&staged, &path);
                sent += 1;
            }
            Err(error) => {
                // A connection that dropped mid-upload is a "later", not a
                // verdict: park the file in the same queue the offline path
                // uses and let the tick retry it. The card says queued instead
                // of failed, and the bytes stay where the sender can still
                // read them.
                tracing::warn!(
                    "Send to {}:{} failed, queued for retry: {error}",
                    peer.host,
                    peer.port
                );
                let item = chat::QueuedItem {
                    message_id,
                    peer_id: peer.id.clone(),
                    kind: "file".to_string(),
                    text: String::new(),
                    path,
                    at: message.at,
                };
                let (queued_message, evicted) = chats.enqueue(message, item);
                for evicted in evicted {
                    stage::release(&staged, &evicted.path);
                }
                chat::broadcast(&chats, &queued_message);
                queued += 1;
            }
        }
    }

    info!("Sent {sent} file(s) to {}:{}", peer.host, peer.port);
    Ok(SendOutcome { sent, queued })
}

/// Opens a copy a phone's file can be streamed into while its device is away.
/// The webview holds `File` objects, not paths, so this is the only way a
/// mobile send can survive the composer closing — or the app being killed.
#[tauri::command]
pub async fn chat_stage_open(app: AppHandle, name: String) -> Result<String, String> {
    let path = stage::open(&stage::staged_dir(&app), &name)?;
    Ok(path.to_string_lossy().to_string())
}

/// One base64 chunk of that file. Chunks stay small enough for a single
/// webview message, so a photo costs a handful of calls and no big buffers.
#[tauri::command]
pub async fn chat_stage_append(app: AppHandle, path: String, chunk: String) -> Result<(), String> {
    stage::append(&stage::staged_dir(&app), &path, &chunk)
}

/// Gives up on a copy the sender never finished.
#[tauri::command]
pub async fn chat_stage_cancel(app: AppHandle, path: String) -> Result<(), String> {
    stage::discard(&stage::staged_dir(&app), &path);
    Ok(())
}

/// Files the Explorer verb handed over. The window drains this once it has
/// subscribed, so a cold start and a hand-off look the same from the front end.
#[tauri::command]
pub async fn send_take(app: AppHandle) -> Result<Vec<String>, String> {
    Ok(app.state::<crate::shell::SendQueue>().take())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StagedPath {
    pub name: String,
    pub path: String,
    pub bytes: u64,
}

/// Name + size for the file list the picker shows. Anything that was deleted
/// between the right-click and the sheet rendering simply reports zero bytes so
/// the row still appears with a name; the send will fail loudly there.
#[tauri::command]
pub async fn send_stat(paths: Vec<String>) -> Result<Vec<StagedPath>, String> {
    Ok(paths
        .iter()
        .map(|path| {
            let file = PathBuf::from(path);
            let name = file
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| path.clone());
            let bytes = std::fs::metadata(&file).map(|meta| meta.len()).unwrap_or(0);
            StagedPath { name, path: path.clone(), bytes }
        })
        .collect())
}

/// Turns the system notification on or off without restarting the listener —
/// nothing about it touches the socket.
#[tauri::command]
pub async fn transfer_set_notify(
    app: AppHandle,
    transfer: State<'_, Transfer>,
    notify: bool,
) -> Result<TransferSnapshot, String> {
    let next = Config {
        notify,
        ..transfer.config()
    };
    let path = config::config_path(&app)?;
    next.save(&path)?;
    transfer.set_config(next);
    Ok(snapshot(&app, &transfer).await)
}

#[tauri::command]
pub async fn shell_menu_state() -> Result<crate::shell::MenuState, String> {
    let exe = std::env::current_exe().map_err(|error| format!("找不到程序本体：{error}"))?;
    Ok(crate::shell::state(&exe))
}

/// Adds or removes `FileDrop 传送` in the Explorer file context menu.
#[tauri::command]
pub async fn shell_menu_set(enabled: bool) -> Result<crate::shell::MenuState, String> {
    let exe = std::env::current_exe().map_err(|error| format!("找不到程序本体：{error}"))?;
    if enabled {
        crate::shell::install(&exe)
    } else {
        crate::shell::remove(&exe)
    }
}

fn is_mobile() -> bool {
    cfg!(any(target_os = "android", target_os = "ios"))
}
