/** Wire types for the transfer commands, as the Rust side serialises them. */

export interface Peer {
  id: string;
  name: string;
  host: string;
  port: number;
}

export interface ReceivedFile {
  name: string;
  path: string;
  bytes: number;
  /** Milliseconds since the Unix epoch. */
  at: number;
}

export interface TransferStatus {
  running: boolean;
  url: string | null;
  /** data: URL of the QR code for `url`. */
  qr: string | null;
  deviceName: string;
  storageDir: string;
  port: number;
  /** This instance's stable id; the identity other devices file chats under. */
  instanceId: string;
  peers: Peer[];
  autoStart: boolean;
  /** Toast on an inbound file or message while the window is hidden. */
  notify: boolean;
  mobile: boolean;
  platform: string;
  lanIp: string;
  version: string;
  lastError: string | null;
}

/** A path Explorer handed to `FileDrop 传送`, paired with its size. */
export interface StagedPath {
  name: string;
  path: string;
  bytes: number;
}

/** What HKCU says about the Explorer verb. */
export interface MenuState {
  supported: boolean;
  installed: boolean;
  command: string;
}

/** The two anchors the Android save-location picker builds its choices from. */
export interface StorageRoots {
  sharedRoot: string;
  privateDir: string;
}

export interface TransferSnapshot {
  status: TransferStatus;
  received: ReceivedFile[];
}

/** A file arriving over the relay or a multipart upload. */
export interface IncomingProgress {
  name: string;
  received: number;
  total: number;
  progress: number;
}

export interface SendProgress {
  target: string;
  name: string;
  index: number;
  count: number;
  sent: number;
  total: number;
  progress: number;
}

export interface ReceivedEvent {
  name: string;
  path: string;
  bytes: number;
}

export interface StagedFile {
  id: string;
  name: string;
  size: number;
  /** Desktop: an absolute path the Rust sender can read. */
  path?: string;
  /** Mobile: the File the webview picked. */
  file?: File;
}

export interface SendTarget {
  name: string;
  host: string;
  port: number;
}

/** `device` = another FileDrop app, `browser` = the phone transfer page. */
export type PeerKind = "device" | "browser";

export interface ChatMessage {
  id: string;
  peerId: string;
  peerName: string;
  direction: "in" | "out";
  kind: "text" | "file";
  text: string;
  fileName: string;
  /** Desktop: an absolute path `transfer_open_file` can open. */
  filePath: string;
  bytes: number;
  at: number;
  failed: boolean;
  /** `out` still in the outbox; it goes automatically when the peer returns. */
  queued: boolean;
}

export interface Conversation {
  id: string;
  name: string;
  host: string;
  port: number;
  kind: PeerKind;
  lastAt: number;
  preview: string;
  lastKind: string;
  unread: number;
  online: boolean;
  /** Sends waiting for this device to come back. */
  queued: number;
}

export interface ChatEvent {
  message: ChatMessage;
  peer: Conversation | null;
}

/** What `chat_send_files` did with the paths it was handed. */
export interface SendOutcome {
  sent: number;
  queued: number;
}

/** A transfer in flight; it has no stored message until the last byte lands. */
export interface PendingFile {
  id: string;
  peerId: string;
  peerName: string;
  name: string;
  bytes: number;
  received: number;
  total: number;
  direction: "in" | "out";
  at: number;
}
