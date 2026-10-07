import { fireEvent, render, screen } from "@testing-library/react";
import { MixerView } from "./MixerView";
import { makeState } from "../test/fixtures";
import { useStore } from "../lib/store";
import * as api from "../lib/api";

vi.mock("../lib/api", () => ({
  send: vi.fn().mockResolvedValue(undefined),
  getAppIcon: vi.fn().mockResolvedValue(null),
}));

describe("MixerView", () => {
  beforeEach(() => vi.mocked(api.send).mockClear());

  it("renders one channel per app and a master section", () => {
    const s = makeState();
    useStore.setState({ state: s });
    render(<MixerView state={s} />);
    expect(screen.getByText("Master output")).toBeInTheDocument();
    expect(screen.getByRole("slider", { name: "Spotify volume" })).toBeInTheDocument();
    expect(screen.getByRole("slider", { name: "System sounds volume" })).toBeInTheDocument();
    expect(screen.getByRole("slider", { name: "Master volume" })).toBeInTheDocument();
  });

  it("offers output routing only for non-system apps", () => {
    const s = makeState();
    useStore.setState({ state: s });
    render(<MixerView state={s} />);
    expect(screen.getByRole("combobox", { name: "Output device for Spotify" })).toBeInTheDocument();
    expect(screen.queryByRole("combobox", { name: "Output device for System sounds" })).toBeNull();
    expect(screen.getByText("Follows default")).toBeInTheDocument();
  });

  it("explains when routing is unavailable", () => {
    const s = makeState({ routingAvailable: false });
    useStore.setState({ state: s });
    render(<MixerView state={s} />);
    expect(screen.getByText(/isn't available on this Windows build/i)).toBeInTheDocument();
  });

  it("master section mutes the default device and switches it", () => {
    const s = makeState();
    useStore.setState({ state: s });
    render(<MixerView state={s} />);
    fireEvent.click(screen.getByRole("button", { name: "Mute Headphones" }));
    expect(api.send).toHaveBeenCalledWith({ type: "setDeviceMute", id: "out1", muted: true });
    fireEvent.change(screen.getByRole("combobox", { name: "Master output device" }), { target: { value: "out2" } });
    expect(api.send).toHaveBeenLastCalledWith({ type: "setDefaultDevice", id: "out2", role: "console" });
  });

  it("shows levels in dB", () => {
    const s = makeState();
    s.apps[0] = { ...s.apps[0], volume: 0.62 };
    useStore.setState({ state: s });
    render(<MixerView state={s} />);
    expect(screen.getAllByText("−8.3 dB").length).toBeGreaterThan(0);
  });
});
