import { fireEvent, render, screen } from "@testing-library/react";
import { DeviceSelect } from "./DeviceSelect";
import { makeApp, makeState } from "../test/fixtures";
import { useStore } from "../lib/store";
import * as api from "../lib/api";

vi.mock("../lib/api", () => ({ send: vi.fn().mockResolvedValue(undefined) }));

describe("DeviceSelect", () => {
  beforeEach(() => vi.mocked(api.send).mockClear());

  it("renders nothing when routing is unavailable", () => {
    useStore.setState({ state: makeState({ routingAvailable: false }) });
    const { container } = render(<DeviceSelect app={makeApp({ key: "a.exe" })} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("lists the default plus active outputs, excluding unplugged ones", () => {
    useStore.setState({ state: makeState() });
    render(<DeviceSelect app={makeApp({ key: "a.exe", name: "A" })} />);
    const options = screen.getAllByRole("option").map((o) => o.textContent);
    expect(options).toEqual(["Default (Headphones)", "Headphones", "Speakers"]);
  });

  it("routes to a device and back to default", () => {
    useStore.setState({ state: makeState() });
    const { rerender } = render(<DeviceSelect app={makeApp({ key: "a.exe", name: "A" })} />);
    fireEvent.change(screen.getByRole("combobox", { name: "Output device for A" }), { target: { value: "out2" } });
    expect(api.send).toHaveBeenCalledWith({ type: "setAppOutput", key: "a.exe", deviceId: "out2" });
    rerender(<DeviceSelect app={makeApp({ key: "a.exe", name: "A", output: "out2" })} />);
    fireEvent.change(screen.getByRole("combobox", { name: "Output device for A" }), { target: { value: "" } });
    expect(api.send).toHaveBeenLastCalledWith({ type: "setAppOutput", key: "a.exe", deviceId: null });
  });

  it("keeps an unavailable routed device visible", () => {
    useStore.setState({ state: makeState() });
    render(<DeviceSelect app={makeApp({ key: "a.exe", name: "A", output: "gone" })} />);
    expect(screen.getByRole("option", { name: "Old DAC (unavailable)" })).toBeInTheDocument();
  });
});
