/** Lowest level shown on meters, in dBFS. */
export const MIN_DB = -60;

/** Converts a linear peak (0..1) to a 0..1 meter position on a dB scale. */
export function peakToPosition(peak: number): number {
  if (!(peak > 0)) return 0;
  const db = 20 * Math.log10(peak);
  return Math.min(1, Math.max(0, (db - MIN_DB) / -MIN_DB));
}
