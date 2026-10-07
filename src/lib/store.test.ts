import { defaultMic, defaultOutput, errorMessage, inputs, outputs, run, saveMicConfig, useStore } from "./store";
import { makeDevice, makeState } from "../test/fixtures";
import * as api from "./api";

vi.mock("./api", () => ({
  send: vi.fn(),
  setMicHotkey: vi.fn(),
  sendHeadset: vi.fn(),
}));

describe("selectors", () => {
  it("lists active, visible outputs and inputs and finds defaults", () => {
    const s = makeState();
    s.devices.push(makeDevice({ id: "hid", hidden: true }));
    expect(outputs(s).map((d) => d.id)).toEqual(["out1", "out2"]);
    expect(inputs(s).map((d) => d.id)).toEqual(["mic1"]);
    expect(defaultOutput(s)?.id).toBe("out1");
    expect(defaultMic(s)?.id).toBe("mic1");
  });
});

describe("run", () => {
  beforeEach(() => useStore.setState({ toasts: [] }));

  it("returns true on success", async () => {
    vi.mocked(api.send).mockResolvedValueOnce(undefined);
    expect(await run({ type: "toggleMicMute" })).toBe(true);
    expect(useStore.getState().toasts).toEqual([]);
  });

  it("shows the backend error message as a toast on failure", async () => {
    vi.mocked(api.send).mockRejectedValueOnce({ code: "notFound", message: "That device is no longer connected" });
    expect(await run({ type: "toggleMicMute" })).toBe(false);
    expect(useStore.getState().toasts.map((t) => t.message)).toEqual(["That device is no longer connected"]);
  });

  it("saveMicConfig toasts hotkey registration errors", async () => {
    vi.mocked(api.setMicHotkey).mockRejectedValueOnce({ code: "invalid", message: "Couldn't register" });
    expect(await saveMicConfig({ hotkey: "Ctrl+F1", mode: "toggle" })).toBe(false);
    expect(useStore.getState().toasts.at(-1)?.message).toBe("Couldn't register");
  });
});

describe("toasts", () => {
  it("dedupes identical messages and keeps at most 4", () => {
    useStore.setState({ toasts: [] });
    const { pushToast } = useStore.getState();
    pushToast("error", "same");
    pushToast("error", "same");
    expect(useStore.getState().toasts).toHaveLength(1);
    for (let i = 0; i < 6; i++) pushToast("info", `m${i}`);
    expect(useStore.getState().toasts).toHaveLength(4);
  });

  it("errorMessage handles strings, error objects and junk", () => {
    expect(errorMessage("x")).toBe("x");
    expect(errorMessage(new Error("boom"))).toBe("boom");
    expect(errorMessage(42)).toBe("Something went wrong");
  });
});

import { runHeadset } from "./store";
import { makeHeadset } from "../test/fixtures";

describe("runHeadset", () => {
  beforeEach(() => useStore.setState({ toasts: [] }));

  it("toasts headset errors", async () => {
    vi.mocked(api.sendHeadset).mockRejectedValueOnce({ code: "unavailable", message: "Space One isn't connected" });
    expect(await runHeadset({ type: "retry" })).toBe(false);
    expect(useStore.getState().toasts.at(-1)?.message).toBe("Space One isn't connected");
  });

  it("stores the latest headset state", () => {
    useStore.getState().setHeadset(makeHeadset({ name: "x" }));
    expect(useStore.getState().headset?.name).toBe("x");
  });
});
