import { ingestFrame, readLevel, stepMeter, PEAK_HOLD_MS } from "./meters";

describe("stepMeter", () => {
  const zero = { level: 0, peak: 0, peakAt: 0 };

  it("attacks instantly and records the peak", () => {
    const s = stepMeter(zero, 0.8, 1000);
    expect(s).toEqual({ level: 0.8, peak: 0.8, peakAt: 1000 });
  });

  it("decays smoothly instead of dropping", () => {
    const s = stepMeter({ level: 0.8, peak: 0.8, peakAt: 1000 }, 0, 1016);
    expect(s.level).toBeLessThan(0.8);
    expect(s.level).toBeGreaterThan(0.5);
  });

  it("holds the peak, then lets it fall", () => {
    const held = stepMeter({ level: 0.2, peak: 0.9, peakAt: 1000 }, 0.2, 1000 + PEAK_HOLD_MS - 1);
    expect(held.peak).toBe(0.9);
    const falling = stepMeter({ level: 0.2, peak: 0.9, peakAt: 1000 }, 0.2, 1000 + PEAK_HOLD_MS + 1);
    expect(falling.peak).toBeLessThan(0.9);
    expect(falling.peak).toBeGreaterThanOrEqual(falling.level);
  });
});

describe("ingestFrame", () => {
  it("stores master, mic and per-app values and zeroes apps that disappear", () => {
    ingestFrame({ apps: { "a.exe": 0.5, "b.exe": 0.25 }, master: 0.7, mic: 0.1 });
    expect(readLevel("master")).toBe(0.7);
    expect(readLevel("mic")).toBe(0.1);
    expect(readLevel("app:a.exe")).toBe(0.5);
    ingestFrame({ apps: { "b.exe": 0.3 }, master: 0, mic: 0 });
    expect(readLevel("app:a.exe")).toBe(0);
    expect(readLevel("app:b.exe")).toBe(0.3);
    expect(readLevel("app:never")).toBe(0);
  });
});
