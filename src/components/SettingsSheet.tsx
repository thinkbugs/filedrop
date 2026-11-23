import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../lib/bridge";
import type { MenuState, StorageRoots, TransferSnapshot, TransferStatus } from "../types";
import Avatar from "./Avatar";
import {
  IconBack,
  IconCheck,
  IconChevron,
  IconFolder,
  IconPopOut,
  IconX,
} from "./Icons";

const RELEASES_URL = "https://github.com/thinkbugs/filedrop/releases";

/** 1.0.0 → 1.0, so the update row reads like a release line, not a build. */
function shortVersion(version: string): string {
  const parts = version.split(".");
  return parts.length >= 2 ? `${parts[0]}.${parts[1]}` : version;
}

/** Trailing slice of the instance id, shown as the WeChat-style “微信号”. */
function shortId(instanceId: string): string {
  const clean = instanceId.replace(/[^a-zA-Z0-9]/g, "");
  return clean ? `…${clean.slice(-6).toLowerCase()}` : instanceId;
}

/** The download folder primary shared storage is offered as. */
function downloadRoot(sharedRoot: string): string {
  return `${sharedRoot.replace(/\/+$/, "")}/Download`;
}

/** A path the phone user can recognise, instead of `/storage/emulated/0/…`. */
function describeStorage(dir: string, roots: StorageRoots | null): string {
  if (!roots) return dir;
  if (dir === roots.privateDir) return "应用私有目录";
  const download = downloadRoot(roots.sharedRoot);
  if (dir === download) return "下载";
  if (dir.startsWith(`${download}/`)) return `下载 / ${dir.slice(download.length + 1)}`;
  return dir;
}

interface Props {
  status: TransferStatus;
  onClose: () => void;
  onSaved: (snapshot: TransferSnapshot) => void;
}

