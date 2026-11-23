import { useEffect, useMemo, useState } from "react";
import { api } from "../lib/bridge";
import { extensionOf, formatBytes } from "../lib/format";
import type { Conversation, StagedPath } from "../types";
import Avatar from "./Avatar";
import { IconAlert, IconX } from "./Icons";

interface Props {
  paths: string[];
  conversations: Conversation[];
  onClose: () => void;
  onSent: (sent: number, queued: number, peerName: string) => void;
  onError: (message: string) => void;
}

// The verb is used to hand off a small batch to a known peer; the last five
// conversations cover almost every real send without needing a search.
const RECIPIENTS = 5;

function messageOf(reason: unknown): string {
  if (reason instanceof Error) return reason.message;
  return String(reason);
}

export default function SendToSheet({ paths, conversations, onClose, onSent, onError }: Props) {
  const [files, setFiles] = useState<StagedPath[]>([]);
  const [busy, setBusy] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void api
      .sendStat(paths)
      .then((list) => {
        if (!cancelled) setFiles(list);
      })
      .catch((reason) => onError(`读取文件失败：${messageOf(reason)}`));
    return () => {
      cancelled = true;
    };
  }, [paths, onError]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  // The right-click send is aimed at people the user talks to; the request is
  // the five most recent device conversations, online or not. Browser peers are
  // excluded because the file path in `chat_send_files` rejects them outright.
  const recipients = useMemo(
    () =>
      conversations
        .filter((peer) => peer.kind === "device")
        .sort((a, b) => b.lastAt - a.lastAt)
        .slice(0, RECIPIENTS),
    [conversations],
  );

  async function sendTo(peer: Conversation) {
    setBusy(peer.id);
    try {
      const { sent, queued } = await api.sendFiles(peer.id, paths);
      onSent(sent, queued, peer.name);
    } catch (reason) {
      onError(`发送失败：${messageOf(reason)}`);
    } finally {
      setBusy(null);
    }
  }

  return (
    <div
      className="sheet-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <aside
        className="sheet send-to"
        role="dialog"
        aria-modal="true"
        aria-label="FileDrop 传送"
      >
        <header className="sheet-head">
          <div>
            <h2>FileDrop 传送</h2>
            <p>
              {paths.length} 个文件 · 从最近 {RECIPIENTS} 台设备中选择发送
            </p>
          </div>
          <button className="icon-btn" onClick={onClose} aria-label="关闭">
            <IconX size={16} />
          </button>
        </header>

        <div className="sheet-body">
          <section className="send-to-files">
            {files.map((file) => (
              <div className="send-to-file" key={file.path}>
                <span className="send-to-ext">{extensionOf(file.name) || "文件"}</span>
                <div className="send-to-file-meta">
                  <strong title={file.name}>{file.name}</strong>
                  <small>{file.bytes > 0 ? formatBytes(file.bytes) : "文件已不在原位置"}</small>
                </div>
              </div>
            ))}
          </section>

          <section className="send-to-recipients">
            <h3>最近对话设备</h3>
            {recipients.length === 0 ? (
              <div className="send-to-empty">
                <IconAlert size={14} />
                <span>还没有对话过的设备，先在 FileDrop 主界面里连上一台。</span>
              </div>
            ) : (
              <ul>
                {recipients.map((peer) => (
                  <li key={peer.id}>
                    <button
                      type="button"
                      className="send-to-row"
                      onClick={() => void sendTo(peer)}
                      disabled={busy !== null || !peer.online}
                      title={peer.online ? "" : "该设备当前不在线，先让它出现在局域网里再发送"}
                    >
                      <Avatar
                        name={peer.name}
                        kind={peer.kind}
                        size={36}
                        online={peer.online}
                      />
                      <div className="send-to-meta">
                        <strong>{peer.name}</strong>
                        <small>
                          {peer.online ? peer.host : `${peer.host} · 离线`}
                        </small>
                      </div>
                      <span className="send-to-action">
                        {busy === peer.id
                          ? "发送中…"
                          : peer.online
                            ? "发送"
                            : "离线"}
                      </span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </section>
        </div>

        <footer className="sheet-foot">
          <small className="sheet-note">两台设备需要在同一局域网内。</small>
          <button className="btn ghost" onClick={onClose} disabled={busy !== null}>
            取消
          </button>
        </footer>
      </aside>
    </div>
  );
}
