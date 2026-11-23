//! The queue behind "send it once they come back".
//!
//! A send to an offline device records the bubble and lands here; this loop is
//! the only thing that empties it, so it has to run from Rust. On Android the
//! webview's timers stop when the app is backgrounded, which is exactly when a
//! queued message still has to go out.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::time::{sleep, timeout};

use super::chat::{self, ChatMessage, ChatPeer, Chats, QueuedItem, KIND_BROWSER};
use super::{client, server, stage, Transfer};

/// How often to look for a device that came back.
const TICK: Duration = Duration::from_secs(5);
/// Long enough for a healthy LAN peer, short enough that one dead address does
/// not stall the rest of the queue for the duration of a TCP timeout.
const ATTEMPT: Duration = Duration::from_secs(8);

pub fn spawn(app: tauri::AppHandle) {
    use tauri::Manager;
    tauri::async_runtime::spawn(async move {
        let staged = stage::staged_dir(&app);
        // A phone that was mid-staging when the process died left bytes no
        // queued send can name; they are nobody's business any more.
        if let Some(chats) = app.try_state::<Chats>() {
            let keep: Vec<String> = chats
                .outbox()
                .into_iter()
                .filter(|item| item.kind == "file")
                .map(|item| item.path)
                .collect();
            stage::purge(&staged, &keep);
        }
        loop {
            sleep(TICK).await;
            flush(&app).await;
        }
    });
}

/// Delivers everything whose peer has reappeared, oldest first. A peer that
/// fails is skipped for the rest of the pass — its later items could not have
/// arrived either, and the next tick retries them in order.
async fn flush(app: &tauri::AppHandle) {
    use tauri::Manager;
    let Some(chats) = app.try_state::<Chats>() else {
        return;
    };
    let Some(transfer) = app.try_state::<Transfer>() else {
        return;
    };
    let queue = chats.outbox();
    if queue.is_empty() {
        return;
    }
    let staged = stage::staged_dir(app);
    let now = chat::now_ms();
    let mut waiting = HashSet::new();
    for item in queue {
        if now.saturating_sub(item.at) > chat::QUEUE_TTL {
            if let Some(failed) = chats.drop_queued(&item.message_id) {
                chat::broadcast(&chats, &failed);
            }
            stage::release(&staged, &item.path);
            continue;
        }
        if waiting.contains(&item.peer_id) {
            continue;
        }
        match deliver(&chats, &transfer, &item, &staged).await {
            Delivery::Sent(Some(message)) => chat::broadcast(&chats, &message),
            Delivery::Sent(None) => {}
            Delivery::Waiting => {
                waiting.insert(item.peer_id.clone());
            }
            Delivery::Abandoned => {}
        }
    }
}

/// The send the composer is waiting on. `Some(message)` means the peer has it;
/// `None` means it stayed in the queue and the loop will retry it.
///
/// A peer the app already knows is away is not probed here: the composer should
/// answer at once, and the tick will find the device when it returns.
pub async fn send_now(
    chats: &Chats,
    transfer: &Transfer,
    item: &QueuedItem,
    staged: &Path,
) -> Option<ChatMessage> {
    let peer = chats.peer(&item.peer_id)?;
    if !reachable(transfer, &peer).await {
        return None;
    }
    match deliver(chats, transfer, item, staged).await {
        Delivery::Sent(message) => message,
        _ => None,
    }
}

/// The same signal the device list shows: mDNS for an app, a live socket for a
/// browser tab.
pub async fn reachable(transfer: &Transfer, peer: &ChatPeer) -> bool {
    if peer.kind == KIND_BROWSER {
        return server::browser_online(&peer.id).await;
    }
    // A conversation added by address is pinned by the user and never shows up
    // in discovery, so presence there says nothing about it; the address it was
    // given is what the send would use.
    if peer.id.starts_with("manual:") {
        return !peer.host.trim().is_empty() && peer.port != 0;
    }
    transfer
        .peers()
        .iter()
        .any(|observed| observed.id == peer.id)
}

enum Delivery {
    /// Delivered; the message with its queue flag cleared, when it is on file.
    Sent(Option<ChatMessage>),
    /// Peer still unreachable — keep it queued and try again next tick.
    Waiting,
    /// Nothing to send any more (conversation gone, file deleted).
    Abandoned,
}

impl Delivery {
    /// What a test asked for when a delivery came back as the wrong variant.
    #[cfg(test)]
    fn name(&self) -> &'static str {
        match self {
            Delivery::Sent(_) => "sent",
            Delivery::Waiting => "waiting",
            Delivery::Abandoned => "abandoned",
        }
    }
}

