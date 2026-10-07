import { AMBIENT_OPTIONS, LEVEL_OPTIONS, connectionLabel, isConnected } from "./headset";
import { makeHeadset } from "../test/fixtures";

describe("headset helpers", () => {
  it("knows when the headphones are usable", () => {
    expect(isConnected(makeHeadset())).toBe(true);
    expect(isConnected(makeHeadset({ connection: { state: "connecting" } }))).toBe(false);
    expect(isConnected(null)).toBe(false);
  });

  it("describes every connection state in plain words", () => {
    expect(connectionLabel({ state: "connected" })).toBe("Connected");
    expect(connectionLabel({ state: "connecting" })).toBe("Connecting…");
    expect(connectionLabel({ state: "notFound" })).toBe("Not found. Switch it on and pair it with this PC");
    expect(connectionLabel({ state: "disconnected", retryInSecs: 10 })).toBe("Not connected, retrying in 10 s");
  });

  it("offers the three ambient modes and five levels", () => {
    expect(AMBIENT_OPTIONS.map((o) => o.label)).toEqual(["Noise cancelling", "Transparency", "Normal"]);
    expect(LEVEL_OPTIONS.map((o) => o.value)).toEqual([1, 2, 3, 4, 5]);
  });
});
