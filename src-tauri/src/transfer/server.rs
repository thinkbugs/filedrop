//! FileDrop-compatible LAN transfer hub.
//!
//! The desktop is a peer in the room like any browser tab, but a headless one:
//! it appears in every client's peer list with `rtcSupported: false`, so a phone
//! that picks it sends files over the same WebSocket relay a pair of browsers
//! would use. That is the whole point of the feature — "发到本机" — and it needs
//! no WebRTC stack on this side: the relayed chunk protocol (`header`, binary
//! chunks, `partition`, `partition-received`) is enough to reassemble a file.
//!
//! Everything lives on one `0.0.0.0:<port>` listener: `/` serves the phone page,
//! `/server/webrtc` is the signalling socket, and `/upload` accepts plain
//! multipart for another FileDrop instance or any client that would rather POST.

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        ConnectInfo, DefaultBodyLimit, Multipart, State,
    },
    http::{HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use futures_util::{SinkExt, StreamExt};
use qrcode::QrCode;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Instant;
use tauri::Emitter;
#[cfg(target_os = "android")]
use tauri::Manager;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, RwLock};
use tower_http::services::ServeDir;

use crate::transfer::chat::{
    self, ChatMessage, ChatPeer, Chats, KIND_BROWSER, KIND_DEVICE,
};
use crate::transfer::config::Config;

/// The headless desktop peer. Clients never see this id; they see the name in
/// `peers`, which is why the home device keeps the plain device name while
/// browser tabs get a suffix — otherwise a phone would see two identical names.
const HUB_ID: &str = "filedrop";

pub type ConfigStore = Arc<RwLock<Config>>;

struct Peer {
    name: String,
    rtc_supported: bool,
    tx: mpsc::UnboundedSender<Message>,
}

type PeerMap = Arc<RwLock<HashMap<String, Peer>>>;

#[derive(Clone)]
struct AppState {
    peers: PeerMap,
    config: ConfigStore,
    chats: Chats,
}

static APP: OnceLock<tauri::AppHandle> = OnceLock::new();

/// Phone pages with an open socket, keyed by their chat conversation
/// (`ws:<ip>`). Text sent back to a phone goes straight down this socket.
struct LiveSocket {
    socket_id: String,
    tx: mpsc::UnboundedSender<Message>,
}

type LiveMap = Arc<RwLock<HashMap<String, LiveSocket>>>;

static LIVE: OnceLock<LiveMap> = OnceLock::new();

fn live_sockets() -> &'static LiveMap {
    LIVE.get_or_init(|| Arc::new(RwLock::new(HashMap::new())))
}

/// The UI listens from the first render, before the listener is even started.
pub fn attach_app(app: tauri::AppHandle) {
    let _ = APP.set(app);
}

pub async fn browser_online(chat_key: &str) -> bool {
    live_sockets().read().await.contains_key(chat_key)
}

/// Pushes a text bubble to a phone page. The page treats a frame carrying
/// `sender` as peer traffic, which is what makes the bubble appear in its
/// conversation with this device.
pub async fn send_text_to_browser(chat_key: &str, text: &str) -> Result<(), String> {
    let encoded =
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, text.as_bytes());
    let live = live_sockets().read().await;
    let Some(socket) = live.get(chat_key) else {
        return Err("对方已经离开，等它重新打开传输页".to_string());
    };
    send_json(
        &socket.tx,
        serde_json::json!({ "type": "text", "text": encoded, "sender": HUB_ID }),
    );
    Ok(())
}

/// One received file, from any of the three doors: relay, text, multipart.
fn notify_file(name: String, path: String, bytes: u64) {
    if let Some(app) = APP.get() {
        crate::transfer::notify_file(app, name, path, bytes);
    }
}

pub(crate) fn emit_to_ui(event: &str, payload: serde_json::Value) {
    if let Some(app) = APP.get() {
        let _ = app.emit(event, payload);
    }
}

pub(crate) fn app_handle() -> Option<tauri::AppHandle> {
    APP.get().cloned()
}

fn notify_progress(name: &str, received: u64, total: u64, chat_id: Option<&str>) {
    let progress = if total > 0 {
        (received as f64 / total as f64).min(1.0)
    } else {
        0.0
    };
    emit_to_ui(
        "transfer-progress",
        serde_json::json!({
            "name": name,
            "received": received,
            "total": total,
            "progress": progress
        }),
    );
    if let Some(chat_id) = chat_id {
        chat::emit_file_progress(chat_id, received, total);
    }
}

fn generate_peer_id() -> String {
    uuid::Uuid::new_v4().to_string()[..8].to_string()
}

fn send_json(tx: &mpsc::UnboundedSender<Message>, value: serde_json::Value) {
    if let Ok(text) = serde_json::to_string(&value) {
        let _ = tx.send(Message::Text(text));
    }
}

// ------------------------------------------------------------------ sockets

async fn webrtc_handler(
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, addr, state))
}

