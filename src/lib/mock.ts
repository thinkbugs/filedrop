import type {
  ChatMessage,
  Conversation,
  Peer,
  ReceivedFile,
  TransferStatus,
} from "../types";

// Dev-only fixtures so the phone layout can be previewed in a plain browser at
// phone size (`npm run dev`). The shipped app always runs inside Tauri, so this
// never executes there — App only seeds it when `!inTauri() && DEV`.

const now = Date.now();
const minutes = (n: number) => now - n * 60_000;
const hours = (n: number) => now - n * 3_600_000;

export function mockStatus(): TransferStatus {
  return {
    running: true,
    url: "http://192.168.0.37:8787",
    qr: null,
    deviceName: "我的手机",
    storageDir: "/data/user/0/com.filedrop.app/FileDrop",
    port: 8787,
    instanceId: "95caa635-7af6-4b61-8949-1dc2367f7632",
    peers: mockPeers(),
    autoStart: true,
    notify: true,
    mobile: true,
    platform: "android",
    lanIp: "192.168.0.37",
    version: "1.0.0",
    lastError: null,
  };
}

export function mockPeers(): Peer[] {
  return [
    { id: "peer-xbooks", name: "X-BOOKS", host: "192.168.0.27", port: 8787 },
    { id: "peer-living", name: "客厅的电脑", host: "192.168.0.15", port: 8787 },
    { id: "peer-tv", name: "卧室小米电视", host: "192.168.0.44", port: 8787 },
  ];
}

export function mockConversations(): Conversation[] {
  return [
    {
      id: "peer-xbooks",
      name: "X-BOOKS",
      host: "192.168.0.27",
      port: 8787,
      kind: "device",
      lastAt: minutes(2),
      preview: "会议纪要.pdf",
      lastKind: "file",
      unread: 3,
      online: true,
      queued: 0,
    },
    {
      id: "ws:192.168.0.51",
      name: "手机网页 · 访客",
      host: "192.168.0.51",
      port: 0,
      kind: "browser",
      lastAt: minutes(26),
      preview: "在吗？帮我发一下刚才的照片",
      lastKind: "text",
      unread: 1,
      online: true,
      queued: 0,
    },
    {
      id: "peer-living",
      name: "客厅的电脑",
      host: "192.168.0.15",
      port: 8787,
      kind: "device",
      lastAt: hours(5),
      preview: "已经收到，谢谢！",
      lastKind: "text",
      unread: 0,
      online: false,
      queued: 2,
    },
  ];
}

export function mockMessages(): Record<string, ChatMessage[]> {
  const xbooks: ChatMessage[] = [
    msg("m1", "peer-xbooks", "X-BOOKS", "in", "text", "晚上八点把方案发我一份", hours(6)),
    msg("m2", "peer-xbooks", "我的手机", "out", "text", "好的，稍等", hours(6) + 40_000),
    fileMsg("m3", "peer-xbooks", "X-BOOKS", "in", "季度报告.xlsx", 2_480_000, hours(5)),
    fileMsg("m4", "peer-xbooks", "我的手机", "out", "设计稿终版.zip", 18_400_000, hours(4)),
    msg("m5", "peer-xbooks", "X-BOOKS", "in", "text", "收到，看着没问题", minutes(30)),
    fileMsg("m6", "peer-xbooks", "X-BOOKS", "in", "会议纪要.pdf", 842_000, minutes(2)),
  ];
  const guest: ChatMessage[] = [
    msg("g1", "ws:192.168.0.51", "手机网页 · 访客", "in", "text", "你好，这个页面能发文件吗", minutes(40)),
    msg("g2", "ws:192.168.0.51", "我的手机", "out", "text", "可以，点下面的回形针选择文件就行", minutes(38)),
    msg("g3", "ws:192.168.0.51", "手机网页 · 访客", "in", "text", "在吗？帮我发一下刚才的照片", minutes(26)),
  ];
  const living: ChatMessage[] = [
    msg("l1", "peer-living", "我的手机", "out", "text", "帮我把桌面上的照片传过来", hours(6)),
    fileMsg("l2", "peer-living", "客厅的电脑", "in", "IMG_2043.jpg", 3_910_000, hours(5) + 120_000),
    msg("l3", "peer-living", "我的手机", "out", "text", "已经收到，谢谢！", hours(5)),
    { ...msg("l4", "peer-living", "我的手机", "out", "text", "这份方案你有空看一下", minutes(12)), queued: true },
    { ...msg("l5", "peer-living", "我的手机", "out", "text", "在线了回我一下", minutes(11)), queued: true },
  ];
  return {
    "peer-xbooks": xbooks,
    "ws:192.168.0.51": guest,
    "peer-living": living,
  };
}

export function mockReceived(): ReceivedFile[] {
  return [
    { name: "会议纪要.pdf", path: "/x/会议纪要.pdf", bytes: 842_000, at: minutes(2) },
    { name: "IMG_2043.jpg", path: "/x/IMG_2043.jpg", bytes: 3_910_000, at: hours(5) },
    { name: "项目计划.docx", path: "/x/项目计划.docx", bytes: 156_000, at: hours(20) },
  ];
}

function msg(
  id: string,
  peerId: string,
  peerName: string,
  direction: "in" | "out",
  _kind: "text",
  text: string,
  at: number,
  queued = false,
): ChatMessage {
  return {
    id,
    peerId,
    peerName,
    direction,
    kind: "text",
    text,
    fileName: "",
    filePath: "",
    bytes: 0,
    at,
    failed: false,
    queued,
  };
}

function fileMsg(
  id: string,
  peerId: string,
  peerName: string,
  direction: "in" | "out",
  fileName: string,
  bytes: number,
  at: number,
): ChatMessage {
  return {
    id,
    peerId,
    peerName,
    direction,
    kind: "file",
    text: "",
    fileName,
    filePath: `/data/user/0/com.filedrop.app/FileDrop/${fileName}`,
    bytes,
    at,
    failed: false,
    queued: false,
  };
}
