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

export interface Meter {
  source: string;
  vertical: boolean;
  fill: HTMLElement;
  peak: HTMLElement;
  state: MeterState;
  /** What was last written to the page, so unchanged frames cost nothing. */
  painted: string;
}

/** Meter data arrives at 30 Hz; animating faster than 60 fps (e.g. on a 240 Hz monitor) only burns CPU. */
const FRAME_MS = 1000 / 60;

const meters = new Set<Meter>();
let raf = 0;
let lastFrame = -Infinity;

export function createMeter(source: string, vertical: boolean, fill: HTMLElement, peak: HTMLElement): Meter {
  return { source, vertical, fill, peak, state: { level: 0, peak: 0, peakAt: 0 }, painted: "" };
}

/** Advances one meter and writes to the page only if what it shows changed (0.1% steps). */
export function paintMeter(m: Meter, now: number): boolean {
  m.state = stepMeter(m.state, peakToPosition(readLevel(m.source)), now);
  const lv = Math.round(m.state.level * 1000) / 10;
  const pk = Math.round(m.state.peak * 1000) / 10;
  const key = `${lv}|${pk}`;
  if (key === m.painted) return false;
  m.painted = key;
  m.peak.style.opacity = pk > 1 ? "0.8" : "0";
  if (m.vertical) {
    m.fill.style.clipPath = `inset(${100 - lv}% 0 0 0)`;
    m.peak.style.bottom = `${pk}%`;
  } else {
    m.fill.style.clipPath = `inset(0 ${100 - lv}% 0 0)`;
    m.peak.style.left = `calc(${pk}% - 1px)`;
  }
  return true;
}

function tick(now: number) {
  if (now - lastFrame >= FRAME_MS - 1) {
    lastFrame = now;
    for (const m of meters) paintMeter(m, now);
  }
  raf = meters.size ? requestAnimationFrame(tick) : 0;
}

/** Drives a meter's DOM directly (no React renders). Returns an unregister function. */
export function registerMeter(source: string, vertical: boolean, fill: HTMLElement, peak: HTMLElement): () => void {
  const r = createMeter(source, vertical, fill, peak);
  meters.add(r);
  if (!raf) raf = requestAnimationFrame(tick);
  return () => {
    meters.delete(r);
  };
}
