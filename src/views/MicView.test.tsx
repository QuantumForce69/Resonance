import { fireEvent, render, screen } from "@testing-library/react";
import { MicView } from "./MicView";
import { makeDevice, makeProcessing, makeState } from "../test/fixtures";
import * as api from "../lib/api";

vi.mock("../lib/api", () => ({
  send: vi.fn().mockResolvedValue(undefined),
  setMicHotkey: vi.fn().mockResolvedValue(undefined),
  openVbCable: vi.fn().mockResolvedValue(undefined),
  getAppIcon: vi.fn().mockResolvedValue(null),
}));

describe("MicView", () => {
  it("shows the processing section and says when apps hear the processed mic", () => {
    render(<MicView state={makeState({ processing: makeProcessing({ status: { state: "running" } }) })} />);
    expect(screen.getByRole("region", { name: "Processing" })).toBeInTheDocument();
    expect(screen.getByText(/apps hear your mic through resonance's processing/i)).toBeInTheDocument();
  });

  beforeEach(() => {
    vi.mocked(api.send).mockClear();
    vi.mocked(api.setMicHotkey).mockClear();
  });

  it("explains when there is no microphone", () => {
    const s = makeState();
    s.devices = s.devices.filter((d) => d.flow === "render");
    render(<MicView state={s} />);
    expect(screen.getByText(/no microphone found/i)).toBeInTheDocument();
  });

  it("mutes the mic and shows the muted state", () => {
    const s = makeState();
    const { rerender } = render(<MicView state={s} />);
    fireEvent.click(screen.getByRole("button", { name: "Mute microphone" }));
    expect(api.send).toHaveBeenCalledWith({ type: "setMicMute", muted: true });
    s.devices = s.devices.map((d) => (d.id === "mic1" ? { ...d, muted: true } : d));
    rerender(<MicView state={{ ...s }} />);
    expect(screen.getByText(/muted, apps can't hear you/i)).toBeInTheDocument();
  });

  it("turns the boost knob to a detent", () => {
    const s = makeState();
    s.devices = s.devices.map((d) => (d.id === "mic1" ? { ...d, boost: { db: 10, min: 0, max: 30, step: 10 } } : d));
    render(<MicView state={s} />);
    expect(screen.getByRole("radio", { name: "+10 dB" })).toHaveAttribute("aria-checked", "true");
    fireEvent.click(screen.getByRole("radio", { name: "+20 dB" }));
    expect(api.send).toHaveBeenCalledWith({ type: "setMicBoost", id: "mic1", db: 20 });
  });

  it("changes shortcut behaviour and records a new shortcut", () => {
    render(<MicView state={makeState()} />);
    fireEvent.click(screen.getByRole("radio", { name: "Push to talk" }));
    expect(api.setMicHotkey).toHaveBeenCalledWith({ hotkey: "Ctrl+Shift+M", mode: "pushToTalk" });
    fireEvent.click(screen.getByRole("button", { name: /mute shortcut is ctrl/i }));
    fireEvent.keyDown(window, { code: "F13", key: "F13" });
    expect(api.setMicHotkey).toHaveBeenLastCalledWith({ hotkey: "F13", mode: "toggle" });
  });

  it("switches the default microphone", () => {
    const s = makeState();
    s.devices.push(makeDevice({ id: "mic2", name: "Headset Mic", flow: "capture" }));
    render(<MicView state={s} />);
    fireEvent.change(screen.getByRole("combobox", { name: "Default microphone" }), { target: { value: "mic2" } });
    expect(api.send).toHaveBeenCalledWith({ type: "setDefaultDevice", id: "mic2", role: "console" });
  });
});
