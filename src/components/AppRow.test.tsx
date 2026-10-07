import { fireEvent, render, screen } from "@testing-library/react";
import { AppRow } from "./AppRow";
import { makeApp, makeState } from "../test/fixtures";
import { useStore } from "../lib/store";
import * as api from "../lib/api";

vi.mock("../lib/api", () => ({
  send: vi.fn().mockResolvedValue(undefined),
  getAppIcon: vi.fn().mockResolvedValue(null),
}));

describe("AppRow", () => {
  beforeEach(() => {
    vi.mocked(api.send).mockClear();
    useStore.setState({ state: makeState() });
  });

  it("shows the app name and its level in dB", () => {
    render(<AppRow app={makeApp({ key: "spotify.exe", name: "Spotify", volume: 0.62 })} />);
    expect(screen.getByText("Spotify")).toBeInTheDocument();
    expect(screen.getByText("−8.3 dB")).toBeInTheDocument();
  });

  it("shows Muted instead of a level when muted", () => {
    render(<AppRow app={makeApp({ key: "spotify.exe", name: "Spotify", muted: true })} />);
    expect(screen.getByText("Muted")).toBeInTheDocument();
  });

  it("toggles mute", () => {
    render(<AppRow app={makeApp({ key: "spotify.exe", name: "Spotify" })} />);
    fireEvent.click(screen.getByRole("button", { name: "Mute Spotify" }));
    expect(api.send).toHaveBeenCalledWith({ type: "setAppMute", key: "spotify.exe", muted: true });
  });

  it("sends volume changes from the fader", () => {
    render(<AppRow app={makeApp({ key: "spotify.exe", name: "Spotify" })} />);
    fireEvent.change(screen.getByRole("slider", { name: "Spotify volume" }), { target: { value: "40" } });
    expect(api.send).toHaveBeenCalledWith({ type: "setAppVolume", key: "spotify.exe", volume: 0.4 });
  });

  it("shows a letter tile when the program has no icon", () => {
    render(<AppRow app={makeApp({ key: "zoom.exe", name: "Zoom" })} />);
    expect(screen.getByText("Z")).toBeInTheDocument();
  });
});
