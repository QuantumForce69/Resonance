import { amountCurve, autoMakeup, compressorCurve, isProcessed, processingSources } from "./processing";
import { DEFAULT_DSP, makeDevice, makeProcessing, makeState } from "../test/fixtures";

describe("processing helpers", () => {
  it("maps the compressor amount like the backend", () => {
    expect(amountCurve(0)).toEqual({ thresholdDb: -10, ratio: 1.5 });
    expect(amountCurve(1)).toEqual({ thresholdDb: -35, ratio: 6 });
    expect(compressorCurve(DEFAULT_DSP.compressor)).toEqual({ thresholdDb: -22.5, ratio: 3.75 });
  });

  it("uses a custom threshold and ratio, and derives auto makeup from them", () => {
    const c = { ...DEFAULT_DSP.compressor, custom: true, thresholdDb: -30, ratio: 4 };
    expect(compressorCurve(c)).toEqual({ thresholdDb: -30, ratio: 4 });
    expect(autoMakeup(c)).toBeCloseTo(4.5);
  });

  it("never offers VB-Cable as a source", () => {
    const s = makeState();
    const name = "CABLE Output (VB-Audio Virtual Cable)";
    s.devices.push(makeDevice({ id: "cout", name, originalName: name, flow: "capture" }));
    expect(processingSources(s).map((d) => d.id)).toEqual(["mic1"]);
  });

  it("knows when processing is running", () => {
    expect(isProcessed(makeState())).toBe(false);
    expect(isProcessed(makeState({ processing: makeProcessing({ status: { state: "running" } }) }))).toBe(true);
  });
});
