import { invoke } from "@tauri-apps/api/core";
import type {
  ChatMessage,
  Conversation,
  MenuState,
  SendOutcome,
  StagedPath,
  StorageRoots,
  TransferSnapshot,
} from "../types";

export function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export const api = {
  status: () => invoke<TransferSnapshot>("transfer_status"),
  start: (port?: number) =>
    invoke<TransferSnapshot>("transfer_start", { port: port ?? null }),
  stop: () => invoke<TransferSnapshot>("transfer_stop"),
  /** Android: re-assert the resident service after the app returns to view. */
  keepAlive: () => invoke<void>("transfer_keep_alive"),
  saveConfig: (config: {
    deviceName: string;
    storageDir: string;
    port: number;
    autoStart: boolean;
  }) => invoke<TransferSnapshot>("transfer_set_config", config),
  openDir: (path: string) => invoke<void>("transfer_open_dir", { path }),
  /** Android: the two anchors the save-location picker builds its choices from. */
  storageRoots: () => {
    // `import.meta.env.DEV` is folded away in the shipped bundle, so only the
    // browser preview of the phone layout sees these fixtures.
    if (import.meta.env.DEV && !inTauri())
      return Promise.resolve<StorageRoots>({
        sharedRoot: "/storage/emulated/0",
        privateDir: "/data/user/0/com.filedrop.app/FileDrop",
      });
    return invoke<StorageRoots>("transfer_storage_roots");
  },
  openFile: (path: string) => invoke<void>("transfer_open_file", { path }),
  send: (host: string, port: number, paths: string[]) =>
    invoke<number>("transfer_send", { host, port, paths }),

  conversations: () => invoke<Conversation[]>("chat_list"),
  messages: (peerId: string) => invoke<ChatMessage[]>("chat_messages", { peerId }),
  markRead: (peerId: string) => invoke<void>("chat_mark_read", { peerId }),
  ensurePeer: (peer: {
    id: string;
    name: string;
    host: string;
    port: number;
  }) => invoke<Conversation>("chat_ensure_peer", peer),
  addManual: (host: string, port: number) =>
    invoke<Conversation>("chat_add_manual", { host, port }),
  sendText: (peerId: string, text: string) =>
    invoke<ChatMessage>("chat_send_text", { peerId, text }),
  sendFiles: (peerId: string, paths: string[]) =>
    invoke<SendOutcome>("chat_send_files", { peerId, paths }),

  /** Files the Explorer verb handed over before the webview was ready. */
  sendTake: () => invoke<string[]>("send_take"),
  /** Files another app shared into FileDrop (Android share sheet). */
  takeShared: () => {
    if (import.meta.env.DEV && !inTauri()) return Promise.resolve<string[]>([]);
    return invoke<string[]>("transfer_take_shared");
  },
  /** Android: whether Doze lets this app keep networking with the screen off. */
  batteryState: () => {
    if (import.meta.env.DEV && !inTauri()) return Promise.resolve(true);
    return invoke<boolean>("transfer_battery_state");
  },
  batteryRequest: () => {
    if (import.meta.env.DEV && !inTauri()) return Promise.resolve(true);
    return invoke<boolean>("transfer_battery_request");
  },
  sendStat: (paths: string[]) => invoke<StagedPath[]>("send_stat", { paths }),
  setNotify: (notify: boolean) => invoke<TransferSnapshot>("transfer_set_notify", { notify }),
  menuState: () => invoke<MenuState>("shell_menu_state"),
  menuSet: (enabled: boolean) => invoke<MenuState>("shell_menu_set", { enabled }),
  openUrl: (url: string) => invoke<void>("open_url", { url }),
};

/**
 * Base64 kept under one webview message: the app writes it straight to disk, so
 * the queue never waits on a buffer the phone cannot hold twice.
 */
const STAGE_CHUNK = 512 * 1024;

function toBase64(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const comma = String(reader.result ?? "").indexOf(",");
      if (comma < 0) reject(new Error("读取文件失败"));
      else resolve(String(reader.result).slice(comma + 1));
    };
    reader.onerror = () => reject(new Error("读取文件失败"));
    reader.readAsDataURL(blob);
  });
}

/**
 * Copies a phone's file into the app's staging folder and hands back the path
 * the outbox can send from later — a queued send then survives closing the
 * chat, and even a restart, because the bytes are on disk, not in the webview.
 */
export async function stageFile(file: File): Promise<string> {
  const path = await invoke<string>("chat_stage_open", { name: file.name });
  let done = 0;
  try {
    while (done < file.size) {
      const next = Math.min(done + STAGE_CHUNK, file.size);
      const chunk = await toBase64(file.slice(done, next));
      await invoke("chat_stage_append", { path, chunk });
      done = next;
    }
  } catch (reason) {
    await invoke("chat_stage_cancel", { path }).catch(() => {});
    throw reason;
  }
  return path;
}
