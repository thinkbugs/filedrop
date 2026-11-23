import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { clockOf, dayLabel, extensionOf, formatBytes, percent } from "../lib/format";
import type { ChatMessage, Conversation, PendingFile } from "../types";
import Avatar from "./Avatar";
import {
  IconAlert,
  IconBack,
  IconCheck,
  IconClock,
  IconPaperclip,
  IconPlus,
  IconSend,
} from "./Icons";

interface Props {
  peer: Conversation;
  messages: ChatMessage[];
  pending: PendingFile[];
  progress: Record<string, { sent: number; total: number }>;
  sending: boolean;
  running: boolean;
  deviceName: string;
  narrow: boolean;
  onSendText: (text: string) => Promise<boolean>;
  onPickFiles: () => void;
  onOpenFile: (message: ChatMessage) => void;
  onBack: () => void;
  onToggleReceive: () => void;
}

type Line =
  | { key: string; at: number; day: string; kind: "day" }
  | { key: string; at: number; kind: "text"; message: ChatMessage }
  | { key: string; at: number; kind: "file"; message?: ChatMessage; file?: PendingFile };

function sameDay(a: number, b: number): boolean {
  return new Date(a).setHours(0, 0, 0, 0) === new Date(b).setHours(0, 0, 0, 0);
}

function buildLines(messages: ChatMessage[], pending: PendingFile[]): Line[] {
  const items: Line[] = [
    ...messages.map((message) =>
      message.kind === "file"
        ? {
            key: `file-${message.id}`,
            at: message.at,
            kind: "file" as const,
            message,
          }
        : { key: message.id, at: message.at, kind: "text" as const, message },
    ),
    ...pending.map((file) => ({
      key: `pending-${file.id}`,
      at: file.at,
      kind: "file" as const,
      file,
    })),
  ];
  items.sort((a, b) => a.at - b.at || a.key.localeCompare(b.key));

  const lines: Line[] = [];
  let previous = 0;
  for (const item of items) {
    if (!previous || !sameDay(previous, item.at)) {
      lines.push({
        key: `day-${item.at}`,
        at: item.at,
        day: dayLabel(item.at),
        kind: "day",
      });
    }
    lines.push(item);
    previous = item.at;
  }
  return lines;
}

function subtitle(peer: Conversation): string {
  if (peer.kind === "browser") {
    return peer.online
      ? "手机网页 · 在线，可收发文字"
      : "手机网页 · 已离开，重新打开传输页即可继续";
  }
  const endpoint = peer.host ? `${peer.host}:${peer.port}` : "地址未知";
  return `${peer.online ? "在线" : "离线"} · ${endpoint}`;
}

function FileBody({
  name,
  bytes,
  state,
  failed,
  queued,
  direction,
}: {
  name: string;
  bytes: number;
  state?: { sent: number; total: number };
  failed: boolean;
  queued: boolean;
  direction: "in" | "out";
}) {
  const total = state?.total || bytes;
  const moving = Boolean(state) && total > 0 && (state?.sent ?? 0) < total;
  const done = Boolean(state) && !moving;
  return (
    <div className="file-card">
      <span className="file-ext">{extensionOf(name)}</span>
      <span className="file-main">
        <span className="file-name" title={name}>
          {name}
        </span>
        <span className="file-meta">
          {failed ? (
            <em className="file-failed">
              <IconAlert size={11} /> 发送失败
            </em>
          ) : queued ? (
            <em className="file-queued">
              <IconClock size={11} /> 待对方上线
            </em>
          ) : moving ? (
            <>
              {formatBytes(state?.sent ?? 0)}
              {total > 0 ? ` / ${formatBytes(total)} · ${percent((state?.sent ?? 0) / total)}%` : ""}
            </>
          ) : done ? (
            <>
              {formatBytes(bytes)} · {direction === "out" ? "已发送" : "已接收"}
            </>
          ) : (
            formatBytes(bytes)
          )}
        </span>
        {moving && (
          <span className="bar">
            <i style={{ width: `${total > 0 ? percent((state?.sent ?? 0) / total) : 0}%` }} />
          </span>
        )}
      </span>
    </div>
  );
}

