interface IconProps {
  size?: number;
  className?: string;
}

const base = (size: number) => ({
  width: size,
  height: size,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.7,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  "aria-hidden": true,
});

export function DropMark({ size = 22, className }: IconProps) {
  return (
    <svg {...base(size)} className={className} strokeWidth={1.6}>
      <path d="M12 3.2c3.4 3.6 6.2 6.9 6.2 10.1A6.2 6.2 0 0 1 12 19.5a6.2 6.2 0 0 1-6.2-6.2C5.8 10.1 8.6 6.8 12 3.2Z" />
      <path d="M12 9.5v5.6M9.6 12.6 12 15l2.4-2.4" />
    </svg>
  );
}

export function LogoMark({ size = 22, className }: IconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      className={className}
      aria-hidden="true"
      fill="none"
    >
      <path
        d="M21.6 3.1 2.7 10c-.92.32-.86 1.63.08 1.87l6.05 1.55 1.55 6.05c.24.94 1.55 1 1.87.08L21.6 3.1Z"
        fill="#ffffff"
      />
      <path
        d="M21.6 3.1 9.9 13.4"
        stroke="rgba(4,48,28,0.35)"
        strokeWidth="1.1"
        strokeLinecap="round"
      />
      <path
        d="M3.6 20.6c2.9 1.05 6.5.85 9.4-.7"
        stroke="#ffffff"
        strokeWidth="1.6"
        strokeLinecap="round"
      />
    </svg>
  );
}

export function IconSettings({ size = 18, className }: IconProps) {
  return (
    <svg {...base(size)} className={className}>
      <circle cx="12" cy="12" r="3.1" />
      <path d="M19.4 15a1.7 1.7 0 0 0 .34 1.87l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.7 1.7 0 0 0-1.87-.34 1.7 1.7 0 0 0-1.03 1.56V21a2 2 0 1 1-4 0v-.09A1.7 1.7 0 0 0 8.9 19.3a1.7 1.7 0 0 0-1.87.34l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.7 1.7 0 0 0 4.54 15a1.7 1.7 0 0 0-1.56-1.03H3a2 2 0 1 1 0-4h.09A1.7 1.7 0 0 0 4.7 8.9a1.7 1.7 0 0 0-.34-1.87l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.7 1.7 0 0 0 9 4.54h.08A1.7 1.7 0 0 0 10.1 2.98V3a2 2 0 1 1 4 0v.09a1.7 1.7 0 0 0 1.03 1.56 1.7 1.7 0 0 0 1.87-.34l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.7 1.7 0 0 0 19.46 9v.08A1.7 1.7 0 0 0 21 10.1h-.09A2 2 0 1 1 21 14h-.09a1.7 1.7 0 0 0-1.51 1Z" />
    </svg>
  );
}

export function IconRefresh({ size = 16, className }: IconProps) {
  return (
    <svg {...base(size)} className={className}>
      <path d="M20 11a8 8 0 1 0-2.4 6.3" />
      <path d="M20 5v6h-6" />
    </svg>
  );
}

export function IconFolder({ size = 16, className }: IconProps) {
  return (
    <svg {...base(size)} className={className}>
      <path d="M3.5 7.2v10.3c0 .9.7 1.6 1.6 1.6h13.8c.9 0 1.6-.7 1.6-1.6V8.8c0-.9-.7-1.6-1.6-1.6h-6l-1.6-2H5.1c-.9 0-1.6.7-1.6 1.6Z" />
    </svg>
  );
}

export function IconCopy({ size = 15, className }: IconProps) {
  return (
    <svg {...base(size)} className={className}>
      <rect x="9" y="9" width="11" height="11" rx="2.4" />
      <path d="M6 15H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h8a2 2 0 0 1 2 2v1" />
    </svg>
  );
}

export function IconSend({ size = 15, className }: IconProps) {
  return (
    <svg {...base(size)} className={className}>
      <path d="m21 3-8.5 18-2-7.5L3 11.5 21 3Z" />
      <path d="M21 3l-10.5 10.5" />
    </svg>
  );
}

export function IconPlus({ size = 15, className }: IconProps) {
  return (
    <svg {...base(size)} className={className}>
      <path d="M12 5v14M5 12h14" />
    </svg>
  );
}

export function IconTrash({ size = 14, className }: IconProps) {
  return (
    <svg {...base(size)} className={className}>
      <path d="M4 7h16M9 7V4.8c0-.4.4-.8.8-.8h4.4c.4 0 .8.4.8.8V7M6.5 7l.9 12.2c.1.9.8 1.6 1.7 1.6h5.8c.9 0 1.6-.7 1.7-1.6L17.5 7" />
    </svg>
  );
}

export function IconDesktop({ size = 22, className }: IconProps) {
  return (
    <svg {...base(size)} className={className}>
      <rect x="3" y="4" width="18" height="12.5" rx="2" />
      <path d="M9 20.5h6M12 16.5v4" />
    </svg>
  );
}

