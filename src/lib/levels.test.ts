import { peakToPosition, MIN_DB } from "./levels";

describe("peakToPosition", () => {
  it("maps silence and negatives to 0", () => {
    expect(peakToPosition(0)).toBe(0);
    expect(peakToPosition(-1)).toBe(0);
  });
  it("maps full scale to 1 and clamps above", () => {
    expect(peakToPosition(1)).toBe(1);
    expect(peakToPosition(2)).toBe(1);
  });
  it("maps -30 dB to the midpoint of a 60 dB range", () => {
    expect(MIN_DB).toBe(-60);
    expect(peakToPosition(10 ** (-30 / 20))).toBeCloseTo(0.5, 5);
  });
  it("maps anything below MIN_DB to 0", () => {
    expect(peakToPosition(10 ** (-80 / 20))).toBe(0);
  });
});

import { dbToFaderPosition, formatDb, volumeToDb } from "./levels";

describe("fader dB helpers", () => {
  it("maps volume to dB on a 40·log10 curve", () => {
    expect(volumeToDb(1)).toBe(0);
    expect(volumeToDb(0.62)).toBeCloseTo(-8.3, 1);
    expect(volumeToDb(0)).toBe(-Infinity);
  });

  it("formats readouts with a real minus sign and one decimal", () => {
    expect(formatDb(1)).toBe("0.0 dB");
    expect(formatDb(0.62)).toBe("−8.3 dB");
    expect(formatDb(0)).toBe("−∞ dB");
  });

  it("places scale ticks where the fader produces that dB", () => {
    expect(dbToFaderPosition(0)).toBe(1);
    expect(dbToFaderPosition(-10)).toBeCloseTo(0.562, 3);
    expect(volumeToDb(dbToFaderPosition(-20))).toBeCloseTo(-20, 5);
  });
});
