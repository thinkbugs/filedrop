import type { PeerKind } from "../types";
import { IconDesktop, IconPhone } from "./Icons";

const PALETTE: [string, string][] = [
  ["#e4f5ef", "#0f8a72"],
  ["#e8effa", "#2f66c4"],
  ["#f1ebfa", "#6a45c0"],
  ["#faeee2", "#b26a24"],
  ["#fbe9ed", "#bd4258"],
  ["#e6f4f8", "#167a99"],
];

function tone(name: string): [string, string] {
  let hash = 0;
  for (let index = 0; index < name.length; index += 1) {
    hash = (hash * 31 + name.charCodeAt(index)) | 0;
  }
  return PALETTE[Math.abs(hash) % PALETTE.length];
}

interface Props {
  name: string;
  kind: PeerKind;
  size?: number;
  online?: boolean;
  showDot?: boolean;
}

export default function Avatar({
  name,
  kind,
  size = 40,
  online = false,
  showDot = true,
}: Props) {
  const [background, color] = tone(name || "FileDrop");
  return (
    <span
      className="avatar"
      style={{
        width: size,
        height: size,
        background,
        color,
        borderRadius: Math.round(size * 0.28),
      }}
    >
      {kind === "browser" ? (
        <IconPhone size={Math.round(size * 0.5)} />
      ) : (
        <IconDesktop size={Math.round(size * 0.5)} />
      )}
      {showDot && <i className={`avatar-dot ${online ? "on" : ""}`} />}
    </span>
  );
}
