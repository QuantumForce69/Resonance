import { fireEvent, render, screen } from "@testing-library/react";
import { Onboarding } from "./Onboarding";
import { makeDevice, makeProcessing, makeState } from "../test/fixtures";
import * as api from "../lib/api";

vi.mock("../lib/api", () => ({
  send: vi.fn().mockResolvedValue(undefined),
  openVbCable: vi.fn().mockResolvedValue(undefined),
  getAutostart: vi.fn().mockResolvedValue(false),
  setAutostart: vi.fn().mockResolvedValue(undefined),
}));

const fresh = (cable = true) =>
  makeState({ ui: { theme: "system", startMinimized: false, autoRestartAudio: false, onboarded: false }, processing: makeProcessing({ cableInstalled: cable }) });

const next = () => fireEvent.click(screen.getByRole("button", { name: "Next" }));

describe("Onboarding", () => {
  beforeEach(() => vi.clearAllMocks());

  it("sets the chosen mic for apps and calls", () => {
    const s = fresh();
    s.devices.push(makeDevice({ id: "mic2", name: "Headset Mic", flow: "capture" }));
    render(<Onboarding state={s} />);
    expect(screen.getByRole("dialog", { name: "Welcome to Resonance" })).toBeInTheDocument();
    fireEvent.change(screen.getByRole("combobox", { name: "Your microphone" }), { target: { value: "mic2" } });
    expect(api.send).toHaveBeenCalledWith({ type: "setDefaultDevice", id: "mic2", role: "console" });
    expect(api.send).toHaveBeenCalledWith({ type: "setDefaultDevice", id: "mic2", role: "communications" });
  });

  it("offers VB-Cable when it's missing and hides the processing switch", () => {
    render(<Onboarding state={fresh(false)} />);
    next();
    fireEvent.click(screen.getByRole("button", { name: "Get VB-Cable" }));
    expect(api.openVbCable).toHaveBeenCalled();
    next();
    expect(screen.queryByRole("switch", { name: "Turn on Processing" })).toBeNull();
    expect(screen.getByRole("switch", { name: "Start with Windows" })).toBeInTheDocument();
  });

  it("turns on processing and finishes", () => {
    const s = fresh();
    render(<Onboarding state={s} />);
    next();
    expect(screen.getByText(/vb-cable is installed/i)).toBeInTheDocument();
    next();
    fireEvent.click(screen.getByRole("switch", { name: "Turn on Processing" }));
    expect(api.send).toHaveBeenCalledWith({ type: "setProcessing", processing: { ...s.processing.settings, enabled: true } });
    fireEvent.click(screen.getByRole("button", { name: "Done" }));
    expect(api.send).toHaveBeenLastCalledWith({ type: "setUiSettings", ui: { ...s.ui, onboarded: true } });
  });

  it("can be skipped", () => {
    const s = fresh();
    render(<Onboarding state={s} />);
    fireEvent.click(screen.getByRole("button", { name: "Skip" }));
    expect(api.send).toHaveBeenCalledWith({ type: "setUiSettings", ui: { ...s.ui, onboarded: true } });
  });
});
