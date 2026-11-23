export function formatBytes(bytes: number): string {
  if (!bytes || bytes < 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const exponent = Math.min(
    units.length - 1,
    Math.floor(Math.log(bytes) / Math.log(1024)),
  );
  const value = bytes / 1024 ** exponent;
  const digits = exponent === 0 ? 0 : value >= 100 ? 0 : value >= 10 ? 1 : 2;
  return `${value.toFixed(digits)} ${units[exponent]}`;
}

export function timeAgo(milliseconds: number): string {
  const seconds = Math.max(0, Math.round((Date.now() - milliseconds) / 1000));
  if (seconds < 10) return "刚刚";
  if (seconds < 60) return `${seconds} 秒前`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes} 分钟前`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} 小时前`;
  const days = Math.floor(hours / 24);
  return days === 1 ? "昨天" : `${days} 天前`;
}

export function extensionOf(name: string): string {
  const dot = name.lastIndexOf(".");
  if (dot < 0 || dot === name.length - 1) return "FILE";
  return name.slice(dot + 1, dot + 5).toUpperCase();
}

export function percent(progress: number): number {
  return Math.max(0, Math.min(100, Math.round(progress * 100)));
}

const DAY_MS = 24 * 60 * 60 * 1000;

function startOfDay(milliseconds: number): number {
  const date = new Date(milliseconds);
  date.setHours(0, 0, 0, 0);
  return date.getTime();
}

function pad(value: number): string {
  return value < 10 ? `0${value}` : String(value);
}

/** The clock the device list shows: today `14:05`, yesterday, then the date. */
export function listTime(milliseconds: number): string {
  if (!milliseconds) return "";
  const days = Math.floor((startOfDay(Date.now()) - startOfDay(milliseconds)) / DAY_MS);
  const date = new Date(milliseconds);
  if (days <= 0) return `${pad(date.getHours())}:${pad(date.getMinutes())}`;
  if (days === 1) return "昨天";
  if (days < 7) return ["周日", "周一", "周二", "周三", "周四", "周五", "周六"][date.getDay()];
  return `${date.getMonth() + 1}月${date.getDate()}日`;
}

export function clockOf(milliseconds: number): string {
  const date = new Date(milliseconds);
  return `${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

/** The separator between message groups. */
export function dayLabel(milliseconds: number): string {
  const days = Math.floor((startOfDay(Date.now()) - startOfDay(milliseconds)) / DAY_MS);
  const date = new Date(milliseconds);
  if (days <= 0) return "今天";
  if (days === 1) return "昨天";
  const year = date.getFullYear();
  const current = new Date().getFullYear();
  const base = `${date.getMonth() + 1}月${date.getDate()}日`;
  return year === current ? base : `${year}年${base}`;
}
