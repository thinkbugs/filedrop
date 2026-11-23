//! Conversation history behind the device list and the chat pane.
//!
//! A peer is either another FileDrop instance (stable instance id from the
//! config) or a browser tab on the phone page (`ws:<ip>`): a phone tab gets a
//! fresh socket id every time, so its IP is the only thing that survives a
//! reload. Messages are small JSON and land in `chats.json` next to the config,
//! which is what makes the device list survive a restart.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

const MAX_MESSAGES_PER_PEER: usize = 500;
const MAX_PEERS: usize = 80;
const PREVIEW_CHARS: usize = 80;
/// A device that never comes back should not grow `chats.json` forever.
pub const MAX_QUEUED: usize = 300;
pub const QUEUE_TTL: u64 = 7 * 24 * 60 * 60 * 1000;

pub const KIND_DEVICE: &str = "device";
pub const KIND_BROWSER: &str = "browser";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub id: String,
    pub peer_id: String,
    pub peer_name: String,
    /// `in` (the other side wrote) or `out` (this device wrote).
    pub direction: String,
    /// `text` or `file`.
    pub kind: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub file_name: String,
    #[serde(default)]
    pub file_path: String,
    #[serde(default)]
    pub bytes: u64,
    /// Milliseconds since the Unix epoch.
    pub at: u64,
    #[serde(default)]
    pub failed: bool,
    /// `out` that has not reached the peer: it is waiting in the outbox for the
    /// device to come back, so the sender never has to sit and watch.
    #[serde(default)]
    pub queued: bool,
}

impl ChatMessage {
    pub fn text(peer: &ChatPeer, direction: &str, text: String) -> Self {
        Self {
            id: new_id(),
            peer_id: peer.id.clone(),
            peer_name: peer.name.clone(),
            direction: direction.to_string(),
            kind: "text".to_string(),
            text,
            file_name: String::new(),
            file_path: String::new(),
            bytes: 0,
            at: now_ms(),
            failed: false,
            queued: false,
        }
    }

    pub fn file(
        peer: &ChatPeer,
        direction: &str,
        file_name: String,
        file_path: String,
        bytes: u64,
    ) -> Self {
        Self {
            id: new_id(),
            peer_id: peer.id.clone(),
            peer_name: peer.name.clone(),
            direction: direction.to_string(),
            kind: "file".to_string(),
            text: String::new(),
            file_name,
            file_path,
            bytes,
            at: now_ms(),
            failed: false,
            queued: false,
        }
    }

