import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { SettingsView } from "./SettingsView";
import { makeState } from "../test/fixtures";
import * as api from "../lib/api";

vi.mock("../lib/api", () => ({
  send: vi.fn().mockResolvedValue(undefined),
  getAutostart: vi.fn().mockResolvedValue(false),
  setAutostart: vi.fn().mockResolvedValue(undefined),
  restartAudio: vi.fn(() => Promise.resolve()),
}));

vi.mock("@tauri-apps/api/app", () => ({ getVersion: () => Promise.resolve("0.2.0") }));

describe("SettingsView", () => {
  it("shows and changes Start with Windows", async () => {
    render(<SettingsView state={makeState()} />);
    const toggle = screen.getByRole("switch", { name: "Start with Windows" });
    await waitFor(() => expect(toggle).toBeEnabled());
    expect(toggle).toHaveAttribute("aria-checked", "false");
    fireEvent.click(toggle);
    expect(api.setAutostart).toHaveBeenCalledWith(true);
    expect(toggle).toHaveAttribute("aria-checked", "true");
  });

  it("restarts audio from the button and shows progress", async () => {
    let finish!: () => void;
    vi.mocked(api.restartAudio).mockImplementationOnce(() => new Promise<void>((r) => (finish = r)));
    render(<SettingsView state={makeState()} />);
    fireEvent.click(screen.getByRole("button", { name: "Restart audio" }));
    expect(api.restartAudio).toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Restarting…" })).toBeDisabled();
    finish();
    await waitFor(() => expect(screen.getByRole("button", { name: "Restart audio" })).toBeEnabled());
  });

  it("toggles auto-restart", () => {
    render(<SettingsView state={makeState()} />);
    fireEvent.click(screen.getByRole("switch", { name: "Restart audio when a device comes back" }));
    expect(api.send).toHaveBeenCalledWith(expect.objectContaining({ type: "setUiSettings", ui: expect.objectContaining({ autoRestartAudio: true }) }));
  });

  it("shows the app version", async () => {
    render(<SettingsView state={makeState()} />);
    expect(await screen.findByText(/Resonance 0\.2\.0/)).toBeInTheDocument();
  });
});
