import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import ChatView from "./components/ChatView";
import DeviceList from "./components/DeviceList";
import DiscoverPane from "./components/DiscoverPane";
import Rail, { type Tab } from "./components/Rail";
import ReceivePane from "./components/ReceivePane";
import SendToSheet from "./components/SendToSheet";
import SettingsSheet from "./components/SettingsSheet";
import {
  DropMark,
  IconAlert,
  IconRadar,
  IconWifi,
  IconX,
} from "./components/Icons";
import { api, inTauri, stageFile } from "./lib/bridge";
import type {
  ChatMessage,
  Conversation,
  IncomingProgress,
  PendingFile,
  Peer,
  ReceivedEvent,
  ReceivedFile,
  SendOutcome,
  TransferSnapshot,
  TransferStatus,
} from "./types";

const POLL_MS = 3000;
const NARROW_AT = 820;
const MOBILE_UA = /Android|iPhone|iPad|iPod/i.test(navigator.userAgent);

function messageOf(reason: unknown): string {
  if (reason instanceof Error) return reason.message;
  if (typeof reason === "string") return reason;
  return String(reason);
}

/** One line for a send that went out and one for a send the queue is holding. */
function outcomeToast({ sent, queued }: SendOutcome): string {
  if (queued > 0) {
    return sent > 0
      ? `已发送 ${sent} 个文件，${queued} 个已排队，会自动重试`
      : `${queued} 个文件已排队，会自动重试`;
  }
  return `已发送 ${sent} 个文件`;
}

