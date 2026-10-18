import type { App, Device, DspParams, ProcessingView, Profile, UiState } from "../lib/types";

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
    send: null,
    ...p,
  };
}

export const DEFAULT_DSP: DspParams = {
  denoise: { enabled: true, strength: 1 },
  gate: { enabled: true, thresholdDb: -45, attackMs: 2, holdMs: 150, releaseMs: 120, rangeDb: -60 },
  compressor: { enabled: true, amount: 0.5, custom: false, thresholdDb: -22.5, ratio: 3.75, attackMs: 5, releaseMs: 80, makeupDb: null },
  limiter: { enabled: true, ceilingDb: -1, releaseMs: 50 },
};

export function makeProcessing(p: Partial<ProcessingView> = {}): ProcessingView {
  return {
    settings: { enabled: false, sourceId: null, dsp: structuredClone(DEFAULT_DSP), monitor: false, monitorLevel: 0.7, call: { master: 1, duck: true, duckDb: -12 } },
    status: { state: "off" },
    cableInstalled: true,
    callAvailable: true,
    ...p,
  };
}

export function makeProfile(p: Partial<Profile> & { id: string }): Profile {
  return {
    name: `Profile ${p.id}`,
    parts: { apps: {}, processing: null, headphones: null, defaultOutput: null },
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
    ui: { theme: "system", startMinimized: false, autoRestartAudio: false, onboarded: true },
    processing: makeProcessing(),
    profiles: [],
    activeProfile: null,
    ...p,
  };
}

import type { HeadsetState } from "../lib/types";

export function makeHeadset(p: Partial<HeadsetState> = {}): HeadsetState {
  return {
    connection: { state: "connected" },
    name: "soundcore Space One",
    ambientMode: "noiseCanceling",
    ncMode: "manual",
    manualNc: 3,
    adaptiveNc: 2,
    transparency: 4,
    windNoise: false,
    battery: { level: 4, max: 5, charging: null },
    eq: {
      presets: [
        { id: "SoundcoreSignature", name: "Soundcore Signature" },
        { id: "Acoustic", name: "Acoustic" },
      ],
      preset: "SoundcoreSignature",
      bandHz: [100, 200, 400, 800, 1600, 3200, 6400, 12800],
      values: [0, 0, 0, 0, 0, 0, 0, 0],
      min: -120,
      max: 134,
      fractionDigits: 1,
    },
    firmware: "06.87",
    serial: "3035705028ABCDEF",
    ...p,
  };
}
