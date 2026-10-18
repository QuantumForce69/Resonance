import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { CmdError, Command, MeterFrame, MicConfig, PartChoice, UiState } from "./types";

export const send = (cmd: Command) => invoke<void>("command", { cmd });
export const getState = () => invoke<UiState>("get_state");
export const getAppIcon = (exePath: string) => invoke<string | null>("get_app_icon", { exePath });
export const setMicHotkey = (config: MicConfig) => invoke<void>("set_mic_hotkey", { config });
export const takeNotices = () => invoke<CmdError[]>("take_notices");
export const showMain = () => invoke<void>("show_main");
export const openVbCable = () => invoke<void>("open_vb_cable");
export const saveProfile = (name: string, parts: PartChoice) => invoke<void>("save_profile", { name, parts });
export const updateProfile = (id: string) => invoke<void>("update_profile", { id });
export const applyProfile = (id: string) => invoke<void>("apply_profile", { id });
export const getAutostart = () => invoke<boolean>("get_autostart");
export const setAutostart = (enabled: boolean) => invoke<void>("set_autostart", { enabled });

export const onState = (cb: (s: UiState) => void): Promise<UnlistenFn> => listen<UiState>("state", (e) => cb(e.payload));
export const onMeters = (cb: (m: MeterFrame) => void): Promise<UnlistenFn> =>
  listen<MeterFrame>("meters", (e) => cb(e.payload));
export const onNotice = (cb: (e: CmdError) => void): Promise<UnlistenFn> => listen<CmdError>("notice", (e) => cb(e.payload));

import type { HeadsetCommand, HeadsetState } from "./types";

export const sendHeadset = (cmd: HeadsetCommand) => invoke<void>("headset_command", { cmd });
export const getHeadset = () => invoke<HeadsetState>("get_headset");
export const onHeadset = (cb: (h: HeadsetState) => void): Promise<UnlistenFn> =>
  listen<HeadsetState>("headset", (e) => cb(e.payload));

export const restartAudio = () => invoke<void>("restart_audio");
