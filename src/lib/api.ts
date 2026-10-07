import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { CmdError, Command, MeterFrame, MicConfig, UiState } from "./types";

export const send = (cmd: Command) => invoke<void>("command", { cmd });
export const getState = () => invoke<UiState>("get_state");
export const getAppIcon = (exePath: string) => invoke<string | null>("get_app_icon", { exePath });
export const setMicHotkey = (config: MicConfig) => invoke<void>("set_mic_hotkey", { config });
export const takeNotices = () => invoke<CmdError[]>("take_notices");
export const showMain = () => invoke<void>("show_main");

export const onState = (cb: (s: UiState) => void): Promise<UnlistenFn> => listen<UiState>("state", (e) => cb(e.payload));
export const onMeters = (cb: (m: MeterFrame) => void): Promise<UnlistenFn> =>
  listen<MeterFrame>("meters", (e) => cb(e.payload));
export const onNotice = (cb: (e: CmdError) => void): Promise<UnlistenFn> => listen<CmdError>("notice", (e) => cb(e.payload));

import type { HeadsetCommand, HeadsetState } from "./types";

export const sendHeadset = (cmd: HeadsetCommand) => invoke<void>("headset_command", { cmd });
export const getHeadset = () => invoke<HeadsetState>("get_headset");
export const onHeadset = (cb: (h: HeadsetState) => void): Promise<UnlistenFn> =>
  listen<HeadsetState>("headset", (e) => cb(e.payload));
