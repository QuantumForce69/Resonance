import { partLabels } from "./profiles";
import { DEFAULT_DSP, makeProfile } from "../test/fixtures";

describe("partLabels", () => {
  it("names the parts a profile controls, in a fixed order", () => {
    expect(partLabels(makeProfile({ id: "p1" }))).toEqual(["Apps"]);
    const all = makeProfile({
      id: "p2",
      parts: {
        apps: null,
        processing: {
          settings: { enabled: true, sourceId: null, dsp: DEFAULT_DSP, monitor: false, monitorLevel: 0.7, call: { master: 1, duck: true, duckDb: -12 } },
          callSends: {},
        },
        headphones: { ambientMode: "transparency", ncMode: null, manualNc: null, transparency: 3, windNoise: null, eqPreset: null, eqBands: null },
        defaultOutput: "out1",
      },
    });
    expect(partLabels(all)).toEqual(["Mic & processing", "Headphones", "Default output"]);
  });
});