async fn handle_socket(socket: WebSocket, addr: SocketAddr, state: AppState) {
    let peer_id = generate_peer_id();
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    let (mut ws_tx, mut ws_rx) = socket.split();

    let pump = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_tx.send(msg).await.is_err() {
                break;
            }
        }
    });

    let device_name = state.config.read().await.device_name.clone();
    let own_name = format!("{} · {}", device_name, &peer_id[..4]);
    let chat_key = format!("ws:{}", addr.ip());

    {
        let mut peers = state.peers.write().await;
        peers.insert(
            peer_id.clone(),
            Peer {
                name: own_name.clone(),
                rtc_supported: true,
                tx: tx.clone(),
            },
        );
    }
    // The last tab from one address wins the reply socket; a reload replaces
    // its own entry when the old socket closes (guarded by the socket id).
    live_sockets().write().await.insert(
        chat_key.clone(),
        LiveSocket {
            socket_id: peer_id.clone(),
            tx: tx.clone(),
        },
    );

    send_json(
        &tx,
        serde_json::json!({
            "type": "display-name",
            "message": { "displayName": own_name, "deviceName": own_name }
        }),
    );

    // The page builds its peer list only from this message; without it a phone
    // sees an empty grid and has nobody to send to.
    {
        let peers = state.peers.read().await;
        let list: Vec<serde_json::Value> = peers
            .iter()
            .filter(|(id, _)| id.as_str() != peer_id)
            .map(|(id, peer)| {
                // The bundled client renders `peer.name.displayName`; a bare
                // string here leaves every row in its peer grid unlabelled.
                let name = if id == HUB_ID {
                    device_name.clone()
                } else {
                    peer.name.clone()
                };
                serde_json::json!({
                    "id": id,
                    "name": { "displayName": name, "deviceName": name },
                    "rtcSupported": peer.rtc_supported
                })
            })
            .collect();
        send_json(&tx, serde_json::json!({ "type": "peers", "peers": list }));
        let joined = serde_json::json!({
            "id": peer_id,
            "name": { "displayName": own_name, "deviceName": own_name },
            "rtcSupported": true
        });
        for (id, peer) in peers.iter() {
            if id.as_str() != peer_id {
                send_json(
                    &peer.tx,
                    serde_json::json!({ "type": "peer-joined", "peer": joined }),
                );
            }
        }
    }

    // Binary chunks carry no recipient, so the last relayed text message's `to`
    // is what they belong to — that is how the page client uses this socket.
    let mut last_to: Option<String> = None;

    while let Some(Ok(msg)) = ws_rx.next().await {
        match msg {
            Message::Text(text) => {
                let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
                    continue;
                };
                match value.get("type").and_then(|v| v.as_str()).unwrap_or_default() {
                    "disconnect" => break,
                    "pong" => {}
                    _ => {
                        let Some(to) = value.get("to").and_then(|v| v.as_str()).map(str::to_string)
                        else {
                            continue;
                        };
                        last_to = Some(to.clone());
                        let mut relay = value.clone();
                        if let Some(obj) = relay.as_object_mut() {
                            obj.remove("to");
                            obj.insert("sender".into(), serde_json::json!(peer_id));
                            // The hub cannot see this socket's address from the
                            // relayed frame, and it needs both the IP (to file
                            // the message under a stable conversation) and the
                            // page's name (to label it).
                            obj.insert("senderIp".into(), serde_json::json!(addr.ip().to_string()));
                            obj.insert("senderName".into(), serde_json::json!(own_name));
                        }
                        let peers = state.peers.read().await;
                        if let Some(recipient) = peers.get(&to) {
                            send_json(&recipient.tx, relay);
                        }
                    }
                }
            }
            Message::Binary(chunk) => {
                let Some(to) = last_to.clone() else { continue };
                let peers = state.peers.read().await;
                if let Some(recipient) = peers.get(&to) {
                    let _ = recipient.tx.send(Message::Binary(chunk));
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    {
        let mut peers = state.peers.write().await;
        peers.remove(&peer_id);
        for peer in peers.values() {
            send_json(
                &peer.tx,
                serde_json::json!({ "type": "peer-left", "peerId": peer_id }),
            );
        }
    }
    {
        let mut live = live_sockets().write().await;
        let mine = live
            .get(&chat_key)
            .map(|socket| socket.socket_id == peer_id)
            .unwrap_or(false);
        if mine {
            live.remove(&chat_key);
        }
    }

    pump.abort();
}

// ------------------------------------------------------------- hub (this PC)

struct Incoming {
    name: String,
    path: PathBuf,
    file: tokio::fs::File,
    expected: u64,
    received: u64,
    reported: f64,
    /// Chat bubble this transfer belongs to; the start event carried this id,
    /// so progress and the final message replace the same placeholder.
    chat_id: String,
    chat_key: String,
    chat_name: String,
}

async fn reply(peers: &PeerMap, to: &Option<String>, value: serde_json::Value) {
    let Some(id) = to else { return };
    let map = peers.read().await;
    if let Some(peer) = map.get(id) {
        // Clients route relayed traffic by `sender`; a reply without it is
        // dropped as an unknown message type and the transfer never advances.
        let mut value = value;
        if let Some(obj) = value.as_object_mut() {
            obj.entry("sender")
                .or_insert_with(|| serde_json::json!(HUB_ID));
        }
        send_json(&peer.tx, value);
    }
}

async fn hub_loop(
    mut rx: mpsc::UnboundedReceiver<Message>,
    config: ConfigStore,
    peers: PeerMap,
    chats: Chats,
) {
    // One slot per sender: two phones can send at the same time without their
    // chunks landing in each other's file.
    let mut incoming: HashMap<String, Incoming> = HashMap::new();
    // The relay stamps `sender` on every text message; binary chunks reuse it.
    let mut sender: Option<String> = None;
    let mut sender_ip: Option<String> = None;
    let mut sender_name: Option<String> = None;

    while let Some(msg) = rx.recv().await {
        match msg {
            Message::Text(text) => {
                let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
                    continue;
                };
                if let Some(from) = value.get("sender").and_then(|v| v.as_str()) {
                    sender = Some(from.to_string());
                }
                if let Some(ip) = value.get("senderIp").and_then(|v| v.as_str()) {
                    sender_ip = Some(ip.to_string());
                }
                if let Some(name) = value.get("senderName").and_then(|v| v.as_str()) {
                    sender_name = Some(name.to_string());
                }
                // A browser tab has a throwaway socket id; its address is what
                // keeps the conversation together across reloads.
                let chat_key = match sender_ip.as_deref() {
                    Some(ip) if !ip.is_empty() => format!("ws:{ip}"),
                    _ => format!("ws:{}", sender.clone().unwrap_or_default()),
                };
                let chat_name = sender_name
                    .clone()
                    .filter(|name| !name.is_empty())
                    .or_else(|| sender.clone())
                    .unwrap_or_else(|| "手机".to_string());
                match value.get("type").and_then(|v| v.as_str()).unwrap_or_default() {
                    "header" => {
                        let key = sender.clone().unwrap_or_else(|| "anonymous".to_string());
                        if let Some(old) = incoming.remove(&key) {
                            chat::emit_file_cancelled(&old.chat_id);
                            drop(old.file);
                            let _ = tokio::fs::remove_file(&old.path).await;
                        }
                        let name = value
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("file");
                        let expected = value.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
                        let dir = PathBuf::from(config.read().await.storage_dir.clone());
                        let Some(path) = landing_path(&dir, &sanitize_file_name(name)).await else {
                            eprintln!("[filedrop] 没有可写入的存储目录，这个文件被丢弃");
                            continue;
                        };
                        match tokio::fs::File::create(&path).await {
                            Ok(file) => {
                                let display = path
                                    .file_name()
                                    .map(|n| n.to_string_lossy().into_owned())
                                    .unwrap_or_default();
                                let peer = chats.ensure_peer(
                                    &chat_key,
                                    &chat_name,
                                    sender_ip.as_deref().unwrap_or_default(),
                                    0,
                                    KIND_BROWSER,
                                );
                                let chat_id = chat::new_id();
                                chat::emit_file_start(
                                    &chat_id,
                                    &peer.id,
                                    &peer.name,
                                    &display,
                                    expected,
                                    "in",
                                );
                                incoming.insert(
                                    key.clone(),
                                    Incoming {
                                        name: display.clone(),
                                        path,
                                        file,
                                        expected,
                                        received: 0,
                                        reported: 0.0,
                                        chat_id: chat_id.clone(),
                                        chat_key: peer.id,
                                        chat_name: peer.name,
                                    },
                                );
                                notify_progress(&display, 0, expected, Some(&chat_id));
                                // An empty file has no chunk to check, so the
                                // byte-count finish never fires for it.
                                if expected == 0 {
                                    finish_file(&mut incoming, &key, &peers, &sender, &chats).await;
                                }
                            }
                            Err(error) => eprintln!("[filedrop] 无法创建文件：{error}"),
                        }
                    }
                    "partition" => {
                        // The sender waits for this before shipping the next
                        // partition; skipping it stalls the transfer.
                        reply(
                            &peers,
                            &sender,
                            serde_json::json!({
                                "type": "partition-received",
                                "offset": value.get("offset").cloned().unwrap_or_default()
                            }),
                        )
                        .await;
                        if let Some(key) = sender.clone() {
                            let complete = incoming
                                .get(&key)
                                .map(|state| state.expected > 0 && state.received >= state.expected)
                                .unwrap_or(false);
                            if complete {
                                finish_file(&mut incoming, &key, &peers, &sender, &chats).await;
                            }
                        }
                    }
                    "text" => {
                        let body = value.get("text").and_then(|v| v.as_str()).unwrap_or_default();
                        // The page sends base64; anything else is read as-is
                        // so a hand-rolled client is not forced through it.
                        let decoded = base64::Engine::decode(
                            &base64::engine::general_purpose::STANDARD,
                            body,
                        )
                        .ok()
                        .and_then(|bytes| String::from_utf8(bytes).ok())
                        .unwrap_or_else(|| body.to_string());
                        let text = decoded.trim_end().to_string();
                        if !text.trim().is_empty() {
                            let peer = chats.ensure_peer(
                                &chat_key,
                                &chat_name,
                                sender_ip.as_deref().unwrap_or_default(),
                                0,
                                KIND_BROWSER,
                            );
                            let message = chats.record(ChatMessage::text(&peer, "in", text));
                            chat::broadcast(&chats, &message);
                        }
                        reply(
                            &peers,
                            &sender,
                            serde_json::json!({ "type": "transfer-complete" }),
                        )
                        .await;
                    }
                    _ => {}
                }
            }
            Message::Binary(chunk) => {
                let Some(key) = sender.clone() else { continue };
                let Some(state) = incoming.get_mut(&key) else {
                    continue;
                };
                if state.file.write_all(&chunk).await.is_err() {
                    chat::emit_file_cancelled(&state.chat_id);
                    incoming.remove(&key);
                    continue;
                }
                state.received += chunk.len() as u64;
                let progress = if state.expected == 0 {
                    1.0
                } else {
                    (state.received as f64 / state.expected as f64).min(1.0)
                };
                if progress - state.reported >= 0.05 || progress >= 1.0 {
                    state.reported = progress;
                    reply(
                        &peers,
                        &sender,
                        serde_json::json!({ "type": "progress", "progress": progress }),
                    )
                    .await;
                    notify_progress(
                        &state.name,
                        state.received,
                        state.expected,
                        Some(&state.chat_id),
                    );
                }
                // The page client does not send a closing `partition` when the
                // file ends mid-partition, so the partition arm alone would leave
                // the last file open (and the sender waiting forever).
                let complete = incoming
                    .get(&key)
                    .map(|state| state.expected > 0 && state.received >= state.expected)
                    .unwrap_or(false);
                if complete {
                    finish_file(&mut incoming, &key, &peers, &sender, &chats).await;
                }
            }
            _ => {}
        }
    }
}

async fn finish_file(
    incoming: &mut HashMap<String, Incoming>,
    key: &str,
    peers: &PeerMap,
    sender: &Option<String>,
    chats: &Chats,
) {
    let Some(mut state) = incoming.remove(key) else {
        return;
    };
    if let Err(error) = state.file.flush().await {
        eprintln!("[filedrop] 写入失败：{error}");
        return;
    }
    drop(state.file);
    notify_progress(&state.name, state.received, state.expected, None);
    notify_file(
        state.name.clone(),
        state.path.to_string_lossy().to_string(),
        state.received,
    );

    let peer = chats.ensure_peer(&state.chat_key, &state.chat_name, "", 0, KIND_BROWSER);
    let mut message = ChatMessage::file(
        &peer,
        "in",
        state.name.clone(),
        state.path.to_string_lossy().to_string(),
        state.received,
    );
    message.id = state.chat_id.clone();
    let message = chats.record(message);
    chat::broadcast(chats, &message);

    reply(
        peers,
        sender,
        serde_json::json!({ "type": "transfer-complete" }),
    )
    .await;
}

fn sanitize_file_name(name: &str) -> String {
    let base = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("file")
        .trim()
        .to_string();
    let cleaned: String = base
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        "file".to_string()
    } else {
        cleaned
    }
}

fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let candidate = dir.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let path = Path::new(name);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| name.to_string());
    let ext = path.extension().map(|e| e.to_string_lossy().into_owned());
    for index in 1..1000 {
        let file = match &ext {
            Some(ext) => format!("{stem} ({index}).{ext}"),
            None => format!("{stem} ({index})"),
        };
        let next = dir.join(file);
        if !next.exists() {
            return next;
        }
    }
    dir.join(format!("{stem}-{}", uuid::Uuid::new_v4()))
}

/// Where the next inbound file called `name` lands.
///
/// A folder in Android's shared storage is not this process's to create: the
/// file only exists once MediaStore hands out a row, and the path that comes
/// back is not necessarily the one asked for, because a name already taken is
/// renamed there. So the phone asks first and writes to the answer. When the
/// platform cannot answer at all — a row without a path, a folder on another
/// volume, no UI thread to ask — the bytes still have to go somewhere, and the
/// private directory is the one that never refuses: harder to find, but the file
/// arrives.
async fn landing_path(dir: &Path, name: &str) -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    {
        if !under_private_storage(dir) {
            if let Some(path) = claim_shared(dir, name).await {
                return Some(path);
            }
            return private_landing(name);
        }
    }

    if let Err(error) = tokio::fs::create_dir_all(dir).await {
        eprintln!("[filedrop] 无法创建存储目录：{error}");
        return None;
    }
    Some(unique_path(dir, name))
}

