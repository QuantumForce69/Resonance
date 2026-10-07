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
