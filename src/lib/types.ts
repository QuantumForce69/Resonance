export type Flow = "render" | "capture";
export type DeviceState = "active" | "unplugged";
export type Role = "console" | "communications";
export type HotkeyMode = "toggle" | "pushToTalk" | "pushToMute";
export type Theme = "system" | "light" | "dark";

export interface Boost {
  db: number;
  min: number;
  max: number;
  step: number;
}

export interface Device {
  id: string;
  name: string;
  originalName: string;
  alias: string | null;
  hidden: boolean;
  flow: Flow;
  state: DeviceState;
  defaultConsole: boolean;
  defaultComms: boolean;
  volume: number;
  muted: boolean;
  boost: Boost | null;
}

export interface App {
  key: string;
  name: string;
  exePath: string | null;
  isSystem: boolean;
  pids: number[];
  sessionIds: string[];
  volume: number;
  muted: boolean;
  output: string | null;
}

export interface MicConfig {
  hotkey: string | null;
  mode: HotkeyMode;
}

export interface UiSettings {
  theme: Theme;
  startMinimized: boolean;
}

export interface UiState {
  devices: Device[];
  apps: App[];
  routingAvailable: boolean;
  mic: MicConfig;
  ui: UiSettings;
  accent: string;
}

export interface MeterFrame {
  apps: Record<string, number>;
  master: number;
  mic: number;
}

export type Command =
  | { type: "setAppVolume"; key: string; volume: number }
  | { type: "setAppMute"; key: string; muted: boolean }
  | { type: "setAppOutput"; key: string; deviceId: string | null }
  | { type: "setDeviceVolume"; id: string; volume: number }
  | { type: "setDeviceMute"; id: string; muted: boolean }
  | { type: "setDefaultDevice"; id: string; role: Role }
  | { type: "setDeviceAlias"; id: string; alias: string | null }
  | { type: "setDeviceHidden"; id: string; hidden: boolean }
  | { type: "setMicBoost"; id: string; db: number }
  | { type: "setMicMute"; muted: boolean }
  | { type: "toggleMicMute" }
  | { type: "setMicConfig"; config: MicConfig }
  | { type: "setUiSettings"; ui: UiSettings }
  | { type: "resetAppMemory" };

export interface CmdError {
  code: "notFound" | "unavailable" | "invalid" | "os";
  message: string;
}