#[cfg(target_os = "android")]
async fn claim_shared(dir: &Path, name: &str) -> Option<PathBuf> {
    let app = app_handle()?;
    let dir_text = dir.to_string_lossy().to_string();
    let name = name.to_string();
    let worker = app.clone();
    // The claim waits on the UI thread, so it goes to a worker: an accept loop
    // that stops for one file stops for every peer behind it.
    let claimed = tokio::task::spawn_blocking(move || {
        crate::android::claim_landing_path(&worker, &dir_text, &name)
    })
    .await
    .ok()?
    .ok()?;
    let Some(path) = claimed else {
        // The file still arrives (in the app's own directory), but the user
        // chose a folder and will look for it there, so say why it moved.
        if let Ok(reason) = crate::android::claim_error(&app) {
            let reason = reason.trim().to_string();
            if !reason.is_empty() {
                if let Some(transfer) = app.try_state::<crate::transfer::Transfer>() {
                    transfer.set_error(Some(format!(
                        "无法保存到所选文件夹，文件已存入应用私有目录（{reason}）"
                    )));
                }
            }
        }
        return None;
    };
    Some(PathBuf::from(path))
}

#[cfg(target_os = "android")]
fn private_storage_dir() -> Option<PathBuf> {
    let app = app_handle()?;
    Some(PathBuf::from(crate::transfer::config::private_storage(&app)))
}

