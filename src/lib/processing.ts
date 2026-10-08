import type { App, CompressorParams, Device, UiState } from "./types";

const CABLE_CAPTURE = "CABLE Output";

/** Threshold and ratio for the compressor's Amount control (mirrors the backend). */
export function amountCurve(amount: number) {
  const a = Math.min(1, Math.max(0, amount));
  return { thresholdDb: -10 - 25 * a, ratio: 1.5 + 4.5 * a };
}

export function compressorCurve(c: CompressorParams) {
  return c.custom ? { thresholdDb: c.thresholdDb, ratio: c.ratio } : amountCurve(c.amount);
}

/** Automatic makeup: half the gain reduction a −18 dBFS signal receives. */
export function autoMakeup(c: CompressorParams) {
  const { thresholdDb, ratio } = compressorCurve(c);
  return (Math.max(0, -18 - thresholdDb) * (1 - 1 / ratio)) / 2;
}

/** Mics that can be processed: every active input except VB-Cable's own output. */
export const processingSources = (s: UiState): Device[] =>
  s.devices.filter((d) => d.flow === "capture" && d.state === "active" && !d.originalName.startsWith(CABLE_CAPTURE));

export const isProcessed = (s: UiState) => s.processing.status.state === "running";

const CALL_APPS = ["discord.exe", "teams.exe", "ms-teams.exe", "zoom.exe", "skype.exe", "slack.exe"];
export const DEFAULT_SEND = 0.7;
export const ECHO_WARNING = "Sending a call app into the call makes everyone hear themselves.";
export const CALL_OFF_HINT = "Turn on Processing to send apps into calls";

/** Apps can be sent right now: Windows can capture single apps and processing is running. */
export const callReady = (s: UiState) => s.processing.callAvailable && isProcessed(s);
export const isCallApp = (a: App) => CALL_APPS.includes(a.key);
export const sentApps = (s: UiState) => s.apps.filter((a) => a.send !== null);

const round1 = (v: number) => Math.round(v * 10) / 10;
export const pct = (v: number) => `${Math.round(v * 100)}%`;
export const signedDb = (v: number) => `${v > 0 ? "+" : ""}${round1(v)} dB`;
