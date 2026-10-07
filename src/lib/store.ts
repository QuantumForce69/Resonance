import { create } from "zustand";
import { send, setMicHotkey } from "./api";
import type { Command, Device, MicConfig, UiState } from "./types";

export interface Toast {
  id: number;
  kind: "error" | "info";
  message: string;
}

interface Store {
  state: UiState | null;
  toasts: Toast[];
  setState: (s: UiState) => void;
  pushToast: (kind: Toast["kind"], message: string) => void;
  dismissToast: (id: number) => void;
}

let nextToastId = 1;

export const useStore = create<Store>((set) => ({
  state: null,
  toasts: [],
  setState: (state) => set({ state }),
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

/** Sends a command; failures become toasts. */
export async function run(cmd: Command): Promise<boolean> {
  try {
    await send(cmd);
    return true;
  } catch (e) {
    useStore.getState().pushToast("error", errorMessage(e));
    return false;
  }
}

/** Registers the hotkey (validated by the backend) and persists it. */
export async function saveMicConfig(config: MicConfig): Promise<boolean> {
  try {
    await setMicHotkey(config);
    return true;
  } catch (e) {
    useStore.getState().pushToast("error", errorMessage(e));
    return false;
  }
}

export const outputs = (s: UiState): Device[] =>
  s.devices.filter((d) => d.flow === "render" && d.state === "active" && !d.hidden);
export const inputs = (s: UiState): Device[] =>
  s.devices.filter((d) => d.flow === "capture" && d.state === "active" && !d.hidden);
export const defaultOutput = (s: UiState): Device | undefined =>
  s.devices.find((d) => d.flow === "render" && d.defaultConsole && d.state === "active");
export const defaultMic = (s: UiState): Device | undefined =>
  s.devices.find((d) => d.flow === "capture" && d.defaultConsole && d.state === "active");