export default function App() {
  const [status, setStatus] = useState<TransferStatus | null>(null);
  const [received, setReceived] = useState<ReceivedFile[]>([]);
  const [conversations, setConversations] = useState<Conversation[]>([]);
  const [messages, setMessages] = useState<Record<string, ChatMessage[]>>({});
  const [pending, setPending] = useState<PendingFile[]>([]);
  const [progress, setProgress] = useState<
    Record<string, { sent: number; total: number }>
  >({});
  const [incoming, setIncoming] = useState<IncomingProgress[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [tab, setTab] = useState<Tab>("chats");
  const [chatOpen, setChatOpen] = useState(false);
  const [narrow, setNarrow] = useState(
    typeof window === "undefined" ? false : window.innerWidth < NARROW_AT,
  );
  const [query, setQuery] = useState("");
  const [toast, setToast] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<"server" | "send" | null>(null);
  const [scanning, setScanning] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  // Files handed over by Explorer's `FileDrop 传送` verb. Kept as paths so the
  // sheet can re-stat them (deleted between the click and the render) and pass
  // them straight to `chat_send_files`, which reads them from disk.
  const [sendToPaths, setSendToPaths] = useState<string[]>([]);

  const activeRef = useRef<string | null>(null);
  const fileInput = useRef<HTMLInputElement>(null);

  const mobile = status?.mobile ?? MOBILE_UA;
  const active = useMemo(
    () => conversations.find((peer) => peer.id === activeId) ?? null,
    [conversations, activeId],
  );

  const run = useCallback(async () => {
    if (!inTauri()) return;
    try {
      const [snapshot, list]: [TransferSnapshot, Conversation[]] =
        await Promise.all([api.status(), api.conversations()]);
      setStatus(snapshot.status);
      setReceived(snapshot.received);
      setConversations(list);
      setActiveId((current) =>
        current && list.some((peer) => peer.id === current) ? current : null,
      );
    } catch {
      // One failed poll; the next one retries.
    }
  }, []);

  const syncPeer = useCallback(async (peerId: string) => {
    if (!inTauri()) return;
    try {
      const list = await api.messages(peerId);
      setMessages((current) => ({ ...current, [peerId]: list }));
      setPending((current) =>
        current.filter((file) => !list.some((message) => message.id === file.id)),
      );
      // A recorded file is finished with, but its last progress event stays in
      // the map forever otherwise — that is the card that keeps showing "93%"
      // after the file has arrived.
      setProgress((current) => {
        let next = current;
        for (const message of list) {
          if (next[message.id]) {
            if (next === current) next = { ...current };
            delete next[message.id];
          }
        }
        return next;
      });
    } catch {
      // The store is authoritative on the next open.
    }
  }, []);

  useEffect(() => {
    void run();
    const id = setInterval(() => void run(), POLL_MS);
    return () => clearInterval(id);
  }, [run]);

  // Dev-only browser preview: outside Tauri there is no backend, so seed the
  // phone layout with fixtures to review each page at phone size. The shipped
  // app always runs inside Tauri (`inTauri()`), so this branch never fires and
  // the mock stays out of the production bundle (dynamic import).
  useEffect(() => {
    if (inTauri() || !import.meta.env.DEV) return;
    void import("./lib/mock").then((fixtures) => {
      setStatus(fixtures.mockStatus());
      setConversations(fixtures.mockConversations());
      setMessages(fixtures.mockMessages());
      setReceived(fixtures.mockReceived());
    });
  }, []);

  useEffect(() => {
    const onResize = () => setNarrow(window.innerWidth < NARROW_AT);
    onResize();
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

  /**
   * The part of the page the on-screen keyboard covers, as a CSS variable.
   * Android resizes the webview itself for the IME (see MainActivity), and then
   * this reads 0; it only does work on a webview that lets the keyboard overlay
   * the page instead of shrinking it.
   */
  useEffect(() => {
    const view = window.visualViewport;
    if (!view) return;
    const onViewport = () => {
      const covered = Math.max(0, window.innerHeight - view.height - view.offsetTop);
      document.documentElement.style.setProperty("--keyboard", `${Math.round(covered)}px`);
    };
    onViewport();
    view.addEventListener("resize", onViewport);
    return () => view.removeEventListener("resize", onViewport);
  }, []);

  // Android can end the resident service while the app is away. Coming back
  // re-asserts it, so the device is discoverable again without a restart.
  useEffect(() => {
    if (!inTauri()) return;
    const onVisible = () => {
      if (document.visibilityState === "visible") void api.keepAlive().catch(() => {});
    };
    document.addEventListener("visibilitychange", onVisible);
    return () => document.removeEventListener("visibilitychange", onVisible);
  }, []);

  useEffect(() => {
    if (!inTauri()) return;
    let disposed = false;
    const unlisteners: UnlistenFn[] = [];
    const track = (listener: Promise<UnlistenFn>) => {
      listener.then((unlisten) => {
        if (disposed) unlisten();
        else unlisteners.push(unlisten);
      });
    };

    // The Rust side owns the transcript, so every event simply re-reads the
    // affected conversation instead of mirroring the store in two places.
    track(
      listen<{ peerId?: string }>("notification-open", (event) => {
        const peerId = event.payload?.peerId;
        if (!peerId) return;
        // A toast was clicked: land on the conversation it was about, not just
        // on whatever the window was showing.
        setSettingsOpen(false);
        setTab("chats");
        setActiveId(peerId);
        setChatOpen(true);
        void syncPeer(peerId);
      }),
    );

    track(
      listen<{ message?: { peerId?: string }; peer?: { id?: string } }>(
        "chat-message",
        (event) => {
          const peerId = event.payload?.message?.peerId ?? event.payload?.peer?.id;
          if (peerId && peerId === activeRef.current) {
            void syncPeer(peerId);
            // The conversation is on screen, so an unread count would lie.
            void api.markRead(peerId).catch(() => undefined);
          }
          void run();
        },
      ),
    );

    track(
      listen<PendingFile>("chat-file-start", (event) => {
        const file = event.payload;
        setPending((current) => [
          ...current.filter((item) => item.id !== file.id),
          { ...file, received: 0, total: file.bytes },
        ]);
        if (file.peerId !== activeRef.current) void run();
      }),
    );

    track(
      listen<{ id: string; received: number; total: number }>(
        "chat-file-progress",
        (event) => {
          const { id, received: sent, total } = event.payload;
          setProgress((current) => ({ ...current, [id]: { sent, total } }));
          setPending((current) =>
            current.map((file) =>
              file.id === id ? { ...file, received: sent, total } : file,
            ),
          );
        },
      ),
    );

    track(
      listen<{ id: string }>("chat-file-cancelled", (event) => {
        setPending((current) => current.filter((file) => file.id !== event.payload.id));
      }),
    );

    track(
      listen<IncomingProgress>("transfer-progress", (event) => {
        setIncoming((current) => [
          ...current.filter((item) => item.name !== event.payload.name),
          event.payload,
        ]);
      }),
    );

    track(
      listen<ReceivedEvent>("transfer-file", (event) => {
        setIncoming((current) =>
          current.filter((item) => item.name !== event.payload.name),
        );
        setReceived((current) =>
          [
            {
              name: event.payload.name,
              path: event.payload.path,
              bytes: event.payload.bytes,
              at: Date.now(),
            },
            ...current.filter((file) => file.path !== event.payload.path),
          ].slice(0, 100),
        );
        setToast(`已收到 ${event.payload.name}`);
      }),
    );

    // A second FileDrop process (Explorer right-click on a running instance)
    // hands its file list to this one; the queue already holds them, so the
    // listener only needs to open the sheet.
    track(
      listen<{ paths: string[] }>("send-to-request", (event) => {
        const incomingPaths = event.payload?.paths ?? [];
        if (!incomingPaths.length) return;
        setSendToPaths((current) => Array.from(new Set([...current, ...incomingPaths])));
      }),
    );

    return () => {
      disposed = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [run, syncPeer]);

  // Cold start: the files arrived before this webview could subscribe, so the
  // Rust side parked them in a queue. Drain once, on mount.
  useEffect(() => {
    if (!inTauri()) return;
    void api
      .sendTake()
      .then((queued) => {
        if (queued.length) setSendToPaths((current) => Array.from(new Set([...current, ...queued])));
      })
      .catch(() => undefined);
  }, []);

  useEffect(() => {
    if (!toast) return;
    const id = setTimeout(() => setToast(null), 3200);
    return () => clearTimeout(id);
  }, [toast]);

  // Android share sheet: MainActivity copies the shared URIs into the app's
  // share inbox and then calls `__filedropShared`; a cold start drains the same
  // inbox on mount. Both land in the sheet the Explorer verb already uses.
  const takeShared = useCallback(async () => {
    if (!inTauri()) return;
    try {
      const shared = await api.takeShared();
      if (shared.length) {
        setSendToPaths((current) => Array.from(new Set([...current, ...shared])));
      }
    } catch {
      // The inbox still holds the files; the next share or start retries.
    }
  }, []);

  useEffect(() => {
    void takeShared();
    const bridge = window as Window & { __filedropShared?: () => boolean };
    bridge.__filedropShared = () => {
      void takeShared();
      return true;
    };
    // Sharing from another app leaves it in the background, so coming back is
    // the moment the inbox is worth a look even if the native call was missed.
    const onVisible = () => {
      if (document.visibilityState === "visible") void takeShared();
    };
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      document.removeEventListener("visibilitychange", onVisible);
      delete bridge.__filedropShared;
    };
  }, [takeShared]);

  // The gesture/hardware back button asks the page before the system takes the
  // user out of FileDrop: close whatever is on top, and only when nothing is
  // open let MainActivity fall through to closing the window.
  useEffect(() => {
    const bridge = window as Window & { __filedropBack?: () => boolean };
    bridge.__filedropBack = () => {
      if (settingsOpen) {
        setSettingsOpen(false);
        return true;
      }
      if (sendToPaths.length > 0) {
        setSendToPaths([]);
        return true;
      }
      if (chatOpen && active) {
        setChatOpen(false);
        return true;
      }
      if (tab !== "chats") {
        setTab("chats");
        return true;
      }
      return false;
    };
    return () => {
      delete bridge.__filedropBack;
    };
  }, [settingsOpen, chatOpen, active, tab, sendToPaths.length]);

  useEffect(() => {
    if (!error) return;
    const id = setTimeout(() => setError(null), 6000);
    return () => clearTimeout(id);
  }, [error]);

  async function openConversation(peer: Conversation) {
    activeRef.current = peer.id;
    setActiveId(peer.id);
    setChatOpen(true);
    setTab("chats");
    await syncPeer(peer.id);
    void api.markRead(peer.id).catch(() => undefined);
    setConversations((current) =>
      current.map((item) => (item.id === peer.id ? { ...item, unread: 0 } : item)),
    );
  }

  async function openDiscovered(peer: Peer) {
    try {
      const conversation = await api.ensurePeer({
        id: peer.id,
        name: peer.name,
        host: peer.host,
        port: peer.port,
      });
      await openConversation(conversation);
      void run();
    } catch (reason) {
      setError(messageOf(reason));
    }
  }

  async function addManual(host: string, port: number): Promise<boolean> {
    setScanning(true);
    try {
      const conversation = await api.addManual(host, port);
      await openConversation(conversation);
      void run();
      return true;
    } catch (reason) {
      setError(messageOf(reason));
      return false;
    } finally {
      setScanning(false);
    }
  }

  async function sendText(text: string): Promise<boolean> {
    const peerId = activeId;
    if (!peerId) return false;
    setBusy("send");
    try {
      const message = await api.sendText(peerId, text);
      await syncPeer(peerId);
      void run();
      // The send already "finished" as far as the composer is concerned; say so,
      // otherwise a quiet bubble reads like nothing happened.
      if (message.queued) setToast("对方当前离线，已排队，上线后自动发送");
      return true;
    } catch (reason) {
      setError(messageOf(reason));
      return false;
    } finally {
      setBusy(null);
    }
  }

  /** Desktop: the Rust sender reads the paths, so the dialog hands back files. */
  async function sendPaths(paths: string[]) {
    const peerId = activeId;
    if (!peerId || !paths.length) return;
    setBusy("send");
    setError(null);
    try {
      const outcome = await api.sendFiles(peerId, paths);
      await syncPeer(peerId);
      setToast(outcomeToast(outcome));
    } catch (reason) {
      setError(`发送失败：${messageOf(reason)}`);
    } finally {
      setBusy(null);
      void run();
    }
  }

  /**
   * Mobile: the picker hands the webview a `File`, not a path, so the bytes are
   * copied into the app's staging folder and given to the same Rust sender the
   * desktop uses. Online it goes now; away, it queues. Neither case puts a
   * socket in the webview — Android blocks a page from `https://tauri.localhost`
   * calling plain `http://<peer>:<port>`, so a browser-side upload could never
   * have reached another device.
   */
  async function sendMobileFiles(files: File[]) {
    const peer = active;
    if (!peer || !status) return;
    if (!peer.host || !peer.port) {
      setError("还没有这台设备的地址，等它重新出现或手动添加");
      return;
    }
    setBusy("send");
    setError(null);
    try {
      const paths: string[] = [];
      for (const file of files) {
        paths.push(await stageFile(file));
      }
      const outcome = await api.sendFiles(peer.id, paths);
      await syncPeer(peer.id);
      setToast(outcomeToast(outcome));
    } catch (reason) {
      setError(`发送失败：${messageOf(reason)}`);
    } finally {
      setBusy(null);
      void run();
    }
  }

  function pickFiles() {
    const peer = active;
    if (!peer) return;
    if (peer.kind !== "device") {
      setError("网页设备暂时只能接收文字；文件请在该手机上选择后发送");
      return;
    }
    if (mobile) {
      fileInput.current?.click();
      return;
    }
    void (async () => {
      try {
        const picked = await open({
          multiple: true,
          directory: false,
          title: `选择发送给 ${peer.name} 的文件`,
        });
        if (!picked) return;
        await sendPaths(Array.isArray(picked) ? picked : [picked]);
      } catch (reason) {
        setError(`选择文件失败：${messageOf(reason)}`);
      }
    })();
  }

  function onFilesPicked(event: React.ChangeEvent<HTMLInputElement>) {
    const files = Array.from(event.target.files ?? []);
    event.target.value = "";
    if (files.length) void sendMobileFiles(files);
  }

  async function toggleServer() {
    if (!status) return;
    const wasRunning = status.running;
    setBusy("server");
    setError(null);
    try {
      const snapshot = await (wasRunning ? api.stop() : api.start());
      setStatus(snapshot.status);
      setReceived(snapshot.received);
      setConversations(await api.conversations().catch(() => conversations));
      setToast(wasRunning ? "接收已停止" : "接收已开启");
    } catch (reason) {
      setError(`${wasRunning ? "停止" : "开启"}接收失败：${messageOf(reason)}`);
    } finally {
      setBusy(null);
    }
  }

  async function copyUrl() {
    if (!status?.url) return;
    try {
      await navigator.clipboard.writeText(status.url);
      setToast("接收地址已复制");
    } catch {
      setError("复制失败，请手动选中地址复制");
    }
  }

  async function openStorage() {
    if (!status) return;
    try {
      await api.openDir(status.storageDir);
    } catch (reason) {
      setError(messageOf(reason));
    }
  }

  async function openFile(file: ReceivedFile | ChatMessage) {
    const path = "path" in file ? file.path : file.filePath;
    if (!path) {
      setToast("文件在对方设备上");
      return;
    }
    try {
      await api.openFile(path);
    } catch (reason) {
      setError(messageOf(reason));
    }
  }

  // Native drag and drop hands the webview real paths, which is exactly what
  // the sender needs; dropping into a conversation sends it there.
  useEffect(() => {
    if (!inTauri()) return;
    let disposed = false;
    let unlisten: UnlistenFn | undefined;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "drop") {
          const peer = conversations.find((item) => item.id === activeRef.current);
          if (!peer) {
            setError("先打开一个对话，再把文件拖进来");
            return;
          }
          if (peer.kind !== "device") {
            setError("网页设备暂时只能接收文字；文件请在该手机上选择后发送");
            return;
          }
          void sendPaths(event.payload.paths);
        }
      })
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
      unlisten?.();
    };
    // `sendPaths` reads the live active id, so one listener is enough.
  }, [conversations]);

  if (!status) {
    return (
      <div className="app boot-shell">
        <div className="boot">
          <span className="brand-mark">
            <DropMark size={28} />
          </span>
          <h1>FileDrop</h1>
          <p>{inTauri() ? "正在启动接收服务…" : "请在 FileDrop 桌面应用中打开"}</p>
        </div>
      </div>
    );
  }

  const needle = query.trim().toLowerCase();
  const visible = needle
    ? conversations.filter((peer) =>
        [peer.name, peer.host, peer.preview]
          .join(" ")
          .toLowerCase()
          .includes(needle),
      )
    : conversations;

  const unread = conversations.reduce((sum, peer) => sum + peer.unread, 0);
  const chatted: Record<string, boolean> = {};
  for (const peer of conversations) {
    if (peer.kind === "device") chatted[peer.id] = true;
  }

  const showDetail = narrow && (tab !== "chats" || Boolean(active && chatOpen));
  // The 我的 page keeps the tab bar visible, so a chat left open must not hide it.
  const showChat = narrow && !settingsOpen && tab === "chats" && Boolean(active && chatOpen);
  const goDiscover = () => {
    setTab("discover");
    setScanning(true);
    void run();
    setTimeout(() => setScanning(false), 2500);
  };

  return (
    <div
      className={`app${narrow ? " is-narrow" : ""}${showDetail ? " show-detail" : ""}${showChat ? " show-chat" : ""}`}
      data-platform={status.platform}
    >
      <Rail
        tab={tab}
        unread={unread}
        running={status.running}
        deviceName={status.deviceName}
        mobile={narrow}
        meActive={settingsOpen}
        onSelect={(next) => {
          setSettingsOpen(false);
          if (next === "chats") {
            setTab("chats");
            setChatOpen(false);
          } else {
            setTab(next);
          }
          if (next === "discover") goDiscover();
        }}
        onSettings={() => setSettingsOpen(true)}
      />

      <DeviceList
        conversations={visible}
        total={conversations.length}
        query={query}
        activeId={activeId}
        running={status.running}
        port={status.port}
        lanIp={status.lanIp}
        deviceName={status.deviceName}
        onQuery={setQuery}
        onSelect={(peer) => void openConversation(peer)}
        onSettings={() => setSettingsOpen(true)}
        onDiscover={goDiscover}
      />

      <main className="content">
        {status.lastError && (
          <div className="banner warn">
            <IconWifi size={15} />
            <span>{status.lastError}</span>
          </div>
        )}
        {error && (
          <div className="banner danger">
            <IconAlert size={15} />
            <span>{error}</span>
            <button
              className="banner-close"
              onClick={() => setError(null)}
              aria-label="关闭提示"
            >
              <IconX size={13} />
            </button>
          </div>
        )}

        {tab === "chats" ? (
          active ? (
            <ChatView
              peer={active}
              messages={messages[active.id] ?? []}
              pending={pending.filter((file) => file.peerId === active.id)}
              progress={progress}
              sending={busy === "send"}
              running={status.running}
              deviceName={status.deviceName}
              narrow={narrow}
              onSendText={sendText}
              onPickFiles={pickFiles}
              onOpenFile={(message) => void openFile(message)}
              onBack={() => setChatOpen(false)}
              onToggleReceive={() => void toggleServer()}
            />
          ) : (
            <section className="pane placeholder">
              <div className="placeholder-inner">
                <span className="placeholder-mark">
                  <DropMark size={30} />
                </span>
                <strong>FileDrop</strong>
                <p>
                  左边选择一台设备，就能发消息、传文件。
                  <br />
                  也可以在「发现」里点一台新设备开始对话。
                </p>
                <button className="btn sm" onClick={goDiscover}>
                  <IconRadar size={14} />
                  发现设备
                </button>
              </div>
            </section>
          )
        ) : tab === "discover" ? (
          <DiscoverPane
            peers={status.peers}
            chatted={chatted}
            scanning={scanning || status.running}
            narrow={false}
            onOpen={(peer) => void openDiscovered(peer)}
            onAdd={addManual}
            onRefresh={goDiscover}
            onGoReceive={() => setTab("receive")}
            onBack={() => setTab("chats")}
          />
        ) : (
          <ReceivePane
            status={status}
            received={received}
            incoming={incoming}
            busy={busy === "server"}
            narrow={false}
            onToggle={() => void toggleServer()}
            onCopy={() => void copyUrl()}
            onOpenDir={() => void openStorage()}
            onOpenFile={(file) => void openFile(file)}
            onBack={() => setTab("chats")}
          />
        )}
      </main>

      {settingsOpen && (
        <SettingsSheet
          status={status}
          onClose={() => setSettingsOpen(false)}
          onSaved={(snapshot) => {
            setStatus(snapshot.status);
            setReceived(snapshot.received);
            void run();
          }}
        />
      )}

      {sendToPaths.length > 0 && (
        <SendToSheet
          paths={sendToPaths}
          conversations={conversations}
          onClose={() => setSendToPaths([])}
          onSent={(count, queued, peerName) => {
            setSendToPaths([]);
            setToast(
              queued > 0
                ? `${queued} 个文件已排队，${peerName} 上线后自动发送`
                : `已发送 ${count} 个文件给 ${peerName}`,
            );
            void run();
          }}
          onError={(message) => setError(message)}
        />
      )}

      {toast && <div className="toast">{toast}</div>}

      <input ref={fileInput} type="file" multiple hidden onChange={onFilesPicked} />
    </div>
  );
}