#[cfg(target_os = "android")]
fn under_private_storage(dir: &Path) -> bool {
    private_storage_dir().is_some_and(|private| dir == private || dir.starts_with(&private))
}

#[cfg(target_os = "android")]
fn private_landing(name: &str) -> Option<PathBuf> {
    let private = private_storage_dir()?;
    std::fs::create_dir_all(&private).ok()?;
    Some(unique_path(&private, name))
}

// ------------------------------------------------------------------ upload

/// A guard, not a policy. The transfer page has no reason to refuse a two-hour
/// recording, so the multipart body limit is disabled on these routes and this
/// number is what stops a client that keeps sending from filling the disk.
const UPLOAD_BYTE_LIMIT: u64 = 16 * 1024 * 1024 * 1024;

/// Streams one multipart field to disk, refusing to exceed `limit`.
async fn field_to_file(
    field: &mut axum::extract::multipart::Field<'_>,
    path: &Path,
    display: &str,
    limit: u64,
    chat_id: Option<&str>,
) -> Result<u64, (String, u64)> {
    let mut file = tokio::fs::File::create(path)
        .await
        .map_err(|error| (format!("无法写入文件：{error}"), 0u64))?;
    let mut written = 0u64;
    let mut last_emit = Instant::now();
    let mut last_bytes = 0u64;
    while let Some(chunk) = field
        .chunk()
        .await
        .map_err(|error| (format!("{error}"), written))?
    {
        written += chunk.len() as u64;
        if written > limit {
            let _ = file.shutdown().await;
            return Err((format!("文件超过 {limit} 字节上限"), written));
        }
        file.write_all(&chunk)
            .await
            .map_err(|error| (format!("写入失败：{error}"), written))?;
        // 512 KiB (and at least 200 ms) between UI updates is enough for a
        // progress bar without flooding the event loop on a fast LAN.
        if written - last_bytes >= 512 * 1024
            || last_emit.elapsed().as_millis() >= 200
            || written == limit
        {
            last_bytes = written;
            last_emit = Instant::now();
            notify_progress(display, written, 0, chat_id);
        }
    }
    file.flush()
        .await
        .map_err(|error| (format!("写入未落盘：{error}"), written))?;
    Ok(written)
}

fn resolve_upload_peer(
    chats: &Chats,
    from_id: &str,
    from_name: &str,
    from_port: u16,
    addr: SocketAddr,
) -> ChatPeer {
    let ip = addr.ip().to_string();
    let id = if from_id.trim().is_empty() {
        format!("ip:{ip}")
    } else {
        from_id.trim().to_string()
    };
    let name = if from_name.trim().is_empty() {
        format!("局域网设备 {ip}")
    } else {
        from_name.trim().to_string()
    };
    chats.ensure_peer(&id, &name, &ip, from_port, KIND_DEVICE)
}

