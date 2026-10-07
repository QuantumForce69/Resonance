import { peakToPosition } from "./levels";
import type { MeterFrame } from "./types";

export const PEAK_HOLD_MS = 1200;
const DECAY = 0.12;
const PEAK_FALL = 0.015;

export interface MeterState {
  level: number;
  peak: number;
  peakAt: number;
}

/** One animation step: instant attack, exponential release, held peak marker. */
export function stepMeter(s: MeterState, target: number, now: number): MeterState {
  const level = target >= s.level ? target : s.level + (target - s.level) * DECAY;
  if (target >= s.peak) return { level, peak: target, peakAt: now };
  if (now - s.peakAt > PEAK_HOLD_MS) return { level, peak: Math.max(level, s.peak - PEAK_FALL), peakAt: s.peakAt };
  return { level, peak: s.peak, peakAt: s.peakAt };
}

const latest = new Map<string, number>();

/** Sources: "master", "mic", "app:<key>". */
export function ingestFrame(f: MeterFrame) {
  latest.set("master", f.master);
  latest.set("mic", f.mic);
  for (const k of latest.keys()) {
    if (k.startsWith("app:") && !(k.slice(4) in f.apps)) latest.set(k, 0);
  }
  for (const [k, v] of Object.entries(f.apps)) latest.set(`app:${k}`, v);
}

export function readLevel(source: string): number {
  return latest.get(source) ?? 0;
}

interface Registration {
  source: string;
  vertical: boolean;
  fill: HTMLElement;
  peak: HTMLElement;
  state: MeterState;
}

const registrations = new Set<Registration>();
let raf = 0;

function tick(now: number) {
  for (const r of registrations) {
    r.state = stepMeter(r.state, peakToPosition(readLevel(r.source)), now);
    const lv = r.state.level * 100;
    const pk = r.state.peak * 100;
    r.peak.style.opacity = pk > 1 ? "0.8" : "0";
    if (r.vertical) {
      r.fill.style.clipPath = `inset(${100 - lv}% 0 0 0)`;
      r.peak.style.bottom = `${pk}%`;
    } else {
      r.fill.style.clipPath = `inset(0 ${100 - lv}% 0 0)`;
      r.peak.style.left = `calc(${pk}% - 1px)`;
    }
  }
  raf = registrations.size ? requestAnimationFrame(tick) : 0;
}

/** Drives a meter's DOM directly (no React renders). Returns an unregister function. */
export function registerMeter(source: string, vertical: boolean, fill: HTMLElement, peak: HTMLElement): () => void {
  const r: Registration = { source, vertical, fill, peak, state: { level: 0, peak: 0, peakAt: 0 } };
  registrations.add(r);
  if (!raf) raf = requestAnimationFrame(tick);
  return () => {
    registrations.delete(r);
  };
}
