import type { CompressorParams, Device, UiState } from "./types";

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
