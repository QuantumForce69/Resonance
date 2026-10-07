/** Lowest level shown on meters, in dBFS. */
export const MIN_DB = -60;

/** Converts a linear peak (0..1) to a 0..1 meter position on a dB scale. */
export function peakToPosition(peak: number): number {
  if (!(peak > 0)) return 0;
  const db = 20 * Math.log10(peak);
  return Math.min(1, Math.max(0, (db - MIN_DB) / -MIN_DB));
}

/** Fader curve shared by readouts and scale ticks: dB = 40·log10(volume). */
export function volumeToDb(volume: number): number {
  if (!(volume > 0)) return -Infinity;
  return 40 * Math.log10(Math.min(1, volume));
}

export function formatDb(volume: number): string {
  const db = volumeToDb(volume);
  if (db === -Infinity) return "−∞ dB";
  const text = Math.abs(db).toFixed(1);
  return db < -0.05 ? `−${text} dB` : `${text} dB`;
}

/** Fader position (0..1) that produces `db`. */
export function dbToFaderPosition(db: number): number {
  return Math.min(1, 10 ** (db / 40));
}