export function IconInbox({ size = 18, className }: IconProps) {
  return (
    <svg {...base(size)} className={className}>
      <path d="M3.5 13.5 6 5.5c.2-.9 1-1.5 1.9-1.5h8.2c.9 0 1.7.6 1.9 1.5l2.5 8" />
      <path d="M3.5 13.5v4c0 1.1.9 2 2 2h13c1.1 0 2-.9 2-2v-4h-4.2c-.5 1.5-1.9 2.5-4.3 2.5s-3.8-1-4.3-2.5H3.5Z" />
    </svg>
  );
}

export function IconWifi({ size = 14, className }: IconProps) {
  return (
    <svg {...base(size)} className={className}>
      <path d="M4 9.5a12 12 0 0 1 16 0M6.8 12.6a8 8 0 0 1 10.4 0M9.6 15.7a4 4 0 0 1 4.8 0" />
      <circle cx="12" cy="19" r="0.9" fill="currentColor" stroke="none" />
    </svg>
  );
}

export function IconPopOut({ size = 14, className }: IconProps) {
  return (
    <svg {...base(size)} className={className}>
      <path d="M14 4h6v6M20 4l-9 9" />
      <path d="M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5" />
    </svg>
  );
}

export function IconX({ size = 14, className }: IconProps) {
  return (
    <svg {...base(size)} className={className}>
      <path d="M6 6l12 12M18 6 6 18" />
    </svg>
  );
}

export function IconChat({ size = 20, className }: IconProps) {
  return (
    <svg {...base(size)} className={className}>
      <path d="M4 6.5A2.5 2.5 0 0 1 6.5 4h11A2.5 2.5 0 0 1 20 6.5v7a2.5 2.5 0 0 1-2.5 2.5H12l-4.6 3.2a.9.9 0 0 1-1.4-.7V16a2.5 2.5 0 0 1-2-2.4V6.5Z" />
      <path d="M8.4 9.2h7.2M8.4 12h4.6" />
    </svg>
  );
}

export function IconMe({ size = 20, className }: IconProps) {
  return (
    <svg {...base(size)} className={className}>
      <circle cx="12" cy="8" r="3.7" />
      <path d="M5 20a7 7 0 0 1 14 0" />
    </svg>
  );
}

export function IconRadar({ size = 20, className }: IconProps) {
  return (
    <svg {...base(size)} className={className}>
      <circle cx="12" cy="12" r="8.4" />
      <circle cx="12" cy="12" r="4.4" />
      <path d="M12 12 18 6.6" />
      <circle cx="12" cy="12" r="1.1" fill="currentColor" stroke="none" />
    </svg>
  );
}

export function IconQr({ size = 20, className }: IconProps) {
  return (
    <svg {...base(size)} className={className}>
      <rect x="3.6" y="3.6" width="6.4" height="6.4" rx="1.6" />
      <rect x="14" y="3.6" width="6.4" height="6.4" rx="1.6" />
      <rect x="3.6" y="14" width="6.4" height="6.4" rx="1.6" />
      <path d="M14 14h2.6v2.6H14zM17.8 17.8h2.6v2.6h-2.6M14 20.4h.9" />
    </svg>
  );
}

export function IconBack({ size = 18, className }: IconProps) {
  return (
    <svg {...base(size)} className={className}>
      <path d="M15 5l-7 7 7 7" />
    </svg>
  );
}

export function IconChevron({ size = 16, className }: IconProps) {
  return (
    <svg {...base(size)} className={className}>
      <path d="M9 5l7 7-7 7" />
    </svg>
  );
}

export function IconPaperclip({ size = 18, className }: IconProps) {
  return (
    <svg {...base(size)} className={className}>
      <path d="M17.6 11.4 9.5 19.5a4.4 4.4 0 0 1-6.2-6.2l8.9-8.9a3 3 0 0 1 4.2 4.2l-8.6 8.6a1.6 1.6 0 0 1-2.2-2.2l7.9-7.9" />
    </svg>
  );
}

export function IconSearch({ size = 15, className }: IconProps) {
  return (
    <svg {...base(size)} className={className}>
      <circle cx="11" cy="11" r="6.4" />
      <path d="m15.8 15.8 3.6 3.6" />
    </svg>
  );
}

export function IconPhone({ size = 20, className }: IconProps) {
  return (
    <svg {...base(size)} className={className}>
      <rect x="7" y="2.8" width="10" height="18.4" rx="2.4" />
      <path d="M10.6 5.6h2.8M12 18.2h.01" />
    </svg>
  );
}

export function IconCheck({ size = 14, className }: IconProps) {
  return (
    <svg {...base(size)} className={className} strokeWidth={2}>
      <path d="m5 12.6 4.4 4.4L19 7.4" />
    </svg>
  );
}

export function IconAlert({ size = 15, className }: IconProps) {
  return (
    <svg {...base(size)} className={className}>
      <path d="M12 4.4 20.4 19H3.6L12 4.4Z" />
      <path d="M12 9.8v4M12 16.4h.01" />
    </svg>
  );
}

export function IconClock({ size = 11, className }: IconProps) {
  return (
    <svg {...base(size)} className={className}>
      <circle cx="12" cy="12" r="8.2" />
      <path d="M12 7.6V12l3 1.8" />
    </svg>
  );
}
