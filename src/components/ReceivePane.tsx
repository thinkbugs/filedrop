import type { IncomingProgress, ReceivedFile, TransferStatus } from "../types";
import { extensionOf, formatBytes, percent, timeAgo } from "../lib/format";
import {
  IconChevron,
  IconCopy,
  IconFolder,
  IconInbox,
  IconPopOut,
  IconWifi,
} from "./Icons";

interface Props {
  status: TransferStatus;
  received: ReceivedFile[];
  incoming: IncomingProgress[];
  busy: boolean;
  narrow: boolean;
  onToggle: () => void;
  onCopy: () => void;
  onOpenDir: () => void;
  onOpenFile: (file: ReceivedFile) => void;
  onBack: () => void;
}

const PLATFORM: Record<string, string> = {
  windows: "Windows",
  macos: "macOS",
  android: "Android",
  ios: "iOS",
  linux: "Linux",
};

function Row({ label, value, title }: { label: string; value: string; title?: string }) {
  return (
    <div className="meta-row">
      <span className="meta-label">{label}</span>
      <span className="meta-value" title={title ?? value}>
        {value}
      </span>
    </div>
  );
}

export default function ReceivePane({
  status,
  received,
  incoming,
  busy,
  narrow,
  onToggle,
  onCopy,
  onOpenDir,
  onOpenFile,
  onBack,
}: Props) {
  const running = status.running;
  const mobile = status.mobile;

  return (
    <section className="pane receive">
      <header className="pane-head">
        {narrow && (
          <button className="icon-btn" onClick={onBack} title="返回">
            <IconChevron size={16} className="flip" />
          </button>
        )}
        <div className="pane-title">
          <h2>手机传文件</h2>
          <p>同一网络里的手机扫码即可发送，无需安装应用</p>
        </div>
        <button
          type="button"
          role="switch"
          aria-checked={running}
          aria-label={running ? "停止接收" : "开启接收"}
          className={`switch ${running ? "on" : ""}`}
          onClick={onToggle}
          disabled={busy}
        >
          <span />
        </button>
      </header>

      <div className="pane-body receive-body">
        <div className="qr-card">
          <div className={`qr-frame ${running ? "" : "muted"}`}>
            {status.qr ? (
              <img src={status.qr} alt="接收页二维码" />
            ) : (
              <div className="qr-placeholder">
                <IconInbox size={30} />
                <span>{running ? "正在生成二维码…" : "开启接收后生成"}</span>
              </div>
            )}
          </div>
          <div className="qr-side">
            <strong className="qr-title">
              {running ? "用手机相机扫码" : "接收已停止"}
            </strong>
            <div className="link-box">
              <span className="link-text" title={status.url ?? undefined}>
                {status.url ?? "开启接收后生成地址"}
              </span>
              <button className="btn sm" onClick={onCopy} disabled={!status.url}>
                <IconCopy size={14} />
                复制
              </button>
            </div>
            {!running && (
              <button className="btn primary sm" onClick={onToggle} disabled={busy}>
                开启接收
              </button>
            )}
            <span className="qr-note">
              手机打开网页后可以直接传文件；它发来的文字与文件会记在「{status.deviceName}
              」的对话里
            </span>
          </div>
        </div>

        <div className="meta-card">
          <Row label="设备名称" value={status.deviceName} />
          <Row label="本机地址" value={status.lanIp || "未连接局域网"} />
          <Row label="接收端口" value={running ? String(status.port) : "已停止"} />
          <Row label="保存位置" value={status.storageDir} title={status.storageDir} />
          <Row
            label="运行平台"
            value={`${PLATFORM[status.platform] ?? status.platform} · v${status.version}`}
          />
          <div className="meta-actions">
            <button className="btn sm" onClick={onOpenDir} disabled={mobile}>
              <IconFolder size={14} />
              打开文件夹
            </button>
            <span className="meta-hint">
              <IconWifi size={12} /> 两台设备需在同一局域网
            </span>
          </div>
        </div>

        {incoming.length > 0 && (
          <div className="live-card">
            <div className="section-label">
              正在接收
              <span className="count">{incoming.length}</span>
            </div>
            {incoming.map((item) => {
              const known = item.total > 0;
              const value = known ? percent(item.progress) : 0;
              return (
                <div className="transfer" key={item.name}>
                  <div className="transfer-top">
                    <span className="transfer-name" title={item.name}>
                      {item.name}
                    </span>
                    <span className="transfer-value">
                      {known ? `${value}%` : formatBytes(item.received)}
                    </span>
                  </div>
                  <div className={`bar ${known ? "" : "indeterminate"}`}>
                    <i style={{ width: `${value}%` }} />
                  </div>
                </div>
              );
            })}
          </div>
        )}

        <div className="recent-card">
          <div className="section-label">
            最近接收
            <span className="count">{received.length}</span>
          </div>
          {received.length === 0 ? (
            <p className="pane-empty">收到的文件会出现在这里</p>
          ) : (
            <ul className="recent-list">
              {received.slice(0, 12).map((file) => (
                <li key={`${file.path}-${file.at}`}>
                  <button
                    className="recent-row"
                    title={file.path}
                    onClick={() => onOpenFile(file)}
                    disabled={mobile}
                  >
                    <span className="staged-ext">{extensionOf(file.name)}</span>
                    <span className="recent-copy">
                      <strong>{file.name}</strong>
                      <small>
                        {formatBytes(file.bytes)} · {timeAgo(file.at)}
                      </small>
                    </span>
                    {!mobile && <IconPopOut size={13} />}
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>
    </section>
  );
}