async fn deliver(
    chats: &Chats,
    transfer: &Transfer,
    item: &QueuedItem,
    staged: &Path,
) -> Delivery {
    let Some(peer) = chats.peer(&item.peer_id) else {
        return abandon(chats, item, staged);
    };
    let config = transfer.config();

    if peer.kind == KIND_BROWSER {
        if item.kind != "text" || !server::browser_online(&peer.id).await {
            return Delivery::Waiting;
        }
        let text = item.text.clone();
        let peer_id = peer.id.clone();
        return match timeout(ATTEMPT, server::send_text_to_browser(&peer_id, &text)).await {
            Ok(Ok(())) => Delivery::Sent(arrived(chats, item)),
            _ => Delivery::Waiting,
        };
    }

    let Some((host, port)) = endpoint(transfer, &peer) else {
        return Delivery::Waiting;
    };
    let identity = client::SendIdentity {
        id: &config.instance_id,
        name: &config.device_name,
        port: config.port,
    };

    if item.kind == "text" {
        let text = item.text.clone();
        let attempt = timeout(ATTEMPT, client::send_text(&host, port, &identity, &item.message_id, &text)).await;
        return match attempt {
            Ok(Ok(())) => Delivery::Sent(arrived(chats, item)),
            Ok(Err(error)) => {
                tracing::debug!("Queued text for {host}:{port} still waiting: {error}");
                Delivery::Waiting
            }
            Err(_) => Delivery::Waiting,
        };
    }

    let path = PathBuf::from(&item.path);
    if !path.is_file() {
        return abandon(chats, item, staged);
    }
    let total = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
    let message_id = item.message_id.clone();
    let result = client::send_file(&host, port, &path, Some(&identity), Some(&item.message_id), |bytes| {
        chat::emit_file_progress(&message_id, bytes, total);
    })
    .await;
    match result {
        Ok(bytes) => {
            chat::emit_file_progress(&message_id, bytes, total);
            // A phone staged a copy to get here; the peer owns it now.
            stage::release(staged, &item.path);
            Delivery::Sent(arrived(chats, item))
        }
        Err(error) => {
            tracing::debug!("Queued file for {host}:{port} still waiting: {error}");
            Delivery::Waiting
        }
    }
}

/// Discovery's address wins over the one stored with the conversation: a laptop
/// that moved to another access point answers on a new IP.
fn endpoint(transfer: &Transfer, peer: &chat::ChatPeer) -> Option<(String, u16)> {
    if let Some(observed) = transfer.peers().into_iter().find(|p| p.id == peer.id) {
        return Some((observed.host, observed.port));
    }
    if peer.host.trim().is_empty() || peer.port == 0 {
        return None;
    }
    Some((peer.host.clone(), peer.port))
}

/// Nothing to retry any more: take it out of the queue and let the bubble say
/// failed, so the transcript stops promising a send that cannot happen.
fn abandon(chats: &Chats, item: &QueuedItem, staged: &Path) -> Delivery {
    if let Some(failed) = chats.drop_queued(&item.message_id) {
        chat::broadcast(chats, &failed);
    }
    stage::release(staged, &item.path);
    Delivery::Abandoned
}

