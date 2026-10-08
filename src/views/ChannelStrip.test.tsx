import { fireEvent, render, screen, within } from "@testing-library/react";
import { ChannelStrip } from "./ChannelStrip";
import { makeApp } from "../test/fixtures";
import { useStore } from "../lib/store";
import * as api from "../lib/api";

vi.mock("../lib/api", () => ({
  send: vi.fn().mockResolvedValue(undefined),
  getAppIcon: vi.fn().mockResolvedValue(null),
}));

const spotify = makeApp({ key: "spotify.exe", name: "Spotify" });

describe("ChannelStrip call send", () => {
  beforeEach(() => {
    vi.mocked(api.send).mockClear();
    useStore.setState({ toasts: [] });
  });

  it("sends an app into calls at the default level", () => {
    render(<ChannelStrip app={spotify} call="ready" />);
    fireEvent.click(screen.getByRole("button", { name: "Send Spotify into calls" }));
    expect(api.send).toHaveBeenCalledWith({ type: "setAppSend", key: "spotify.exe", level: 0.7 });
  });

  it("shows the send level of a sent app and adjusts it", () => {
    render(<ChannelStrip app={{ ...spotify, send: 0.5 }} call="ready" />);
    const strip = screen.getByRole("region", { name: "Spotify" });
    expect(within(strip).getByRole("button", { name: "Send Spotify into calls" })).toHaveAttribute("aria-pressed", "true");
    fireEvent.keyDown(within(strip).getByRole("slider", { name: "Send" }), { key: "ArrowUp" });
    expect(api.send).toHaveBeenLastCalledWith({ type: "setAppSend", key: "spotify.exe", level: 0.55 });
    fireEvent.click(within(strip).getByRole("button", { name: "Send Spotify into calls" }));
    expect(api.send).toHaveBeenLastCalledWith({ type: "setAppSend", key: "spotify.exe", level: null });
  });

  it("is disabled with a hint while processing is off", () => {
    render(<ChannelStrip app={{ ...spotify, send: 0.5 }} call="off" />);
    const key = screen.getByRole("button", { name: "Send Spotify into calls" });
    expect(key).toBeDisabled();
    expect(key).toHaveAttribute("title", "Turn on Processing to send apps into calls");
    expect(screen.queryByRole("slider", { name: "Send" })).toBeNull();
  });

  it("has no call key when Windows can't capture apps, or for system sounds", () => {
    const { rerender } = render(<ChannelStrip app={spotify} call="unavailable" />);
    expect(screen.queryByRole("button", { name: /into calls/ })).toBeNull();
    rerender(<ChannelStrip app={makeApp({ key: "#system", name: "System sounds", isSystem: true })} call="ready" />);
    expect(screen.queryByRole("button", { name: /into calls/ })).toBeNull();
  });

  it("warns before sending a call app", () => {
    render(<ChannelStrip app={makeApp({ key: "discord.exe", name: "Discord" })} call="ready" />);
    fireEvent.click(screen.getByRole("button", { name: "Send Discord into calls" }));
    expect(useStore.getState().toasts.map((t) => t.message)).toContain(
      "Sending a call app into the call makes everyone hear themselves.",
    );
  });
});