export default function SettingsSheet({ status, onClose, onSaved }: Props) {
  const [deviceName, setDeviceName] = useState(status.deviceName);
  const [port, setPort] = useState(String(status.port));
  const [storageDir, setStorageDir] = useState(status.storageDir);
  const [autoStart, setAutoStart] = useState(status.autoStart);
  const [notify, setNotify] = useState(status.notify);
  const [menu, setMenu] = useState<MenuState | null>(null);
  const [menuBusy, setMenuBusy] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [roots, setRoots] = useState<StorageRoots | null>(null);
  const [storageOpen, setStorageOpen] = useState(false);
  const [batteryExempt, setBatteryExempt] = useState<boolean | null>(null);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  // The registry is the source of truth for the shell verb, so we read it every
  // time the sheet opens rather than caching the last answer.
  useEffect(() => {
    if (status.mobile) return;
    void api
      .menuState()
      .then(setMenu)
      .catch(() => setMenu(null));
  }, [status.mobile]);

  // A phone has no folder dialog, so the choices are built from the two roots
  // the platform reports.
  useEffect(() => {
    if (!status.mobile) return;
    void api
      .storageRoots()
      .then(setRoots)
      .catch(() => setRoots(null));
  }, [status.mobile]);

  // Doze cuts the network off for apps that are not exempted no matter what
  // locks the receiver holds, so the state is re-read every time the sheet is
  // shown — the user may have changed it in Settings meanwhile.
  useEffect(() => {
    if (status.mobile) {
      void api
        .batteryState()
        .then(setBatteryExempt)
        .catch(() => setBatteryExempt(null));
    }
    const onVisible = () => {
      if (document.visibilityState !== "visible") return;
      void api
        .batteryState()
        .then(setBatteryExempt)
        .catch(() => undefined);
    };
    document.addEventListener("visibilitychange", onVisible);
    return () => document.removeEventListener("visibilitychange", onVisible);
  }, [status.mobile]);

  async function askBatteryExemption() {
    try {
      await api.batteryRequest();
    } catch (reason) {
      setError(`无法打开电池优化设置：${reason instanceof Error ? reason.message : reason}`);
    }
  }

  async function pickFolder() {
    if (status.mobile) return;
    try {
      const picked = await open({
        directory: true,
        multiple: false,
        title: "选择保存位置",
      });
      if (typeof picked === "string") setStorageDir(picked);
    } catch (reason) {
      setError(`无法打开选择框：${reason instanceof Error ? reason.message : reason}`);
    }
  }

  async function save() {
    const portNumber = Number(port);
    if (!deviceName.trim()) {
      setError("设备名称不能为空");
      return;
    }
    if (!Number.isInteger(portNumber) || portNumber < 1 || portNumber > 65535) {
      setError("端口需要在 1 到 65535 之间");
      return;
    }
    if (!storageDir.trim()) {
      setError("保存位置不能为空");
      return;
    }

    setSaving(true);
    setError(null);
    try {
      const wasRunning = status.running;
      if (wasRunning) await api.stop();
      let snapshot = await api.saveConfig({
        deviceName: deviceName.trim(),
        storageDir: storageDir.trim(),
        port: portNumber,
        autoStart,
      });
      if (wasRunning) snapshot = await api.start(portNumber);
      // Notifications live outside the server, so they flip without a restart.
      if (notify !== status.notify) snapshot = await api.setNotify(notify);
      onSaved(snapshot);
      onClose();
    } catch (reason) {
      setError(`保存失败：${reason instanceof Error ? reason.message : reason}`);
    } finally {
      setSaving(false);
    }
  }

  async function toggleMenu(enabled: boolean) {
    setMenuBusy(true);
    setError(null);
    try {
      setMenu(await api.menuSet(enabled));
    } catch (reason) {
      setError(
        `${enabled ? "添加" : "移除"}右键菜单失败：${
          reason instanceof Error ? reason.message : reason
        }`,
      );
    } finally {
      setMenuBusy(false);
    }
  }

  async function openReleases() {
    try {
      await api.openUrl(RELEASES_URL);
    } catch (reason) {
      setError(`无法打开更新页面：${reason instanceof Error ? reason.message : reason}`);
    }
  }

  if (status.mobile) {
    return (
      <div className="me-page" role="region" aria-label="我的">
        <header className="me-nav">
          {/* A wide-but-mobile window (landscape phone, tablet) loses the tab bar,
              so this arrow is the only way out; CSS hides it when the tabs show. */}
          <button className="icon-btn" onClick={onClose} aria-label="返回">
            <IconBack size={22} />
          </button>
          <h2>我的</h2>
        </header>

        <div className="me-scroll">
          {error && <p className="me-error">{error}</p>}

          <div className="me-profile">
            <Avatar name={deviceName || "FileDrop"} kind="device" size={54} />
            <div className="me-profile-main">
              <strong>{deviceName || status.deviceName}</strong>
              <span>设备 ID {shortId(status.instanceId)}</span>
            </div>
            <IconChevron size={18} className="me-qr" />
          </div>

          <section className="me-group">
            <div className="me-group-list">
              <div className="me-cell">
                <span className="me-cell-label">启动时自动接收</span>
                <button
                  type="button"
                  role="switch"
                  aria-checked={autoStart}
                  className={`switch ${autoStart ? "on" : ""}`}
                  onClick={() => setAutoStart((value) => !value)}
                >
                  <span />
                </button>
              </div>
              <div className="me-cell">
                <span className="me-cell-label">
                  收到文件时通知
                  <small>应用不在前台时提醒</small>
                </span>
                <button
                  type="button"
                  role="switch"
                  aria-checked={notify}
                  className={`switch ${notify ? "on" : ""}`}
                  onClick={() => setNotify((value) => !value)}
                >
                  <span />
                </button>
              </div>
              <div className="me-cell">
                <span className="me-cell-label">
                  息屏后保持在线
                  <small>
                    {batteryExempt
                      ? "已允许，锁屏后其他设备仍能发来文件"
                      : "未允许时，息屏一会儿系统会断开网络，对方会以为你离线"}
                  </small>
                </span>
                {batteryExempt ? (
                  <span className="me-cell-value">已允许</span>
                ) : (
                  <button
                    type="button"
                    className="btn sm"
                    onClick={() => void askBatteryExemption()}
                  >
                    去允许
                  </button>
                )}
              </div>
            </div>
          </section>

          <section className="me-group">
            <div className="me-group-list">
              <label className="me-cell">
                <span className="me-cell-label">设备名称</span>
                <input
                  value={deviceName}
                  maxLength={40}
                  onChange={(event) => setDeviceName(event.target.value)}
                />
              </label>
              <label className="me-cell">
                <span className="me-cell-label">接收端口</span>
                <input
                  value={port}
                  inputMode="numeric"
                  onChange={(event) => setPort(event.target.value.replace(/\D/g, ""))}
                />
              </label>
              <button
                type="button"
                className="me-cell me-cell-link"
                onClick={() => setStorageOpen((open) => !open)}
                aria-expanded={storageOpen}
              >
                <span className="me-cell-label">保存位置</span>
                <span className="me-cell-value" title={storageDir}>
                  {describeStorage(storageDir, roots)}
                </span>
                <span className={`me-chev ${storageOpen ? "open" : ""}`}>
                  <IconChevron size={16} />
                </span>
              </button>
              {storageOpen && (
                <StorageChoices
                  value={storageDir}
                  roots={roots}
                  onChange={setStorageDir}
                  onDone={() => setStorageOpen(false)}
                />
              )}
            </div>
          </section>

          <section className="me-group">
            <div className="me-group-list">
              <button className="me-cell" onClick={() => void openReleases()}>
                <span className="me-cell-label">当前版本</span>
                <span className="me-cell-value">
                  FileDrop {shortVersion(status.version)}
                </span>
                <IconPopOut size={15} className="me-chevron" />
              </button>
            </div>
          </section>

          <section className="me-group">
            <div className="me-group-list">
              <button
                type="button"
                className="me-cell me-cell-action"
                onClick={() => void save()}
                disabled={saving}
              >
                {saving ? "保存中…" : "保存设置"}
              </button>
            </div>
          </section>
        </div>
      </div>
    );
  }

  return (
    <div
      className="sheet-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <aside className="sheet" role="dialog" aria-modal="true" aria-label="设置">
        <header className="sheet-head">
          <div>
            <h2>设置</h2>
            <p>修改会保存在这台设备上</p>
          </div>
          <button className="icon-btn" onClick={onClose} aria-label="关闭设置">
            <IconX size={16} />
          </button>
        </header>

        <div className="sheet-body">
          {error && <div className="inline-error">{error}</div>}

          <section className="field-group">
            <h3>常规</h3>
            <label className="field">
              <span>设备名称</span>
              <input
                value={deviceName}
                maxLength={40}
                onChange={(event) => setDeviceName(event.target.value)}
              />
              <small>其他设备会在列表里看到这个名字</small>
            </label>
            <div className="field inline">
              <span>
                启动时自动接收
                <small>打开 FileDrop 后立即可收文件</small>
              </span>
              <button
                type="button"
                role="switch"
                aria-checked={autoStart}
                className={`switch ${autoStart ? "on" : ""}`}
                onClick={() => setAutoStart((value) => !value)}
              >
                <span />
              </button>
            </div>
            <div className="field inline">
              <span>
                收到文件时通知
                <small>窗口不在前台时弹出系统通知</small>
              </span>
              <button
                type="button"
                role="switch"
                aria-checked={notify}
                className={`switch ${notify ? "on" : ""}`}
                onClick={() => setNotify((value) => !value)}
              >
                <span />
              </button>
            </div>
          </section>

          <section className="field-group">
            <h3>网络</h3>
            <label className="field">
              <span>接收端口</span>
              <input
                value={port}
                inputMode="numeric"
                onChange={(event) => setPort(event.target.value.replace(/\D/g, ""))}
              />
              <small>默认 8787，端口被占用时可以换一个</small>
            </label>
          </section>

          <section className="field-group">
            <h3>保存位置</h3>
            <label className="field">
              <span>文件夹</span>
              <div className="field-row">
                <input
                  value={storageDir}
                  onChange={(event) => setStorageDir(event.target.value)}
                />
                <button type="button" className="btn sm" onClick={() => void pickFolder()}>
                  <IconFolder size={14} />
                  选择
                </button>
              </div>
              <small>收到的文件会直接落在这个文件夹里</small>
            </label>
          </section>

          {!status.mobile && menu?.supported && (
            <section className="field-group">
              <h3>系统整合</h3>
              <div className="field inline">
                <span>
                  文件右键菜单「FileDrop 传送」
                  <small>
                    {menu.installed
                      ? "已添加到这台电脑。选中文件右键即可发送给最近联系的设备。"
                      : "打开后可在资源管理器中选中文件（支持多选）→ 右键 → FileDrop 传送。"}
                  </small>
                </span>
                <button
                  type="button"
                  role="switch"
                  aria-checked={menu.installed}
                  className={`switch ${menu.installed ? "on" : ""}`}
                  disabled={menuBusy}
                  onClick={() => void toggleMenu(!menu.installed)}
                >
                  <span />
                </button>
              </div>
            </section>
          )}

          <section className="field-group">
            <h3>更新</h3>
            <div className="field inline">
              <span>
                当前版本 {shortVersion(status.version)}
                <small>有新版本时会发布到 GitHub Releases。</small>
              </span>
              <button
                type="button"
                className="btn sm"
                onClick={() => void openReleases()}
              >
                <IconPopOut size={14} />
                检查更新
              </button>
            </div>
          </section>

          <section className="about-row">
            <span>FileDrop {status.version}</span>
            <span className="dot-sep" />
            <span>
              {status.platform === "windows"
                ? "Windows"
                : status.platform === "macos"
                  ? "macOS"
                  : status.platform === "android"
                    ? "Android"
                    : status.platform}
            </span>
            <span className="dot-sep" />
            <span>传输协议来自 Snapdrop（MIT）</span>
          </section>

          {status.running && (
            <p className="sheet-note">保存时接收会短暂重启，二维码地址保持不变。</p>
          )}
        </div>

        <footer className="sheet-foot">
          <button className="btn ghost" onClick={onClose}>
            取消
          </button>
          <button className="btn primary" onClick={() => void save()} disabled={saving}>
            {saving ? "保存中…" : "保存"}
          </button>
        </footer>
      </aside>
    </div>
  );
}

