import { act, fireEvent, render, screen } from "@testing-library/react";
import { useStore } from "../lib/store";
import type { HeadsetState } from "../lib/types";
import { HeadphonesView } from "./HeadphonesView";
import { makeHeadset } from "../test/fixtures";
import * as api from "../lib/api";

vi.mock("../lib/api", () => ({ sendHeadset: vi.fn().mockResolvedValue(undefined) }));


function withHeadset<T>(headset: HeadsetState | null, ui: T): T {
  act(() => useStore.setState({ headset }));
  return ui;
}

describe("HeadphonesView", () => {
  beforeEach(() => vi.mocked(api.sendHeadset).mockClear());

  it("waits for the first state", () => {
    render(withHeadset(null, <HeadphonesView />));
    expect(screen.getByText(/connecting/i)).toBeInTheDocument();
  });

  it("switches ambient mode and manual noise-cancelling level", () => {
    render(withHeadset(makeHeadset(), <HeadphonesView />));
    fireEvent.click(screen.getByRole("radio", { name: "4" }));
    expect(api.sendHeadset).toHaveBeenCalledWith({ type: "setManualNc", level: 4 });
    fireEvent.click(screen.getByRole("radio", { name: "Adaptive" }));
    expect(api.sendHeadset).toHaveBeenCalledWith({ type: "setNcMode", mode: "adaptive" });
    fireEvent.click(screen.getByRole("radio", { name: "Transparency" }));
    expect(api.sendHeadset).toHaveBeenCalledWith({ type: "setAmbientMode", mode: "transparency" });
  });

  it("shows the adaptive level read-only", () => {
    render(withHeadset(makeHeadset({ ncMode: "adaptive" }), <HeadphonesView />));
    expect(screen.getByText("Adjusting automatically, currently 2 of 5")).toBeInTheDocument();
    expect(screen.queryByRole("radiogroup", { name: "Noise cancelling level" })).toBeNull();
  });

  it("offers transparency strength and wind-noise reduction in transparency mode", () => {
    render(withHeadset(makeHeadset({ ambientMode: "transparency" }), <HeadphonesView />));
    fireEvent.click(screen.getByRole("radio", { name: "2" }));
    expect(api.sendHeadset).toHaveBeenCalledWith({ type: "setTransparency", level: 2 });
    fireEvent.click(screen.getByRole("switch", { name: "Reduce wind noise" }));
    expect(api.sendHeadset).toHaveBeenCalledWith({ type: "setWindNoise", enabled: true });
  });

  it("hides strength controls in normal mode", () => {
    render(withHeadset(makeHeadset({ ambientMode: "normal" }), <HeadphonesView />));
    expect(screen.queryByRole("switch", { name: "Reduce wind noise" })).toBeNull();
    expect(screen.queryByRole("radio", { name: "1" })).toBeNull();
  });

  it("picks EQ presets and shows Custom for a custom curve", () => {
    const { rerender } = render(withHeadset(makeHeadset(), <HeadphonesView />));
    fireEvent.change(screen.getByRole("combobox", { name: "EQ preset" }), { target: { value: "Acoustic" } });
    expect(api.sendHeadset).toHaveBeenCalledWith({ type: "setEqPreset", preset: "Acoustic" });
    const custom = makeHeadset();
    custom.eq = { ...custom.eq!, preset: null };
    rerender(withHeadset(custom, <HeadphonesView />));
    expect(screen.getByRole("combobox", { name: "EQ preset" })).toHaveValue("");
    expect(screen.getByRole("option", { name: "Custom" })).toBeInTheDocument();
  });

  it("sends EQ band edits", () => {
    render(withHeadset(makeHeadset(), <HeadphonesView />));
    fireEvent.change(screen.getByRole("slider", { name: "100 Hz" }), { target: { value: "50" } });
    expect(api.sendHeadset).toHaveBeenCalledWith({ type: "setEqBands", values: [7, 0, 0, 0, 0, 0, 0, 0] });
  });

  it("shows battery and device details", () => {
    render(withHeadset(makeHeadset(), <HeadphonesView />));
    expect(screen.getByText("4 of 5")).toBeInTheDocument();
    expect(screen.getByText("06.87")).toBeInTheDocument();
    expect(screen.getByText("3035705028ABCDEF")).toBeInTheDocument();
  });

  it("disables controls and offers Retry when not connected", () => {
    render(withHeadset(makeHeadset({ connection: { state: "disconnected", retryInSecs: 10 } }), <HeadphonesView />));
    expect(screen.getByText("Not connected, retrying in 10 s")).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Transparency" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(api.sendHeadset).toHaveBeenCalledWith({ type: "retry" });
  });
});