/// `POST /upload` — the plain-multipart path for another FileDrop instance or a
/// client that will not use the relay. Each chunk lands on disk as it arrives;
/// the old version buffered the whole body, which turned a 500 MB video into a
/// 500 MB allocation.
///
/// The reply carries CORS headers because the client that asks "did it arrive?"
/// may be a page on another origin: without them the bytes land and the sender
/// still sees a failed fetch.
async fn upload_file(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    mut multipart: Multipart,
) -> Response {
    let storage_dir = PathBuf::from(state.config.read().await.storage_dir.clone());

    let mut saved_files = Vec::new();
    // A FileDrop sender puts these before the file field; a plain client sends
    // none and the source address is enough to file the transfer.
    let mut from_id = String::new();
    let mut from_name = String::new();
    let mut from_port = 0u16;
    let mut message_id = String::new();

    while let Ok(Some(mut field)) = multipart.next_field().await {
        match field.name() {
            Some("messageId") => {
                message_id = field.text().await.unwrap_or_default().trim().to_string();
                continue;
            }
            Some("fromId") => {
                from_id = field.text().await.unwrap_or_default().trim().to_string();
                continue;
            }
            Some("fromName") => {
                from_name = field.text().await.unwrap_or_default().trim().to_string();
                continue;
            }
            Some("fromPort") => {
                from_port = field
                    .text()
                    .await
                    .ok()
                    .and_then(|value| value.trim().parse::<u16>().ok())
                    .unwrap_or(0);
                continue;
            }
            _ => {}
        }

        let file_name = field.file_name().unwrap_or("unknown").to_string();
        let wanted = sanitize_file_name(&file_name);
        // A resend of a file that is already on file: read the bytes away and
        // answer success. Writing them again is how a lost acknowledgement used
        // to become a second copy on disk and a second bubble in the chat.
        if !message_id.is_empty() && state.chats.contains(&message_id) {
            while let Ok(Some(_)) = field.chunk().await {}
            continue;
        }
        let Some(file_path) = landing_path(&storage_dir, &wanted).await else {
            return with_cors(
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "没有可写入的存储目录".to_string(),
                )
                    .into_response(),
            );
        };
        // A name already taken gets renamed by the platform, and the card has to
        // say what actually landed rather than what was asked for.
        let display = file_path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or(wanted);
        let peer = resolve_upload_peer(&state.chats, &from_id, &from_name, from_port, addr);
        // The sender's id, when it gave one, is used for the progress events and
        // the stored message alike: that keeps the card and the progress on the
        // same key, and makes a resend recognisable.
        let chat_id = if message_id.is_empty() {
            chat::new_id()
        } else {
            message_id.clone()
        };
        chat::emit_file_start(&chat_id, &peer.id, &peer.name, &display, 0, "in");
        let received =
            match field_to_file(&mut field, &file_path, &display, UPLOAD_BYTE_LIMIT, Some(&chat_id))
                .await
            {
                Ok(received) => received,
                Err((error, _)) => {
                    chat::emit_file_cancelled(&chat_id);
                    let _ = tokio::fs::remove_file(&file_path).await;
                    return with_cors((StatusCode::BAD_REQUEST, error).into_response());
                }
            };
        let mut message = ChatMessage::file(
            &peer,
            "in",
            display.clone(),
            file_path.to_string_lossy().to_string(),
            received,
        );
        message.id = chat_id;
        let message = state.chats.record(message);
        chat::broadcast(&state.chats, &message);

        saved_files.push(display.clone());
        notify_file(display, file_path.to_string_lossy().to_string(), received);
    }

    with_cors(
        (StatusCode::OK, format!("成功保存 {} 个文件", saved_files.len())).into_response(),
    )
}

