//! The client half of app-to-app transfer: one streaming multipart POST to a
//! peer's `/upload`, plus the two small JSON calls in front of it — `/message`
//! for chat text and `/health` for a new conversation's identity. No HTTP crate
//! for three requests; the server streams the body, so this side streams the
//! file out of it.

use std::path::Path;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Who is sending. The receiver stores these fields with the incoming file so
/// the conversation list can answer "which device sent this" even after the
/// socket is gone.
pub struct SendIdentity<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub port: u16,
}

fn host_header(host: &str, port: u16) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

fn field(boundary: &str, name: &str, value: &str) -> String {
    format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
    )
}

/// Returns the number of file bytes written. `on_sent` is called with the
/// running total after every disk read, which is what the UI's progress bar
/// follows.
pub async fn send_file(
    host: &str,
    port: u16,
    path: &Path,
    identity: Option<&SendIdentity<'_>>,
    message_id: Option<&str>,
    mut on_sent: impl FnMut(u64),
) -> Result<u64, String> {
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .ok_or_else(|| "这个路径不是文件".to_string())?;
    // The name goes inside a quoted multipart header; a quote or a newline in it
    // would split the header and corrupt the request.
    let header_name = file_name.replace(['"', '\r', '\n'], "_");
    let bytes = tokio::fs::metadata(path)
        .await
        .map_err(|error| format!("读取文件失败：{error}"))?
        .len();
    let boundary = format!("----FileDrop{}", std::process::id());
    let mut preamble = String::new();
    if let Some(identity) = identity {
        preamble.push_str(&field(&boundary, "fromId", identity.id));
        preamble.push_str(&field(&boundary, "fromName", identity.name));
        preamble.push_str(&field(&boundary, "fromPort", &identity.port.to_string()));
    }
    if let Some(message_id) = message_id {
        // Same reason as the text path: a resent upload must be dismissible as
        // a repeat instead of landing a second copy.
        preamble.push_str(&field(&boundary, "messageId", message_id));
    }
    preamble.push_str(&format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{header_name}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
    ));
    let tail = format!("\r\n--{boundary}--\r\n");
    let body_len = preamble.len() as u64 + bytes + tail.len() as u64;

    let mut stream = TcpStream::connect((host, port))
        .await
        .map_err(|error| format!("连不上 {host}:{port}：{error}"))?;
    let request = format!(
        "POST /upload HTTP/1.1\r\nHost: {}\r\nContent-Type: multipart/form-data; boundary={boundary}\r\nContent-Length: {body_len}\r\nConnection: close\r\n\r\n{preamble}",
        host_header(host, port)
    );
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|error| error.to_string())?;

    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|error| format!("打开文件失败：{error}"))?;
    let mut buffer = vec![0u8; 256 * 1024];
    let mut sent = 0u64;
    loop {
        let read = file
            .read(&mut buffer)
            .await
            .map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        stream
            .write_all(&buffer[..read])
            .await
            .map_err(|error| format!("发送中断：{error}"))?;
        sent += read as u64;
        on_sent(sent);
    }
    stream
        .write_all(tail.as_bytes())
        .await
        .map_err(|error| error.to_string())?;
    stream.flush().await.map_err(|error| error.to_string())?;

    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .await
        .map_err(|error| format!("等待对端确认失败：{error}"))?;
    let status = String::from_utf8_lossy(&response)
        .lines()
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .nth(1)
        .unwrap_or_default()
        .to_string();
    if status != "200" {
        return Err(format!("对端拒绝了文件（HTTP {status}）"));
    }
    Ok(sent)
}

/// `POST /message` — the chat text door. Returns the peer's HTTP status turned
/// into a Chinese error so the composer can put the text back.
pub async fn send_text(
    host: &str,
    port: u16,
    identity: &SendIdentity<'_>,
    message_id: &str,
    text: &str,
) -> Result<(), String> {
    let body = serde_json::json!({
        "fromId": identity.id,
        "fromName": identity.name,
        "fromPort": identity.port,
        // The receiver keeps this id, so a resend after a lost acknowledgement
        // is recognised as the same message instead of a second bubble.
        "id": message_id,
        "text": text,
    })
    .to_string();
    let mut stream = TcpStream::connect((host, port))
        .await
        .map_err(|error| format!("连不上 {host}:{port}：{error}"))?;
    let request = format!(
        "POST /message HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        host_header(host, port),
        body.len()
    );
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|error| format!("发送失败：{error}"))?;
    stream.flush().await.map_err(|error| error.to_string())?;
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .await
        .map_err(|error| format!("等待对端确认失败：{error}"))?;
    let status = String::from_utf8_lossy(&response)
        .lines()
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .nth(1)
        .unwrap_or_default()
        .to_string();
    if status != "200" {
        return Err(format!("对方没有接收这条消息（HTTP {status}）"));
    }
    Ok(())
}

/// `GET /health`: the instance id behind a manually typed address, so a hand
/// added device does not split into two conversations once it answers.
pub async fn probe(host: &str, port: u16) -> Option<(String, String, u16)> {
    use tokio::time::{timeout, Duration};

    let request = format!(
        "GET /health HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        host_header(host, port)
    );
    let mut stream = timeout(Duration::from_secs(2), TcpStream::connect((host, port)))
        .await
        .ok()?
        .ok()?;
    stream.write_all(request.as_bytes()).await.ok()?;
    let mut response = Vec::new();
    timeout(Duration::from_secs(2), stream.read_to_end(&mut response))
        .await
        .ok()?
        .ok()?;
    let text = String::from_utf8_lossy(&response);
    let (head, body) = text.split_once("\r\n\r\n")?;
    if !head.starts_with("HTTP/1.1 200") && !head.starts_with("HTTP/1.0 200") {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(body.trim()).ok()?;
    let id = value.get("id")?.as_str()?.trim().to_string();
    if id.is_empty() {
        return None;
    }
    let name = value
        .get("name")
        .and_then(|name| name.as_str())
        .unwrap_or_default()
        .to_string();
    let port = value
        .get("port")
        .and_then(|port| port.as_u64())
        .unwrap_or(0) as u16;
    Some((id, name, port))
}
