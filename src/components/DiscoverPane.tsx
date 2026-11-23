import { useState } from "react";
import type { Peer } from "../types";
import Avatar from "./Avatar";
import { IconChevron, IconPlus, IconQr, IconRefresh, IconWifi } from "./Icons";

interface Props {
  peers: Peer[];
  chatted: Record<string, boolean>;
  scanning: boolean;
  narrow: boolean;
  onOpen: (peer: Peer) => void;
  onAdd: (host: string, port: number) => Promise<boolean>;
  onRefresh: () => void;
  onGoReceive: () => void;
  onBack: () => void;
}

const DEFAULT_PORT = 8787;

/** Splits `192.168.1.5`, `192.168.1.5:9000` or `host.local:9000`. */
function parseAddress(value: string): { host: string; port: number } | null {
  const text = value.trim().replace(/^https?:\/\//, "").replace(/\/.*$/, "");
  if (!text) return null;
  const colon = text.lastIndexOf(":");
  if (colon < 0) return { host: text, port: DEFAULT_PORT };
  const port = Number(text.slice(colon + 1));
  if (!Number.isInteger(port) || port < 1 || port > 65535) return null;
  return { host: text.slice(0, colon), port };
}

export default function DiscoverPane({
  peers,
  chatted,
  scanning,
  narrow,
  onOpen,
  onAdd,
  onRefresh,
  onGoReceive,
  onBack,
}: Props) {
  const [address, setAddress] = useState("");
  const [invalid, setInvalid] = useState("");

  async function add() {
    const parsed = parseAddress(address);
    if (!parsed) {
      setInvalid("填写设备的 IP 或地址，例如 192.168.1.12:8787");
      return;
    }
    setInvalid("");
    if (await onAdd(parsed.host, parsed.port)) setAddress("");
  }

  return (
    <section className="pane discover">
      <header className="pane-head">
        {narrow && (
          <button className="icon-btn" onClick={onBack} title="返回">
            <IconChevron size={16} className="flip" />
          </button>
        )}
        <div className="pane-title">
          <h2>发现设备</h2>
          <p>同一局域网里开着文件传输的 FileDrop，会自动出现在这里</p>
        </div>
        <button className="icon-btn" onClick={onRefresh} title="重新搜索">
          <IconRefresh className={scanning ? "spinning" : ""} />
        </button>
      </header>

      <div className="pane-body">
        {peers.length > 0 ? (
          <ul className="device-cards">
            {peers.map((peer) => (
              <li key={peer.id}>
                <button className="device-card" onClick={() => onOpen(peer)}>
                  <Avatar name={peer.name} kind="device" size={44} online />
                  <span className="card-main">
                    <strong>{peer.name}</strong>
                    <span>
                      <IconWifi size={12} /> {peer.host}:{peer.port}
                    </span>
                  </span>
                  {chatted[peer.id] && <span className="card-tag">已有对话</span>}
                  <IconChevron size={16} className="card-go" />
                </button>
              </li>
            ))}
          </ul>
        ) : (
          <div className="radar-empty">
            <span className={`radar ${scanning ? "on" : ""}`}>
              <i />
              <i />
              <i />
              <b>
                <IconWifi size={18} />
              </b>
            </span>
            <strong>{scanning ? "正在搜索同一网络的设备" : "暂时没有找到其他设备"}</strong>
            <span>
              确认对方也打开了 FileDrop 的文件传输，并且两台设备连着同一个 Wi-Fi 或网段
            </span>
          </div>
        )}

        <div className="discover-foot">
          <div className="manual">
            <span className="manual-title">知道对方地址？直接添加</span>
            <div className="manual-row">
              <input
                value={address}
                onChange={(event) => {
                  setAddress(event.target.value);
                  setInvalid("");
                }}
                onKeyDown={(event) => {
                  if (event.key === "Enter") void add();
                }}
                placeholder={`IP 或主机名，例如 192.168.1.12:${DEFAULT_PORT}`}
                spellCheck={false}
              />
              <button className="btn sm" onClick={() => void add()} disabled={!address.trim()}>
                <IconPlus size={14} />
                添加
              </button>
            </div>
            {invalid && <em className="manual-error">{invalid}</em>}
            <span className="manual-hint">
              添加后会尝试连接对方，成功则用它的真实设备名建立对话
            </span>
          </div>

          <button className="qr-link" onClick={onGoReceive}>
            <span className="qr-link-icon">
              <IconQr size={18} />
            </span>
            <span className="qr-link-main">
              <strong>让手机扫码传文件</strong>
              <span>手机无需安装，打开网页即可发送文字与文件</span>
            </span>
            <IconChevron size={16} />
          </button>
        </div>
      </div>
    </section>
  );
}