fn with_cors(mut body: Response) -> Response {
    let headers = body.headers_mut();
    headers.insert(
        axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    headers.insert(
        axum::http::header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("content-type"),
    );
    headers.insert(
        axum::http::header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET,POST,HEAD,OPTIONS"),
    );
    body
}

async fn preflight() -> Response {
    with_cors((StatusCode::NO_CONTENT, "").into_response())
}

async fn health(State(state): State<AppState>) -> Response {
    let config = state.config.read().await;
    with_cors(
        Json(serde_json::json!({
            "app": "FileDrop",
            "name": config.device_name,
            "id": config.instance_id,
            "port": config.port
        }))
        .into_response(),
    )
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct TextIn {
    /// The sender's message id; a resend after a lost acknowledgement carries
    /// the same one and must not become a second bubble.
    #[serde(default)]
    id: String,
    #[serde(default)]
    from_id: String,
    #[serde(default)]
    from_name: String,
    #[serde(default)]
    from_port: u16,
    #[serde(default)]
    text: String,
}

/// `POST /message` — chat text from another FileDrop. The sender's address is
/// the reply route when it did not identify itself; a message with an id and
/// port lets this side answer with its own text later.
async fn receive_text(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(body): Json<TextIn>,
) -> Response {
    let text = body.text.trim_end().to_string();
    if text.trim().is_empty() {
        return with_cors((StatusCode::BAD_REQUEST, "消息不能为空").into_response());
    }
    if text.chars().count() > 20_000 {
        return with_cors((StatusCode::BAD_REQUEST, "消息太长了").into_response());
    }
    let peer = resolve_upload_peer(&state.chats, &body.from_id, &body.from_name, body.from_port, addr);
    let mut message = ChatMessage::text(&peer, "in", text);
    if !body.id.trim().is_empty() {
        // Keep the sender's id: `record` drops a message whose id is already on
        // file, which is what makes a resend harmless.
        message.id = body.id.trim().to_string();
    }
    let message = state.chats.record(message);
    chat::broadcast(&state.chats, &message);
    with_cors(Json(serde_json::json!({ "ok": true })).into_response())
}

/// The join code for the transfer page. SVG rather than PNG: it is one string
/// for the webview to show and needs no image encoder on this side.
pub fn generate_qr_code(url: &str) -> Result<String, String> {
    let code = QrCode::new(url.as_bytes()).map_err(|e| e.to_string())?;
    let svg = code
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(220, 220)
        .quiet_zone(true)
        .dark_color(qrcode::render::svg::Color("#0b1220"))
        .light_color(qrcode::render::svg::Color("#ffffff"))
        .build();
    Ok(format!(
        "data:image/svg+xml;base64,{}",
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, svg.as_bytes())
    ))
}

pub struct Started {
    pub url: String,
    pub port: u16,
    shutdown: tokio::sync::oneshot::Sender<()>,
}

impl Started {
    /// Stops the listener. The socket closes on the next accept, which frees the
    /// port for a restart without waiting for anything.
    pub fn stop(self) {
        let _ = self.shutdown.send(());
    }
}

/// Binding right after a stop can lose the race with the previous listener's
/// graceful shutdown, so an `AddrInUse` gets a few short retries instead of a
/// settings save that fails every other time.
async fn bind_with_retry(port: u16) -> Result<TcpListener, String> {
    let address = SocketAddr::from(([0, 0, 0, 0], port));
    let mut last_error = None;
    for attempt in 0..12 {
        match TcpListener::bind(address).await {
            Ok(listener) => return Ok(listener),
            Err(error) => {
                last_error = Some(error);
                if attempt < 11 {
                    tokio::time::sleep(std::time::Duration::from_millis(80)).await;
                }
            }
        }
    }
    Err(format!(
        "无法监听端口 {port}：{}",
        last_error
            .map(|error| error.to_string())
            .unwrap_or_else(|| "端口被占用".to_string())
    ))
}

/// The whole listener's route table, kept apart from binding so a test can put it
/// on a real socket without a Tauri app handle.
fn hub_router(state: AppState, assets_dir: PathBuf) -> Router {
    // The body limit is disabled on the POST route: axum's 2 MiB default is a
    // memory guard for a buffered body, and this handler never buffers one — it
    // streams to disk under `UPLOAD_BYTE_LIMIT`.
    Router::new()
        .route("/server/webrtc", get(webrtc_handler))
        .route(
            "/upload",
            post(upload_file)
                .options(preflight)
                .layer(DefaultBodyLimit::disable()),
        )
        .route(
            "/message",
            post(receive_text)
                .options(preflight)
                .layer(DefaultBodyLimit::max(128 * 1024)),
        )
        .route("/health", get(health))
        .fallback_service(ServeDir::new(assets_dir))
        .with_state(state)
}

/// Binds and starts serving. The returned task runs on whatever runtime this is
/// awaited from, so the caller must await it from a long-lived one — a private
/// runtime dropped right after `block_on` takes the listener with it.
pub async fn start_snapdrop_server(
    port: u16,
    assets_dir: PathBuf,
    app: tauri::AppHandle,
    config: Config,
    chats: Chats,
) -> Result<Started, String> {
    let _ = APP.set(app);

    let config_store: ConfigStore = Arc::new(RwLock::new(config.clone()));
    let peers: PeerMap = Arc::new(RwLock::new(HashMap::new()));
    live_sockets().write().await.clear();
    let (hub_tx, hub_rx) = mpsc::unbounded_channel::<Message>();
    {
        let mut map = peers.write().await;
        map.insert(
            HUB_ID.to_string(),
            Peer {
                name: config.device_name.clone(),
                rtc_supported: false,
                tx: hub_tx,
            },
        );
    }
    tokio::spawn(hub_loop(
        hub_rx,
        config_store.clone(),
        peers.clone(),
        chats.clone(),
    ));

    let listener = bind_with_retry(port).await?;
    let bound = listener
        .local_addr()
        .map(|addr| addr.port())
        .unwrap_or(port);
    let url = format!("http://{}:{}", crate::net::lan_ip(), bound);

    let state = AppState {
        peers,
        config: config_store,
        chats,
    };
    let app_router = hub_router(state, assets_dir);

    let (shutdown, stopped) = tokio::sync::oneshot::channel();
    let stop_signal = async move {
        let _ = stopped.await;
    };

    tracing::info!("FileDrop listening on {url}");

    tokio::spawn(async move {
        if let Err(error) = axum::serve(
            listener,
            app_router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(stop_signal)
        .await
        {
            tracing::warn!("FileDrop stopped: {error}");
        }
    });

    Ok(Started {
        url,
        port: bound,
        shutdown,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;
    use tokio_tungstenite::{connect_async, tungstenite, MaybeTlsStream, WebSocketStream};

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "filedrop-{tag}-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn test_state(storage: &Path, chats: Chats) -> AppState {
        AppState {
            peers: Arc::new(RwLock::new(HashMap::new())),
            config: Arc::new(RwLock::new(Config {
                device_name: "Test Drop".to_string(),
                storage_dir: storage.to_string_lossy().to_string(),
                port: 0,
                auto_start: false,
                notify: false,
                instance_id: "test-instance".to_string(),
            })),
            chats,
        }
    }

    async fn serve(state: AppState, assets: PathBuf) -> u16 {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let router = hub_router(state, assets);
        tokio::spawn(async move {
            let _ = axum::serve(
                listener,
                router.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await;
        });
        port
    }

    #[tokio::test]
    async fn multipart_upload_lands_in_storage() {
        let dir = temp_dir("upload");
        let storage = dir.join("received");
        let assets = dir.join("web");
        std::fs::create_dir_all(&assets).unwrap();
        std::fs::write(assets.join("index.html"), "FileDrop").unwrap();

        let chats = Chats::default();
        let port = serve(test_state(&storage, chats.clone()), assets).await;
        let source = dir.join("hello.txt");
        std::fs::write(&source, b"hello").unwrap();

        let identity = crate::transfer::client::SendIdentity {
            id: "sender-id",
            name: "Sender",
            port: 9999,
        };
        let sent = crate::transfer::client::send_file(
            "127.0.0.1",
            port,
            &source,
            Some(&identity),
            None,
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(sent, 5);
        assert_eq!(
            std::fs::read(storage.join("hello.txt")).unwrap(),
            b"hello"
        );
        let messages = chats.messages("sender-id");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].file_name, "hello.txt");
        assert_eq!(messages[0].direction, "in");
        assert_eq!(chats.peer("sender-id").unwrap().name, "Sender");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn text_message_lands_in_a_conversation() {
        let dir = temp_dir("text");
        let storage = dir.join("received");
        let assets = dir.join("web");
        std::fs::create_dir_all(&assets).unwrap();
        std::fs::write(assets.join("index.html"), "FileDrop").unwrap();

        let chats = Chats::default();
        let port = serve(test_state(&storage, chats.clone()), assets).await;

        let body = r#"{"fromId":"peer-a","fromName":"Desk","fromPort":9000,"text":"你好"}"#;
        let request = format!(
            "POST /message HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();
        assert!(String::from_utf8_lossy(&response).starts_with("HTTP/1.1 200"));

        let messages = chats.messages("peer-a");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].text, "你好");
        assert_eq!(messages[0].direction, "in");
        let peer = chats.peer("peer-a").unwrap();
        assert_eq!(peer.name, "Desk");
        assert_eq!(peer.host, "127.0.0.1");
        assert_eq!(peer.port, 9000);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn relayed_phone_file_is_reassembled() {
        let dir = temp_dir("relay");
        let storage = dir.join("received");
        let assets = dir.join("web");
        std::fs::create_dir_all(&assets).unwrap();
        std::fs::write(assets.join("index.html"), "FileDrop").unwrap();

        let chats = Chats::default();
        let state = test_state(&storage, chats.clone());
        let (hub_tx, hub_rx) = mpsc::unbounded_channel::<Message>();
        {
            let mut peers = state.peers.write().await;
            peers.insert(
                HUB_ID.to_string(),
                Peer {
                    name: "Test Drop".to_string(),
                    rtc_supported: false,
                    tx: hub_tx,
                },
            );
        }
        tokio::spawn(hub_loop(
            hub_rx,
            state.config.clone(),
            state.peers.clone(),
            chats.clone(),
        ));
        let port = serve(state, assets).await;

        let (mut socket, _) = connect_async(format!("ws://127.0.0.1:{port}/server/webrtc"))
            .await
            .unwrap();

        // The first frames describe the room; the hub must be in that list or a
        // phone would have nobody to send to.
        let mut saw_hub = false;
        for _ in 0..2 {
            let message = next_json(&mut socket).await;
            if message.get("type").and_then(|v| v.as_str()) == Some("peers") {
                saw_hub = message
                    .get("peers")
                    .and_then(|v| v.as_array())
                    .map(|peers| {
                        peers.iter().any(|peer| {
                            peer.get("id").and_then(|v| v.as_str()) == Some(HUB_ID)
                                && peer.get("rtcSupported") == Some(&serde_json::json!(false))
                        })
                    })
                    .unwrap_or(false);
            }
        }
        assert!(saw_hub, "peer list did not include the FileDrop hub");

        socket
            .send(tungstenite::Message::Text(
                serde_json::json!({
                    "type": "header",
                    "name": "photo.jpg",
                    "size": 5,
                    "to": HUB_ID
                })
                .to_string(),
            ))
            .await
            .unwrap();
        socket
            .send(tungstenite::Message::Binary(b"hello".to_vec()))
            .await
            .unwrap();

        // After the last byte the hub flushes and acknowledges; when the ack is
        // here, the file is on disk.
        let mut completed = false;
        for _ in 0..10 {
            let message = next_json(&mut socket).await;
            if message.get("type").and_then(|v| v.as_str()) == Some("transfer-complete") {
                completed = true;
                break;
            }
        }
        assert!(completed, "no transfer-complete from the hub");
        assert_eq!(
            std::fs::read(storage.join("photo.jpg")).unwrap(),
            b"hello"
        );
        let messages = chats.messages("ws:127.0.0.1");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].file_name, "photo.jpg");
        assert_eq!(messages[0].bytes, 5);
        assert_eq!(messages[0].direction, "in");
        let _ = std::fs::remove_dir_all(&dir);
    }

    async fn next_json(socket: &mut WebSocketStream<MaybeTlsStream<TcpStream>>) -> serde_json::Value {
        loop {
            let message = tokio::time::timeout(Duration::from_secs(3), socket.next())
                .await
                .expect("timed out waiting for a relay frame")
                .expect("socket closed")
                .expect("socket error");
            if let tungstenite::Message::Text(text) = message {
                return serde_json::from_str(&text).unwrap();
            }
        }
    }

    #[test]
    fn file_names_cannot_escape_the_storage_dir() {
        assert_eq!(sanitize_file_name("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_file_name("..\\..\\Windows\\win.ini"), "win.ini");
        assert_eq!(sanitize_file_name(".."), "file");
        assert_eq!(sanitize_file_name("a:b?.txt"), "a_b_.txt");
    }
}