/**
 * The folders a phone will actually accept.
 *
 * Android hands files to MediaStore rather than to a path the app chose, and the
 * transfer server claims the download collection — so the list is the download
 * folder and its subfolders, plus the private directory that always works.
 */
function StorageChoices({
  value,
  roots,
  onChange,
  onDone,
}: {
  value: string;
  roots: StorageRoots | null;
  onChange: (path: string) => void;
  onDone: () => void;
}) {
  const [subfolder, setSubfolder] = useState("");

  if (!roots) {
    return (
      <div className="me-storage">
        <span className="me-storage-note">
          这台设备没有报告可选的文件夹，只能直接填写路径。
        </span>
        <input
          className="me-storage-path"
          value={value}
          onChange={(event) => onChange(event.target.value)}
        />
      </div>
    );
  }

  const download = downloadRoot(roots.sharedRoot);
  const options = [
    { path: `${download}/FileDrop`, title: "下载 / FileDrop", note: "推荐：收到的文件都集中在这里" },
    { path: download, title: "下载", note: "与其他应用的文件放在一起" },
    { path: roots.privateDir, title: "应用私有目录", note: "无需任何权限，其他应用与相册看不到" },
  ];

  function choose(path: string) {
    onChange(path);
    onDone();
  }

  return (
    <div className="me-storage">
      {options.map((option) => (
        <button
          key={option.path}
          type="button"
          role="radio"
          aria-checked={value === option.path}
          className="me-storage-option"
          onClick={() => choose(option.path)}
        >
          <span className="me-storage-main">
            <strong>{option.title}</strong>
            <small>{option.note}</small>
          </span>
          <span className="me-storage-tick">
            {value === option.path && <IconCheck size={15} />}
          </span>
        </button>
      ))}

      <div className="me-storage-custom">
        <input
          value={subfolder}
          maxLength={30}
          placeholder="下载下的其他子文件夹"
          onChange={(event) => setSubfolder(event.target.value.replace(/[\\/:*?"<>|]/g, ""))}
        />
        <button
          type="button"
          className="btn sm"
          disabled={!subfolder.trim()}
          onClick={() => choose(`${download}/${subfolder.trim()}`)}
        >
          使用
        </button>
      </div>

      <small className="me-storage-note">
        选好后要点击「保存设置」才会生效。若某次系统不允许写入这个文件夹，文件会自动存入应用私有目录，不会丢失。
      </small>
    </div>
  );
}
