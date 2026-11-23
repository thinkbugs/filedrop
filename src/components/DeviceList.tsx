import type { Conversation } from "../types";
import { listTime } from "../lib/format";
import Avatar from "./Avatar";
import { IconClock, IconInbox, IconSearch, IconSettings, IconX } from "./Icons";

interface Props {
  conversations: Conversation[];
  total: number;
  query: string;
  activeId: string | null;
  running: boolean;
  port: number;
  lanIp: string;
  deviceName: string;
  onQuery: (value: string) => void;
  onSelect: (peer: Conversation) => void;
  onSettings: () => void;
  onDiscover: () => void;
}

export default function DeviceList({
  conversations,
  total,
  query,
  activeId,
  running,
  port,
  lanIp,
  deviceName,
  onQuery,
  onSelect,
  onSettings,
  onDiscover,
}: Props) {
  return (
    <aside className="list-pane">
      <header className="list-head">
        <div className="list-title">
          <h1>设备</h1>
          <button className="icon-btn list-settings" onClick={onSettings} title="设置">
            <IconSettings size={16} />
          </button>
        </div>
        <div className="search">
          <IconSearch />
          <input
            value={query}
            onChange={(event) => onQuery(event.target.value)}
            placeholder="搜索设备或地址"
            spellCheck={false}
          />
          {query && (
            <button className="search-clear" onClick={() => onQuery("")} title="清空">
              <IconX size={12} />
            </button>
          )}
        </div>
      </header>

      <div className="device-scroll">
        {conversations.length === 0 ? (
          <div className="device-empty">
            <span className="device-empty-icon">
              <IconInbox size={20} />
            </span>
            <strong>{total ? "没有匹配的设备" : "还没有对话过的设备"}</strong>
            <span>
              {total
                ? "换个名字或地址试试"
                : "在右侧点一台发现的设备，或让手机扫码发送文件，它就会出现在这里"}
            </span>
            {!total && (
              <button className="btn sm" onClick={onDiscover}>
                去发现设备
              </button>
            )}
          </div>
        ) : (
          <ul className="device-rows">
            {conversations.map((peer) => (
              <li key={peer.id}>
                <button
                  className={`device-row ${peer.id === activeId ? "active" : ""}`}
                  onClick={() => onSelect(peer)}
                >
                  <Avatar name={peer.name} kind={peer.kind} online={peer.online} />
                  <span className="device-main">
                    <span className="device-top">
                      <span className="device-name">{peer.name}</span>
                      <span className="device-time">{listTime(peer.lastAt)}</span>
                    </span>
                    <span className="device-bottom">
                      <span className="device-preview">
                        {peer.queued > 0 && (
                          <em className="preview-queued">
                            <IconClock size={10} /> 待发送
                          </em>
                        )}
                        {peer.preview ||
                          (peer.kind === "browser" ? "网页设备 · 可收发文字" : "点击开始对话")}
                      </span>
                      {peer.unread > 0 && (
                        <span className="unread">{peer.unread > 99 ? "99+" : peer.unread}</span>
                      )}
                    </span>
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>

      <footer className="list-foot">
        <span className={`status-dot ${running ? "live" : ""}`} />
        <span className="list-foot-text">
          {running ? `${lanIp || "本机"}:${port} 正在接收` : "接收已停止"}
        </span>
        <span className="list-foot-name" title={deviceName}>
          {deviceName}
        </span>
      </footer>
    </aside>
  );
}
