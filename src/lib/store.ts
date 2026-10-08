import { create } from "zustand";
import { send, sendHeadset, setMicHotkey } from "./api";
import { DEFAULT_SEND, ECHO_WARNING, isCallApp } from "./processing";
import type { App, Command, Device, HeadsetCommand, HeadsetState, MicConfig, ProcessingSettings, UiState } from "./types";

export interface Toast {
  id: number;
  kind: "error" | "info";
  message: string;
}

interface Store {
  state: UiState | null;
  headset: HeadsetState | null;
  toasts: Toast[];
  setState: (s: UiState) => void;
  setHeadset: (h: HeadsetState) => void;
  pushToast: (kind: Toast["kind"], message: string) => void;
  dismissToast: (id: number) => void;
}

let nextToastId = 1;

export const useStore = create<Store>((set) => ({
  state: null,
  headset: null,
  toasts: [],
  setState: (state) => set({ state }),
  setHeadset: (headset) => set({ headset }),
  pushToast: (kind, message) =>
    set((s) => ({
      toasts: [...s.toasts.filter((t) => t.message !== message), { id: nextToastId++, kind, message }].slice(-4),
    })),
  dismissToast: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
}));

export function errorMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e && typeof (e as { message: unknown }).message === "string") {
    return (e as { message: string }).message;
  }
  return "Something went wrong";
}

async function attempt(action: () => Promise<unknown>): Promise<boolean> {
  try {
    await action();
    return true;
  } catch (e) {
    useStore.getState().pushToast("error", errorMessage(e));
    return false;
  }
}

/** Sends an audio command; failures become toasts. */
export const run = (cmd: Command) => attempt(() => send(cmd));

export const toggleDeviceMute = (d: Device) => run({ type: "setDeviceMute", id: d.id, muted: !d.muted });
export const toggleAppMute = (a: App) => run({ type: "setAppMute", key: a.key, muted: !a.muted });
export const toggleMicMute = (mic: Device) => run({ type: "setMicMute", muted: !mic.muted });
export const setProcessing = (processing: ProcessingSettings) => run({ type: "setProcessing", processing });
export const setAppSend = (app: App, level: number | null) => run({ type: "setAppSend", key: app.key, level });

/** Sends an app into calls (at the default level) or stops sending it; warns before sending a call app. */
export function toggleAppSend(app: App) {
  if (app.send === null && isCallApp(app)) useStore.getState().pushToast("info", ECHO_WARNING);
  return setAppSend(app, app.send === null ? DEFAULT_SEND : null);
}

/** Sends a headphone command; failures become toasts. */
export const runHeadset = (cmd: HeadsetCommand) => attempt(() => sendHeadset(cmd));

/** Registers the hotkey (validated by the backend) and persists it. */
export const saveMicConfig = (config: MicConfig) => attempt(() => setMicHotkey(config));

export const outputs = (s: UiState): Device[] =>
  s.devices.filter((d) => d.flow === "render" && d.state === "active" && !d.hidden);
export const inputs = (s: UiState): Device[] =>
  s.devices.filter((d) => d.flow === "capture" && d.state === "active" && !d.hidden);
export const defaultOutput = (s: UiState): Device | undefined =>
  s.devices.find((d) => d.flow === "render" && d.defaultConsole && d.state === "active");
export const defaultMic = (s: UiState): Device | undefined =>
  s.devices.find((d) => d.flow === "capture" && d.defaultConsole && d.state === "active");
