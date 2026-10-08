import { fireEvent, render, screen, within } from "@testing-library/react";
import { ProcessingPanel } from "./ProcessingPanel";
import { makeDevice, makeProcessing, makeState } from "../test/fixtures";
import * as api from "../lib/api";

vi.mock("../lib/api", () => ({
  send: vi.fn().mockResolvedValue(undefined),
  openVbCable: vi.fn().mockResolvedValue(undefined),
  getAppIcon: vi.fn().mockResolvedValue(null),
}));

/** The settings object sent by the last setProcessing command. */
function lastSent() {
  const cmd = vi.mocked(api.send).mock.calls.at(-1)![0];
  if (cmd.type !== "setProcessing") throw new Error(`unexpected ${cmd.type}`);
  return cmd.processing;
}

const stage = (name: string) => screen.getByRole("region", { name });

describe("ProcessingPanel", () => {
  beforeEach(() => {
    vi.mocked(api.send).mockClear();
    vi.mocked(api.openVbCable).mockClear();
  });

  it("offers VB-Cable when it isn't installed", () => {
    render(<ProcessingPanel state={makeState({ processing: makeProcessing({ cableInstalled: false }) })} />);
    expect(screen.queryByRole("slider")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Get VB-Cable" }));
    expect(api.openVbCable).toHaveBeenCalled();
  });

  it("turns processing on", () => {
    const s = makeState();
    render(<ProcessingPanel state={s} />);
    fireEvent.click(screen.getByRole("button", { name: "Turn processing on" }));
    expect(lastSent()).toEqual({ ...s.processing.settings, enabled: true });
  });

  it("says what apps hear, and why processing is paused", () => {
    const { rerender } = render(<ProcessingPanel state={makeState({ processing: makeProcessing({ status: { state: "running" } }) })} />);
    expect(screen.getByRole("status")).toHaveTextContent(/apps hear your processed mic/i);
    rerender(<ProcessingPanel state={makeState({ processing: makeProcessing({ status: { state: "paused", reason: "Desk Mic was unplugged" } }) })} />);
    expect(screen.getByRole("status")).toHaveTextContent("Desk Mic was unplugged");
  });

  it("adjusts the gate threshold and turns a stage off", () => {
    render(<ProcessingPanel state={makeState()} />);
    fireEvent.keyDown(within(stage("Noise gate")).getByRole("slider", { name: "Threshold" }), { key: "ArrowUp" });
    expect(lastSent().dsp.gate.thresholdDb).toBe(-44);
    fireEvent.click(within(stage("Noise suppression")).getByRole("switch", { name: "Noise suppression" }));
    expect(lastSent().dsp.denoise.enabled).toBe(false);
  });

  it("sets compressor threshold and ratio from the amount", () => {
    render(<ProcessingPanel state={makeState()} />);
    fireEvent.keyDown(within(stage("Compressor")).getByRole("slider", { name: "Amount" }), { key: "ArrowUp" });
    const c = lastSent().dsp.compressor;
    expect(c.amount).toBeCloseTo(0.55);
    expect(c.custom).toBe(false);
    expect(c.thresholdDb).toBeCloseTo(-23.75);
    expect(c.ratio).toBeCloseTo(3.975);
  });

  it("reveals advanced controls; editing the threshold makes the compressor custom", () => {
    render(<ProcessingPanel state={makeState()} />);
    const comp = stage("Compressor");
    expect(within(comp).queryByRole("slider", { name: "Ratio" })).toBeNull();
    fireEvent.click(within(comp).getByRole("button", { name: "Advanced" }));
    expect(within(comp).getByRole("slider", { name: "Ratio" })).toHaveAttribute("aria-valuetext", "3.8:1");
    fireEvent.keyDown(within(comp).getByRole("slider", { name: "Threshold" }), { key: "ArrowUp" });
    expect(lastSent().dsp.compressor).toMatchObject({ custom: true, thresholdDb: -22, ratio: 3.75 });
  });

  it("switches makeup between automatic and manual", () => {
    render(<ProcessingPanel state={makeState()} />);
    const comp = stage("Compressor");
    fireEvent.click(within(comp).getByRole("button", { name: "Advanced" }));
    const auto = within(comp).getByRole("button", { name: "Auto" });
    expect(auto).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(auto);
    expect(lastSent().dsp.compressor.makeupDb).toBe(1.5);
  });

  it("turns on monitoring", () => {
    render(<ProcessingPanel state={makeState()} />);
    fireEvent.click(screen.getByRole("switch", { name: "Hear yourself" }));
    expect(lastSent().monitor).toBe(true);
  });

  it("picks a source but never offers CABLE Output", () => {
    const s = makeState();
    const name = "CABLE Output (VB-Audio Virtual Cable)";
    s.devices.push(makeDevice({ id: "cout", name, originalName: name, flow: "capture" }));
    render(<ProcessingPanel state={s} />);
    const source = screen.getByRole("combobox", { name: "Source" });
    expect(within(source).queryByRole("option", { name })).toBeNull();
    fireEvent.change(source, { target: { value: "mic1" } });
    expect(lastSent().sourceId).toBe("mic1");
  });
});
