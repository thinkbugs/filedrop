import type { ReactNode } from "react";
import { IconChat, IconMe, IconQr, IconRadar, IconSettings, LogoMark } from "./Icons";

export type Tab = "chats" | "discover" | "receive";

interface Props {
  tab: Tab;
  unread: number;
  running: boolean;
  deviceName: string;
  /** Phone layout: last tab reads 我的 and opens the profile/settings page. */
  mobile: boolean;
  /** 我的 tab is selected while the settings sheet is open. */
  meActive: boolean;
  onSelect: (tab: Tab) => void;
  onSettings: () => void;
}

function Item({
  active,
  badge,
  dot,
  label,
  icon,
  onClick,
}: {
  active: boolean;
  badge?: number;
  dot?: boolean;
  label: string;
  icon: ReactNode;
  onClick: () => void;
}) {
  return (
    <button className={`rail-item ${active ? "active" : ""}`} onClick={onClick} title={label}>
      <span className="rail-icon">
        {icon}
        {Boolean(badge) && <em className="rail-badge">{badge! > 99 ? "99+" : badge}</em>}
        {dot && !badge && <em className="rail-dot" />}
      </span>
      <span className="rail-label">{label}</span>
    </button>
  );
}

export default function Rail({
  tab,
  unread,
  running,
  deviceName,
  mobile,
  meActive,
  onSelect,
  onSettings,
}: Props) {
  return (
    <nav className="rail" aria-label="功能">
      <div className="rail-brand" title={deviceName}>
        <span className="rail-mark">
          <LogoMark size={21} />
        </span>
      </div>

      <div className="rail-group">
        <Item
          active={tab === "chats" && !meActive}
          badge={unread}
          label="对话"
          icon={<IconChat size={21} />}
          onClick={() => onSelect("chats")}
        />
        <Item
          active={tab === "discover" && !meActive}
          label="发现"
          icon={<IconRadar size={21} />}
          onClick={() => onSelect("discover")}
        />
        <Item
          active={tab === "receive" && !meActive}
          label="接收"
          icon={<IconQr size={21} />}
          onClick={() => onSelect("receive")}
        />
        {mobile ? (
          <Item
            active={meActive}
            dot={running}
            label="我的"
            icon={<IconMe size={21} />}
            onClick={onSettings}
          />
        ) : null}
      </div>

      <div className="rail-bottom">
        {!mobile && (
          <Item
            active={false}
            label="设置"
            icon={<IconSettings size={21} />}
            onClick={onSettings}
          />
        )}
        <span
          className={`rail-status ${running ? "live" : ""}`}
          title={running ? "正在接收" : "接收已停止"}
        />
      </div>
    </nav>
  );
}
