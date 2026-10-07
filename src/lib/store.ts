import { create } from "zustand";
import { send, sendHeadset, setMicHotkey } from "./api";
import type { Command, Device, HeadsetCommand, HeadsetState, MicConfig, UiState } from "./types";

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