fn arrived(chats: &Chats, item: &QueuedItem) -> Option<ChatMessage> {
    chats.dequeue(&item.message_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transfer::config::Config;
    use chat::{KIND_DEVICE, MAX_QUEUED};
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn config() -> Config {
        Config {
            device_name: "Sender".to_string(),
            storage_dir: "sender".to_string(),
            port: 9001,
            auto_start: false,
            notify: false,
            instance_id: "sender-id".to_string(),
        }
    }

    /// A peer that answers `/message` with 200, and what it was actually asked.
    struct FakePeer {
        port: u16,
        seen: Arc<Mutex<String>>,
    }

    async fn answering_peer() -> FakePeer {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen = Arc::new(Mutex::new(String::new()));
        let sink = Arc::clone(&seen);
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                // Answer once the whole request is in — the sender only reads
                // the reply after it has finished writing, so waiting for the
                // socket to close here would deadlock it.
                let mut request = Vec::new();
                let mut buffer = [0u8; 1024];
                loop {
                    let read = match socket.read(&mut buffer).await {
                        Ok(0) | Err(_) => break,
                        Ok(read) => read,
                    };
                    request.extend_from_slice(&buffer[..read]);
                    let head = String::from_utf8_lossy(&request).to_string();
                    let Some(split) = head.find("\r\n\r\n") else {
                        continue;
                    };
                    let declared = head[..split]
                        .lines()
                        .find_map(|line| line.strip_prefix("Content-Length:"))
                        .and_then(|value| value.trim().parse::<usize>().ok())
                        .unwrap_or(0);
                    if request.len() >= split + 4 + declared {
                        break;
                    }
                }
                if let Ok(mut slot) = sink.lock() {
                    *slot = String::from_utf8_lossy(&request).to_string();
                }
                let _ = socket
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                    .await;
                let _ = socket.shutdown().await;
            }
        });
        FakePeer { port, seen }
    }

    impl FakePeer {
        fn request(&self) -> String {
            self.seen.lock().unwrap().clone()
        }
    }

    /// A folder of its own per test, because delivery spends staged copies.
    fn staging(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("filedrop-outbox-{tag}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn queued_text(chats: &Chats, peer_id: &str, host: &str, port: u16) -> (ChatMessage, QueuedItem) {
        let peer = chats.ensure_peer(peer_id, "Peer", host, port, KIND_DEVICE);
        let message = ChatMessage::text(&peer, "out", "上线了发给你".to_string());
        let item = QueuedItem {
            message_id: message.id.clone(),
            peer_id: peer.id.clone(),
            kind: "text".to_string(),
            text: message.text.clone(),
            path: String::new(),
            at: message.at,
        };
        (chats.enqueue(message, item.clone()).0, item)
    }

    #[tokio::test]
    async fn a_queued_text_goes_out_when_the_peer_answers() {
        let peer = answering_peer().await;
        let chats = Chats::default();
        let transfer = Transfer::new(config());
        let staged = staging("text");
        let (queued, item) = queued_text(&chats, "p", "127.0.0.1", peer.port);
        assert!(queued.queued, "the bubble must start out waiting");

        match deliver(&chats, &transfer, &item, &staged).await {
            Delivery::Sent(Some(delivered)) => assert!(!delivered.queued),
            other => panic!("expected a delivery, got {}", other.name()),
        }
        assert_eq!(chats.queued_count("p"), 0, "the outbox must empty");
        assert!(!chats.messages("p")[0].queued);

        // The retry must speak the door the real receiver answers, with our
        // identity on it — otherwise the peer files the message under a stranger.
        let request = peer.request();
        assert!(request.starts_with("POST /message HTTP/1.1"), "{request}");
        assert!(request.contains("\"fromId\":\"sender-id\""), "{request}");
        assert!(request.contains("\"text\":\"上线了发给你\""), "{request}");
    }

    #[tokio::test]
    async fn a_peer_that_refuses_keeps_the_send_queued() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let dead_port = listener.local_addr().unwrap().port();
        drop(listener);

        let chats = Chats::default();
        let transfer = Transfer::new(config());
        let staged = staging("refuses");
        let (_, item) = queued_text(&chats, "p", "127.0.0.1", dead_port);

        assert!(matches!(
            deliver(&chats, &transfer, &item, &staged).await,
            Delivery::Waiting
        ));
        assert_eq!(chats.queued_count("p"), 1);
        assert!(!chats.messages("p")[0].failed, "waiting is not failing");
    }

    #[tokio::test]
    async fn the_composer_does_not_wait_for_a_peer_discovery_has_not_seen() {
        let peer = answering_peer().await;
        let chats = Chats::default();
        let transfer = Transfer::new(config());
        let staged = staging("unseen");
        let (_, item) = queued_text(&chats, "p", "127.0.0.1", peer.port);

        assert!(send_now(&chats, &transfer, &item, &staged).await.is_none());
        assert_eq!(chats.queued_count("p"), 1, "the tick owns the retry");
        assert!(peer.request().is_empty(), "an unseen peer is not probed");
    }

    #[tokio::test]
    async fn a_file_whose_source_is_gone_is_given_up_on() {
        let peer = answering_peer().await;
        let chats = Chats::default();
        let transfer = Transfer::new(config());
        let staged = staging("gone");
        let device = chats.ensure_peer("p", "Peer", "127.0.0.1", peer.port, KIND_DEVICE);
        let message = ChatMessage::file(
            &device,
            "out",
            "gone.zip".to_string(),
            "C:\\definitely-not-here\\gone.zip".to_string(),
            12,
        );
        let item = QueuedItem {
            message_id: message.id.clone(),
            peer_id: "p".to_string(),
            kind: "file".to_string(),
            text: String::new(),
            path: message.file_path.clone(),
            at: message.at,
        };
        chats.enqueue(message, item.clone());

        assert!(matches!(
            deliver(&chats, &transfer, &item, &staged).await,
            Delivery::Abandoned
        ));
        assert!(chats.messages("p")[0].failed);
        assert_eq!(chats.queued_count("p"), 0);
    }

    #[tokio::test]
    async fn a_staged_phone_copy_is_spent_once_the_peer_has_it() {
        use base64::Engine;

        let peer = answering_peer().await;
        let chats = Chats::default();
        let transfer = Transfer::new(config());
        let staged = staging("phone");
        let copy = super::stage::open(&staged, "IMG_2043.jpg").unwrap();
        super::stage::append(
            &staged,
            copy.to_str().unwrap(),
            &base64::engine::general_purpose::STANDARD.encode(b"photo bytes"),
        )
        .unwrap();

        let device = chats.ensure_peer("p", "Peer", "127.0.0.1", peer.port, KIND_DEVICE);
        let name = copy.file_name().unwrap().to_string_lossy().to_string();
        let bytes = std::fs::metadata(&copy).unwrap().len();
        let message = ChatMessage::file(&device, "out", name, copy.to_string_lossy().to_string(), bytes);
        let item = QueuedItem {
            message_id: message.id.clone(),
            peer_id: "p".to_string(),
            kind: "file".to_string(),
            text: String::new(),
            path: message.file_path.clone(),
            at: message.at,
        };
        chats.enqueue(message, item.clone());

        match deliver(&chats, &transfer, &item, &staged).await {
            Delivery::Sent(Some(delivered)) => assert_eq!(delivered.file_name, "IMG_2043.jpg"),
            other => panic!("expected a delivery, got {}", other.name()),
        }

        // The peer is sent the copy under the name the picker reported, with the
        // staged id folder left out of the request.
        let request = peer.request();
        assert!(request.starts_with("POST /upload HTTP/1.1"), "{request}");
        assert!(
            request.contains("filename=\"IMG_2043.jpg\""),
            "the staged folder must not leak into the name: {request}"
        );
        assert!(
            request.contains("name=\"fromId\"\r\n\r\nsender-id"),
            "the peer must file the copy under this device: {request}"
        );
        assert!(!copy.exists(), "a delivered copy is spent");
        assert_eq!(chats.queued_count("p"), 0, "the queue must empty");
        let _ = std::fs::remove_dir_all(&staged);
    }

    /// A phone that queues more than the window holds gives back the copies it
    /// no longer owes, so the caller can delete their bytes.
    #[test]
    fn the_oldest_queued_send_is_the_one_dropped_when_the_queue_fills() {
        let chats = Chats::default();
        let staged = staging("overflow");
        let peer = chats.ensure_peer("p", "Peer", "192.168.1.5", 9009, KIND_DEVICE);
        let mut first = String::new();
        let mut oldest_copy = PathBuf::new();
        let mut newest_copy = PathBuf::new();
        let mut evicted = Vec::new();
        for index in 0..=MAX_QUEUED {
            let message = ChatMessage::text(&peer, "out", format!("{index}"));
            let id = message.id.clone();
            if index == 0 {
                first = id.clone();
                oldest_copy = super::stage::open(&staged, "oldest.txt").unwrap();
            }
            if index == MAX_QUEUED {
                newest_copy = super::stage::open(&staged, "newest.txt").unwrap();
            }
            // the ends of the window own a staged copy; the rest is filler
            let copy = match index {
                0 => oldest_copy.clone(),
                _ if index == MAX_QUEUED => newest_copy.clone(),
                _ => PathBuf::new(),
            };
            evicted.extend(
                chats
                    .enqueue(
                        message,
                        QueuedItem {
                            message_id: id,
                            peer_id: "p".to_string(),
                            kind: "file".to_string(),
                            text: format!("{index}"),
                            path: copy.to_string_lossy().to_string(),
                            at: chat::now_ms(),
                        },
                    )
                    .1,
            );
        }
        assert_eq!(chats.outbox().len(), MAX_QUEUED);
        assert!(
            chats.messages("p").iter().any(|message| message.id == first && message.failed),
            "the dropped bubble must stop promising a retry"
        );

        assert_eq!(evicted.len(), 1, "one send per overflow");
        assert_eq!(evicted[0].message_id, first, "the oldest is the one spent");
        super::stage::release(&staged, &evicted[0].path);
        assert!(!oldest_copy.exists(), "eviction must free the staged bytes");
        assert!(newest_copy.exists(), "the newest copy is still owed");
        let _ = std::fs::remove_dir_all(&staged);
    }
}