export default function ChatView({
  peer,
  messages,
  pending,
  progress,
  sending,
  running,
  deviceName,
  narrow,
  onSendText,
  onPickFiles,
  onOpenFile,
  onBack,
  onToggleReceive,
}: Props) {
  const [draft, setDraft] = useState("");
  const body = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLTextAreaElement>(null);

  const lines = buildLines(messages, pending);

  useLayoutEffect(() => {
    const node = body.current;
    if (node) node.scrollTop = node.scrollHeight;
  }, [peer.id, lines.length, draft]);

  useLayoutEffect(() => {
    const node = input.current;
    if (!node) return;
    node.style.height = "auto";
    node.style.height = `${Math.min(node.scrollHeight, 116)}px`;
  }, [draft]);

  useEffect(() => {
    setDraft("");
  }, [peer.id]);

  // The pane shrinks when the keyboard comes up. Whoever was reading the tail
  // has to stay there — the newest message would otherwise end up stranded in
  // the space the keyboard just took. Someone who scrolled back into history
  // is left exactly where they left themselves.
  const following = useRef(true);
  useEffect(() => {
    const node = body.current;
    if (!node) return;
    const view = window.visualViewport;
    following.current = true;
    const onScroll = () => {
      following.current = node.scrollHeight - node.scrollTop - node.clientHeight < 48;
    };
    const onResize = () => {
      if (following.current) node.scrollTop = node.scrollHeight;
    };
    node.addEventListener("scroll", onScroll);
    // A window that is resized for the keyboard fires here; a window the
    // keyboard only overlays fires on the visual viewport instead.
    window.addEventListener("resize", onResize);
    view?.addEventListener("resize", onResize);
    return () => {
      node.removeEventListener("scroll", onScroll);
      window.removeEventListener("resize", onResize);
      view?.removeEventListener("resize", onResize);
    };
  }, [peer.id]);

  async function submit() {
    const value = draft.trim();
    if (!value || sending) return;
    if (await onSendText(value)) setDraft("");
  }

  const canSendFiles = peer.kind === "device";

  return (
    <section className="chat">
      <header className="chat-head">
        {narrow && (
          <button className="icon-btn chat-back" onClick={onBack} title="返回设备列表">
            <IconBack />
          </button>
        )}
        <Avatar name={peer.name} kind={peer.kind} size={36} online={peer.online} />
        <div className="chat-id">
          <strong>{peer.name}</strong>
          <span>{subtitle(peer)}</span>
        </div>
        <div className="chat-actions">
          {narrow ? (
            <button
              className="icon-btn chat-plus"
              onClick={onPickFiles}
              disabled={!canSendFiles || sending}
              aria-label="发送文件"
              title={
                canSendFiles
                  ? "选择文件发送给这台设备"
                  : "网页设备暂时只能接收文字，文件请在手机上选择后发送"
              }
            >
              <IconPlus size={20} />
            </button>
          ) : (
            <button
              className="btn sm"
              onClick={onPickFiles}
              disabled={!canSendFiles || sending}
              title={
                canSendFiles
                  ? "选择文件发送给这台设备"
                  : "网页设备暂时只能接收文字，文件请在手机上选择后发送"
              }
            >
              <IconPaperclip size={14} />
              发送文件
            </button>
          )}
        </div>
      </header>

      {!running && (
        <div className="chat-note">
          <IconAlert size={13} />
          <span>接收已停止，对方发来的消息与文件不会到达</span>
          <button onClick={onToggleReceive}>立即开启</button>
        </div>
      )}

      <div className="chat-body" ref={body}>
        {lines.length === 0 && (
          <div className="chat-empty">
            <span className="chat-empty-mark">
              <IconSend size={20} />
            </span>
            <strong>开始和这台设备对话</strong>
            <span>
              输入文字直接发送，或把文件拖进来 · 本机 {deviceName}
            </span>
          </div>
        )}

        {lines.map((line) => {
          if (line.kind === "day") {
            return (
              <div className="day-sep" key={line.key}>
                {line.day}
              </div>
            );
          }

          if (line.kind === "file") {
            const message = line.message;
            const file = line.file;
            const direction = message ? message.direction : file!.direction;
            const name = message ? message.fileName : file!.name;
            const bytes = message ? message.bytes : file!.bytes;
            return (
              <div className={`msg ${direction}`} key={line.key}>
                <Avatar
                  name={direction === "out" ? deviceName : message?.peerName ?? file!.peerName}
                  kind={direction === "out" ? "device" : peer.kind}
                  size={30}
                  showDot={false}
                />
                <div className="msg-stack">
                  <button
                    className={`bubble file ${message ? "clickable" : ""} ${
                      message?.failed ? "failed" : ""
                    } ${message?.queued ? "queued" : ""}`}
                    onClick={() => message && onOpenFile(message)}
                    disabled={!message}
                    title={message?.filePath || undefined}
                  >
                    <FileBody
                      name={name}
                      bytes={bytes}
                      state={message ? progress[message.id] : undefined}
                      failed={Boolean(message?.failed)}
                      queued={Boolean(message?.queued)}
                      direction={direction}
                    />
                  </button>
                  <span className="msg-time">{clockOf(line.at)}</span>
                </div>
              </div>
            );
          }

          const message = line.message;
          return (
            <div className={`msg ${message.direction}`} key={line.key}>
              <Avatar
                name={
                  message.direction === "out"
                    ? deviceName
                    : message.peerName || peer.name
                }
                kind={message.direction === "out" ? "device" : peer.kind}
                size={30}
                showDot={false}
              />
              <div className="msg-stack">
                <p className={`bubble ${message.failed ? "failed" : ""} ${message.queued ? "queued" : ""}`}>
                  {message.text}
                </p>
                <span className="msg-time">
                  {message.failed ? (
                    <em className="file-failed">
                      <IconAlert size={11} /> 未送达
                    </em>
                  ) : message.queued ? (
                    <em className="file-queued">
                      <IconClock size={11} /> 待对方上线
                    </em>
                  ) : message.direction === "out" ? (
                    <IconCheck size={11} />
                  ) : null}
                  {clockOf(message.at)}
                </span>
              </div>
            </div>
          );
        })}
      </div>

      {peer.queued > 0 && (
        <div className="chat-queue">
          <IconClock size={13} />
          <span>
            {peer.queued} 条内容待 {peer.name} 上线后自动发送，可以离开这一页。
          </span>
        </div>
      )}

      <footer className="composer">
        <div className="composer-box">
          <textarea
            ref={input}
            rows={1}
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key !== "Enter" || event.shiftKey) return;
              // Chinese input methods still hold the composition when Enter is
              // pressed to pick a candidate; that Enter must not send.
              if (event.nativeEvent.isComposing || event.keyCode === 229) return;
              event.preventDefault();
              void submit();
            }}
            placeholder={`发消息给 ${peer.name}`}
          />
          <div className="composer-tools">
            <button
              className="icon-btn"
              onClick={onPickFiles}
              disabled={!canSendFiles || sending}
              title={
                canSendFiles
                  ? "发送文件"
                  : "网页设备暂时只能接收文字，文件请在手机上选择后发送"
              }
            >
              <IconPaperclip />
            </button>
            <span className="composer-hint">
              {canSendFiles
                ? "Enter 发送 · Shift + Enter 换行 · 也可直接拖入文件"
                : "网页设备：文件请在这台手机上选择后发送"}
            </span>
            <button
              className="composer-send"
              onClick={() => void submit()}
              disabled={!draft.trim() || sending}
            >
              发送
            </button>
          </div>
        </div>
      </footer>
    </section>
  );
}
