import type { App, Device, UiState } from "../lib/types";

export function makeDevice(p: Partial<Device> & { id: string }): Device {
  return {
    name: `Device ${p.id}`,
    originalName: `Device ${p.id}`,
    alias: null,
    hidden: false,
    flow: "render",
    state: "active",
    defaultConsole: false,
    defaultComms: false,
    volume: 0.5,
    muted: false,
    boost: null,
    ...p,
  };
}

export function makeApp(p: Partial<App> & { key: string }): App {
  return {
    name: p.key,
    exePath: `C:\\Apps\\${p.key}`,
    isSystem: false,
    pids: [1],
    sessionIds: [`${p.key}-s`],
    volume: 1,
    muted: false,
    output: null,
    ...p,
  };
}

export function makeState(p: Partial<UiState> = {}): UiState {
  return {
    devices: [
      makeDevice({ id: "out1", name: "Headphones", defaultConsole: true, defaultComms: true }),
      makeDevice({ id: "out2", name: "Speakers" }),
      makeDevice({ id: "gone", name: "Old DAC", state: "unplugged" }),
      makeDevice({ id: "mic1", name: "Desk Mic", flow: "capture", defaultConsole: true, defaultComms: true }),
    ],
    apps: [
      makeApp({ key: "spotify.exe", name: "Spotify" }),
      makeApp({ key: "#system", name: "System sounds", isSystem: true, exePath: null }),
    ],
    routingAvailable: true,
    mic: { hotkey: "Ctrl+Shift+M", mode: "toggle" },
    ui: { theme: "system", startMinimized: false },
    accent: "#7c5cff",
    ...p,
  };
}
