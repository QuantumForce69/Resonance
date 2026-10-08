import { amountCurve, autoMakeup, callReady, compressorCurve, isCallApp, isProcessed, pct, processingSources, sentApps, signedDb } from "./processing";
import { DEFAULT_DSP, makeApp, makeDevice, makeProcessing, makeState } from "../test/fixtures";

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

describe("call mix helpers", () => {
  it("is ready only when Windows supports it and processing runs", () => {
    expect(callReady(makeState())).toBe(false);
    const running = makeState({ processing: makeProcessing({ status: { state: "running" } }) });
    expect(callReady(running)).toBe(true);
    running.processing.callAvailable = false;
    expect(callReady(running)).toBe(false);
  });

  it("recognises call apps and lists sent apps", () => {
    expect(isCallApp(makeApp({ key: "discord.exe" }))).toBe(true);
    expect(isCallApp(makeApp({ key: "spotify.exe" }))).toBe(false);
    const s = makeState();
    s.apps[0] = { ...s.apps[0], send: 0.5 };
    expect(sentApps(s).map((a) => a.key)).toEqual(["spotify.exe"]);
  });

  it("formats levels", () => {
    expect(pct(0.7)).toBe("70%");
    expect(signedDb(-12)).toBe("-12 dB");
    expect(signedDb(3.25)).toBe("+3.3 dB");
  });
});
