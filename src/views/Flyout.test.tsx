import { fireEvent, render, screen } from "@testing-library/react";
import { Flyout } from "./Flyout";
import { makeProcessing, makeState } from "../test/fixtures";
import { useStore } from "../lib/store";
import * as api from "../lib/api";

vi.mock("../lib/api", () => ({
  send: vi.fn().mockResolvedValue(undefined),
  getAppIcon: vi.fn().mockResolvedValue(null),
  showMain: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ hide: vi.fn(), close: vi.fn().mockResolvedValue(undefined) }) }));

describe("Flyout", () => {
  it("tags the mic key while processing is running", () => {
    useStore.setState({ state: makeState() });
    const { unmount } = render(<Flyout />);
    expect(screen.queryByText("Processed")).toBeNull();
    unmount();
    useStore.setState({ state: makeState({ processing: makeProcessing({ status: { state: "running" } }) }) });
    render(<Flyout />);
    expect(screen.getByText("Processed")).toBeInTheDocument();
  });

  it("shows a loading state before the first state arrives", () => {
    useStore.setState({ state: null });
    render(<Flyout />);
    expect(screen.getByText(/connecting/i)).toBeInTheDocument();
  });

  it("shows the master output, the mic key and every app", () => {
    useStore.setState({ state: makeState() });
    render(<Flyout />);
    expect(screen.getByText("Headphones")).toBeInTheDocument();
    expect(screen.getByText("Mic live")).toBeInTheDocument();
    expect(screen.getByText(/Desk Mic/)).toBeInTheDocument();
    expect(screen.getByText("Spotify")).toBeInTheDocument();
    expect(screen.getByText("System sounds")).toBeInTheDocument();
  });

  it("mutes the master output and opens the main window", () => {
    useStore.setState({ state: makeState() });
    render(<Flyout />);
    fireEvent.click(screen.getByRole("button", { name: "Mute Headphones" }));
    expect(api.send).toHaveBeenCalledWith({ type: "setDeviceMute", id: "out1", muted: true });
    fireEvent.click(screen.getByRole("button", { name: "Open Resonance" }));
    expect(api.showMain).toHaveBeenCalled();
  });

  it("explains when nothing is playing", () => {
    useStore.setState({ state: makeState({ apps: [] }) });
    render(<Flyout />);
    expect(screen.getByText(/no apps are playing audio/i)).toBeInTheDocument();
  });
});
