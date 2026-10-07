import type { AmbientMode, HeadsetConnection, HeadsetState } from "./types";

export const AMBIENT_OPTIONS: { value: AmbientMode; label: string }[] = [
  { value: "noiseCanceling", label: "Noise cancelling" },
  { value: "transparency", label: "Transparency" },
  { value: "normal", label: "Normal" },
];

export const LEVEL_OPTIONS = [1, 2, 3, 4, 5].map((n) => ({ value: n, label: String(n) }));

export function isConnected(h: HeadsetState | null): boolean {
  return h?.connection.state === "connected";
}

export function connectionLabel(c: HeadsetConnection): string {
  switch (c.state) {
    case "connected":
      return "Connected";
    case "connecting":
      return "Connecting…";
    case "notFound":
      return "Not found. Switch it on and pair it with this PC";
    case "disconnected":
      return `Not connected, retrying in ${c.retryInSecs} s`;
  }
}