    fn preview(&self) -> String {
        if self.kind == "file" {
            return format!("[文件] {}", self.file_name);
        }
        let flat = self.text.split_whitespace().collect::<Vec<_>>().join(" ");
        let mut out: String = flat.chars().take(PREVIEW_CHARS).collect();
        if flat.chars().count() > PREVIEW_CHARS {
            out.push('…');
        }
        out
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatPeer {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub port: u16,
    /// `device` (another FileDrop app) or `browser` (the phone page).
    #[serde(default = "default_kind")]
    pub kind: String,
    #[serde(default)]
    pub last_at: u64,
    #[serde(default)]
    pub preview: String,
    #[serde(default)]
    pub last_kind: String,
    #[serde(default)]
    pub unread: u32,
}

fn default_kind() -> String {
    KIND_DEVICE.to_string()
}

/// One send that could not be delivered and is waiting for the peer to reappear.
/// The transcript already carries the bubble; this is the job list behind it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueuedItem {
    pub message_id: String,
    pub peer_id: String,
    /// `text` or `file`.
    pub kind: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub path: String,
    pub at: u64,
}

impl ChatPeer {
    fn new(id: &str, name: &str, host: &str, port: u16, kind: &str) -> Self {
        Self {
            id: id.to_string(),
            name: if name.trim().is_empty() {
                if host.is_empty() {
                    "FileDrop".to_string()
                } else {
                    host.to_string()
                }
            } else {
                name.trim().to_string()
            },
            host: host.to_string(),
            port,
            kind: kind.to_string(),
            last_at: 0,
            preview: String::new(),
            last_kind: String::new(),
            unread: 0,
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Store {
    #[serde(default)]
    peers: HashMap<String, ChatPeer>,
    #[serde(default)]
    messages: Vec<ChatMessage>,
    /// Oldest first, across every peer — a peer that stays offline must not
    /// hold the others' sends behind it.
    #[serde(default)]
    outbox: Vec<QueuedItem>,
}

#[derive(Clone, Default)]
pub struct Chats {
    inner: Arc<Mutex<Store>>,
    path: Arc<Option<PathBuf>>,
}

impl Chats {
    pub fn load(path: PathBuf) -> Self {
        let store = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<Store>(&text).ok())
            .unwrap_or_default();
        Self {
            inner: Arc::new(Mutex::new(store)),
            path: Arc::new(Some(path)),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Store> {
        self.inner.lock().unwrap_or_else(|error| error.into_inner())
    }

    fn persist(&self, store: &Store) {
        let Some(path) = self.path.as_ref() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let Ok(text) = serde_json::to_string_pretty(store) else {
            return;
        };
        let temp = path.with_extension("json.tmp");
        if std::fs::write(&temp, text).is_ok() {
            let _ = std::fs::rename(&temp, path);
        }
    }

    pub fn list(&self) -> Vec<ChatPeer> {
        let store = self.lock();
        let mut list: Vec<ChatPeer> = store.peers.values().cloned().collect();
        list.sort_by(|a, b| b.last_at.cmp(&a.last_at).then_with(|| a.name.cmp(&b.name)));
        list
    }

    pub fn peer(&self, id: &str) -> Option<ChatPeer> {
        self.lock().peers.get(id).cloned()
    }

    pub fn messages(&self, peer_id: &str) -> Vec<ChatMessage> {
        let store = self.lock();
        store
            .messages
            .iter()
            .filter(|message| message.peer_id == peer_id)
            .cloned()
            .collect()
    }

    /// Creates the conversation if needed and refreshes its endpoint. A
    /// placeholder added by hand (`manual:host:port`) is folded into the real
    /// instance id as soon as the same host and port identify themselves.
    /// Inbound that carries no id is attributed to the device already known on
    /// that host, so a plain web page cannot rename or downgrade a device
    /// conversation it happens to share an address with.
    pub fn ensure_peer(
        &self,
        id: &str,
        name: &str,
        host: &str,
        port: u16,
        kind: &str,
    ) -> ChatPeer {
        let mut store = self.lock();
        let incoming_id = id.trim();
        let identified = !incoming_id.is_empty();
        let mut id = if identified {
            incoming_id.to_string()
        } else {
            format!("ip:{host}")
        };

        if !identified {
            let known = store
                .peers
                .iter()
                .find(|(key, peer)| {
                    peer.kind == KIND_DEVICE && peer.host == host && !key.starts_with("ip:")
                })
                .map(|(key, _)| key.clone());
            if let Some(known_id) = known {
                id = known_id;
            }
        }

        let mut changed = false;

        if !store.peers.contains_key(&id) && !host.is_empty() && kind == KIND_DEVICE {
            let placeholder = store
                .peers
                .iter()
                .find(|(key, peer)| {
                    key.as_str() != id
                        && peer.kind == KIND_DEVICE
                        && peer.host == host
                        && (port == 0 || peer.port == 0 || peer.port == port)
                })
                .map(|(key, _)| key.clone());
            if let Some(old_key) = placeholder {
                if let Some(mut peer) = store.peers.remove(&old_key) {
                    for message in store
                        .messages
                        .iter_mut()
                        .filter(|message| message.peer_id == old_key)
                    {
                        message.peer_id = id.clone();
                    }
                    peer.id = id.clone();
                    store.peers.insert(id.clone(), peer);
                    changed = true;
                }
            }
        }

        let known = store.peers.contains_key(&id);
        let entry = store
            .peers
            .entry(id.clone())
            .or_insert_with(|| ChatPeer::new(&id, name, host, port, kind));
        if (identified || !known) && !name.trim().is_empty() && entry.name != name.trim() {
            entry.name = name.trim().to_string();
            changed = true;
        }
        if !host.is_empty() && entry.host != host {
            entry.host = host.to_string();
            changed = true;
        }
        if port != 0 && entry.port != port {
            entry.port = port;
            changed = true;
        }
        if !kind.is_empty() && entry.kind != kind {
            entry.kind = kind.to_string();
            changed = true;
        }
        let peer = entry.clone();
        if changed {
            self.persist(&store);
        }
        peer
    }

    /// Updates endpoints of conversations already on the list when mDNS sees
    /// them again. Discovery never creates a conversation on its own.
    pub fn sync_discovery(&self, observed: &[(String, String, String, u16)]) {
        let mut store = self.lock();
        let mut changed = false;
        for (id, name, host, port) in observed {
            let Some(peer) = store.peers.get_mut(id) else {
                continue;
            };
            if !name.trim().is_empty() && peer.name != name.trim() {
                peer.name = name.trim().to_string();
                changed = true;
            }
            if !host.is_empty() && peer.host != *host {
                peer.host = host.clone();
                changed = true;
            }
            if *port != 0 && peer.port != *port {
                peer.port = *port;
                changed = true;
            }
        }
        if changed {
            self.persist(&store);
        }
    }

    pub fn record(&self, message: ChatMessage) -> ChatMessage {
        let mut store = self.lock();
        let message = insert(&mut store, message);
        self.persist(&store);
        message
    }

    /// Whether a message with this id is already on file — a resent upload asks
    /// before it writes the bytes a second time.
    pub fn contains(&self, message_id: &str) -> bool {
        self.lock()
            .messages
            .iter()
            .any(|message| message.id == message_id)
    }

    /// Records an outgoing bubble the peer has not received yet and puts the
    /// delivery in the outbox. The bubble appears immediately — the queue is
    /// what lets the sender treat the send as finished and walk away.
    ///
    /// The second half of the return is what the queue had to throw away to fit
    /// it in: a phone staged those bytes, so the caller owns deleting them.
    pub fn enqueue(&self, mut message: ChatMessage, item: QueuedItem) -> (ChatMessage, Vec<QueuedItem>) {
        message.queued = true;
        let mut store = self.lock();
        let message = insert(&mut store, message);
        store.outbox.push(item);
        let mut evicted = Vec::new();
        if store.outbox.len() > MAX_QUEUED {
            let surplus = store.outbox.len() - MAX_QUEUED;
            let dropped: std::collections::HashSet<String> = store.outbox[..surplus]
                .iter()
                .map(|item| item.message_id.clone())
                .collect();
            evicted = store.outbox[..surplus].to_vec();
            store.outbox.drain(..surplus);
            for message in store
                .messages
                .iter_mut()
                .filter(|message| dropped.contains(&message.id))
            {
                message.queued = false;
                message.failed = true;
            }
        }
        self.persist(&store);
        (message, evicted)
    }

    pub fn outbox(&self) -> Vec<QueuedItem> {
        self.lock().outbox.clone()
    }

    /// A queued send that has now arrived: the bubble loses its clock.
    pub fn dequeue(&self, message_id: &str) -> Option<ChatMessage> {
        let mut store = self.lock();
        store.outbox.retain(|item| item.message_id != message_id);
        let message = store
            .messages
            .iter_mut()
            .find(|message| message.id == message_id)?;
        message.queued = false;
        let clone = message.clone();
        self.persist(&store);
        Some(clone)
    }

    /// A queued send that will never arrive — the file went away, the item aged
    /// out. Show it as failed so the bubble stops promising a retry.
    pub fn drop_queued(&self, message_id: &str) -> Option<ChatMessage> {
        let mut store = self.lock();
        store.outbox.retain(|item| item.message_id != message_id);
        let message = store
            .messages
            .iter_mut()
            .find(|message| message.id == message_id)?;
        message.queued = false;
        message.failed = true;
        let clone = message.clone();
        self.persist(&store);
        Some(clone)
    }

    pub fn queued_count(&self, peer_id: &str) -> usize {
        self.lock()
            .outbox
            .iter()
            .filter(|item| item.peer_id == peer_id)
            .count()
    }

    pub fn mark_read(&self, peer_id: &str) -> bool {
        let mut store = self.lock();
        let Some(peer) = store.peers.get_mut(peer_id) else {
            return false;
        };
        if peer.unread == 0 {
            return false;
        }
        peer.unread = 0;
        self.persist(&store);
        true
    }
}

fn insert(store: &mut Store, message: ChatMessage) -> ChatMessage {
    // The same message can arrive twice: a sender that lost the acknowledgement
    // resends, and nothing on this side used to notice. The id is the sender's,
    // so a repeat is the same message, not a new one.
    if let Some(existing) = store.messages.iter().find(|known| known.id == message.id) {
        return existing.clone();
    }
    let peer = store
        .peers
        .entry(message.peer_id.clone())
        .or_insert_with(|| {
            ChatPeer::new(
                &message.peer_id,
                &message.peer_name,
                "",
                0,
                KIND_DEVICE,
            )
        });
    peer.last_at = message.at;
    peer.preview = message.preview();
    peer.last_kind = message.kind.clone();
    if message.direction == "in" {
        peer.unread = peer.unread.saturating_add(1);
    }
    store.messages.push(message.clone());
    trim_peer(store, &message.peer_id);
    trim_peers(store);
    message
}

fn trim_peer(store: &mut Store, peer_id: &str) {
    let mut excess = 0usize;
    let mut total = 0usize;
    for message in store.messages.iter().rev() {
        if message.peer_id != peer_id {
            continue;
        }
        total += 1;
        if total > MAX_MESSAGES_PER_PEER {
            excess += 1;
        }
    }
    if excess == 0 {
        return;
    }
    let mut skipped = 0usize;
    store.messages.retain(|message| {
        if message.peer_id != peer_id {
            return true;
        }
        skipped += 1;
        skipped > excess
    });
}

fn trim_peers(store: &mut Store) {
    if store.peers.len() <= MAX_PEERS {
        return;
    }
    let mut ordered: Vec<(String, u64)> = store
        .peers
        .values()
        .map(|peer| (peer.id.clone(), peer.last_at))
        .collect();
    ordered.sort_by_key(|(_, at)| *at);
    let surplus = store.peers.len() - MAX_PEERS;
    for (id, _) in ordered.into_iter().take(surplus) {
        store.peers.remove(&id);
        store.messages.retain(|message| message.peer_id != id);
    }
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_millis() as u64)
        .unwrap_or(0)
}

pub fn new_id() -> String {
    Uuid::new_v4().to_string()
}

/// `chats.json` sits next to `filedrop.json`, in the app's config directory.
pub fn storage_path(app: &tauri::AppHandle) -> PathBuf {
    use tauri::Manager;
    app.path()
        .app_config_dir()
        .map(|dir| dir.join("chats.json"))
        .unwrap_or_else(|_| std::env::temp_dir().join("filedrop-chats.json"))
}

// ------------------------------------------------------------------- events

/// Sends the stored message together with its refreshed conversation meta, so
/// the list and the open chat update from one frame.
pub fn broadcast(chats: &Chats, message: &ChatMessage) {
    super::server::emit_to_ui(
        "chat-message",
        serde_json::json!({
            "message": message,
            "peer": chats.peer(&message.peer_id),
        }),
    );
    if message.direction == "in" {
        if let Some(app) = super::server::app_handle() {
            notify_inbound(
                &app,
                &message.peer_id,
                &message.peer_name,
                &message.kind,
                &message.text,
                &message.file_name,
            );
        }
    }
}

/// System notification for something that landed while FileDrop was not the
/// window in front — otherwise a file or a message that arrives into the tray
/// is invisible until the user opens the app again.
pub fn notify_inbound(
    app: &tauri::AppHandle,
    peer_id: &str,
    peer_name: &str,
    kind: &str,
    text: &str,
    file_name: &str,
) {
    use tauri::Manager;
    #[cfg(not(windows))]
    use tauri_plugin_notification::NotificationExt;

    if let Some(window) = app.get_webview_window("main") {
        // The transcript is already on screen; a toast would only double it.
        if window.is_visible().unwrap_or(false) && window.is_focused().unwrap_or(false) {
            return;
        }
    }
    let enabled = app
        .try_state::<super::Transfer>()
        .map(|transfer| transfer.config().notify)
        .unwrap_or(true);
    if !enabled {
        return;
    }

    let clip = |value: &str| {
        let chars: Vec<char> = value.chars().collect();
        if chars.len() <= 90 {
            value.to_string()
        } else {
            format!("{}…", chars[..90].iter().collect::<String>())
        }
    };
    let (title, body) = if kind == "file" {
        (format!("收到文件 · {peer_name}"), clip(file_name))
    } else {
        (peer_name.to_string(), clip(text))
    };

    #[cfg(windows)]
    {
        // The plugin's toast cannot report a click, so Windows gets one that
        // brings the window and the conversation back.
        crate::notify_win::show(app, title, body, Some(peer_id.to_string()));
    }

    #[cfg(not(windows))]
    {
        let _ = peer_id;
        if let Err(error) = app.notification().builder().title(title).body(body).show() {
            tracing::warn!("Notification failed: {error}");
        }
    }
}

pub fn emit_file_start(
    id: &str,
    peer_id: &str,
    peer_name: &str,
    name: &str,
    bytes: u64,
    direction: &str,
) {
    super::server::emit_to_ui(
        "chat-file-start",
        serde_json::json!({
            "id": id,
            "peerId": peer_id,
            "peerName": peer_name,
            "name": name,
            "bytes": bytes,
            "direction": direction,
        }),
    );
}

pub fn emit_file_progress(id: &str, received: u64, total: u64) {
    super::server::emit_to_ui(
        "chat-file-progress",
        serde_json::json!({
            "id": id,
            "received": received,
            "total": total,
        }),
    );
}

/// Drops a placeholder bubble whose transfer was replaced or interrupted.
pub fn emit_file_cancelled(id: &str) {
    super::server::emit_to_ui("chat-file-cancelled", serde_json::json!({ "id": id }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_resent_message_is_not_a_second_bubble() {
        let chats = Chats::default();
        let peer = chats.ensure_peer("p", "Peer", "127.0.0.1", 8787, KIND_DEVICE);
        let message = ChatMessage::text(&peer, "in", "vvcnn".to_string());
        chats.record(message.clone());
        chats.record(message.clone());
        assert_eq!(chats.messages("p").len(), 1, "the resend must not duplicate");
        assert!(chats.contains(&message.id));
    }

    fn temp_path(tag: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("filedrop-chat-{tag}-{}", new_id()))
            .join("chats.json")
    }

    #[test]
    fn conversations_sort_by_latest_message() {
        let chats = Chats::default();
        let alice = chats.ensure_peer("a", "Alice", "192.168.1.2", 8787, KIND_DEVICE);
        let bob = chats.ensure_peer("b", "Bob", "192.168.1.3", 8787, KIND_DEVICE);
        chats.record(ChatMessage::text(&alice, "in", "你好".to_string()));
        let mut bob_message = ChatMessage::text(&bob, "out", "在吗".to_string());
        bob_message.at += 10;
        chats.record(bob_message);

        let list = chats.list();
        assert_eq!(list[0].id, "b");
        assert_eq!(list[0].preview, "在吗");
        assert_eq!(list[1].unread, 1);
        assert!(chats.mark_read("a"));
        assert_eq!(chats.peer("a").unwrap().unread, 0);
    }

    #[test]
    fn messages_survive_a_reload() {
        let path = temp_path("reload");
        let alice;
        {
            let chats = Chats::load(path.clone());
            alice = chats.ensure_peer("a", "Alice", "192.168.1.2", 8787, KIND_DEVICE);
            chats.record(ChatMessage::text(&alice, "out", "hello".to_string()));
        }
        let chats = Chats::load(path.clone());
        assert_eq!(chats.messages("a").len(), 1);
        assert_eq!(chats.list()[0].name, "Alice");
        assert_eq!(chats.list()[0].preview, "hello");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_queued_send_clears_when_it_arrives() {
        let chats = Chats::default();
        let phone = chats.ensure_peer("p", "Phone", "192.168.1.5", 8787, KIND_DEVICE);
        let message = ChatMessage::text(&phone, "out", "上线后发给你".to_string());
        let item = QueuedItem {
            message_id: message.id.clone(),
            peer_id: phone.id.clone(),
            kind: "text".to_string(),
            text: message.text.clone(),
            path: String::new(),
            at: message.at,
        };

        let (queued, evicted) = chats.enqueue(message, item);
        assert!(evicted.is_empty(), "the queue has room");
        assert!(queued.queued, "the bubble must show as waiting");
        assert_eq!(chats.queued_count("p"), 1);
        assert_eq!(chats.list()[0].preview, "上线后发给你");

        let arrived = chats.dequeue(&queued.id).expect("the bubble is on file");
        assert!(!arrived.queued);
        assert!(!arrived.failed);
        assert_eq!(chats.queued_count("p"), 0);
    }

    #[test]
    fn a_queue_that_cannot_be_delivered_shows_as_failed() {
        let chats = Chats::default();
        let phone = chats.ensure_peer("p", "Phone", "192.168.1.5", 8787, KIND_DEVICE);
        let message = ChatMessage::text(&phone, "out", "等不到你了".to_string());
        let item = QueuedItem {
            message_id: message.id.clone(),
            peer_id: phone.id.clone(),
            kind: "text".to_string(),
            text: message.text.clone(),
            path: String::new(),
            at: message.at,
        };
        let (queued, _) = chats.enqueue(message, item);

        let failed = chats.drop_queued(&queued.id).expect("the bubble is on file");
        assert!(failed.failed);
        assert!(!failed.queued);
        assert_eq!(chats.queued_count("p"), 0);
    }

    #[test]
    fn queued_sends_survive_a_reload() {
        let path = temp_path("queue");
        let id;
        {
            let chats = Chats::load(path.clone());
            let phone = chats.ensure_peer("p", "Phone", "192.168.1.5", 8787, KIND_DEVICE);
            let message = ChatMessage::text(&phone, "out", "重启后继续发".to_string());
            id = message.id.clone();
            chats.enqueue(
                message,
                QueuedItem {
                    message_id: id.clone(),
                    peer_id: "p".to_string(),
                    kind: "text".to_string(),
                    text: "重启后继续发".to_string(),
                    path: String::new(),
                    at: now_ms(),
                },
            );
        }
        let chats = Chats::load(path.clone());
        assert_eq!(chats.queued_count("p"), 1);
        assert!(chats.messages("p")[0].queued);
        assert!(chats.dequeue(&id).is_some());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn manual_placeholder_is_folded_into_the_real_instance() {
        let chats = Chats::default();
        let manual = chats.ensure_peer(
            "manual:192.168.1.9:8787",
            "192.168.1.9",
            "192.168.1.9",
            8787,
            KIND_DEVICE,
        );
        chats.record(ChatMessage::text(&manual, "out", "ping".to_string()));
        chats.ensure_peer("real-id", "Office PC", "192.168.1.9", 8787, KIND_DEVICE);

        assert!(chats.peer("manual:192.168.1.9:8787").is_none());
        assert_eq!(chats.messages("real-id").len(), 1);
        assert_eq!(chats.peer("real-id").unwrap().name, "Office PC");
    }

    #[test]
    fn unidentified_inbound_stays_on_the_known_device() {
        let chats = Chats::default();
        let phone = chats.ensure_peer("real-id", "Office PC", "192.168.1.9", 8787, KIND_DEVICE);
        chats.record(ChatMessage::text(&phone, "in", "hi".to_string()));
        let anonymous = chats.ensure_peer(
            "",
            "局域网设备 192.168.1.9",
            "192.168.1.9",
            8787,
            KIND_DEVICE,
        );
        chats.record(ChatMessage::text(&anonymous, "in", "网页发来的".to_string()));

        assert!(chats.peer("ip:192.168.1.9").is_none());
        assert_eq!(chats.messages("real-id").len(), 2);
        assert_eq!(chats.peer("real-id").unwrap().name, "Office PC");
    }
}
