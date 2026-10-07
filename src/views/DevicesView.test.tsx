import { fireEvent, render, screen } from "@testing-library/react";
import { DevicesView } from "./DevicesView";
import { makeDevice, makeState } from "../test/fixtures";
import * as api from "../lib/api";

vi.mock("../lib/api", () => ({ send: vi.fn().mockResolvedValue(undefined) }));

describe("DevicesView", () => {
  beforeEach(() => vi.mocked(api.send).mockClear());

  it("groups outputs and inputs and labels defaults and unplugged devices", () => {
    render(<DevicesView state={makeState()} />);
    expect(screen.getByRole("heading", { name: "Outputs" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Inputs" })).toBeInTheDocument();
    expect(screen.getAllByText("Default").length).toBeGreaterThan(0);
    expect(screen.getByText("Unplugged")).toBeInTheDocument();
  });

  it("sets default and calls devices", () => {
    render(<DevicesView state={makeState()} />);
    fireEvent.click(screen.getByRole("button", { name: "Set Speakers as default" }));
    expect(api.send).toHaveBeenCalledWith({ type: "setDefaultDevice", id: "out2", role: "console" });
    fireEvent.click(screen.getByRole("button", { name: "Use Speakers for calls" }));
    expect(api.send).toHaveBeenCalledWith({ type: "setDefaultDevice", id: "out2", role: "communications" });
  });

  it("renames a device with Enter", () => {
    render(<DevicesView state={makeState()} />);
    fireEvent.click(screen.getByRole("button", { name: "Rename Speakers" }));
    const input = screen.getByRole("textbox", { name: "Device name" });
    fireEvent.change(input, { target: { value: "Desk speakers" } });
    fireEvent.submit(input.closest("form")!);
    expect(api.send).toHaveBeenCalledWith({ type: "setDeviceAlias", id: "out2", alias: "Desk speakers" });
  });

  it("collapses hidden devices behind a toggle", () => {
    const s = makeState();
    s.devices.push(makeDevice({ id: "h", name: "Virtual Cable", hidden: true }));
    render(<DevicesView state={s} />);
    expect(screen.queryByText("Virtual Cable")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: /show 1 hidden/i }));
    expect(screen.getByText("Virtual Cable")).toBeInTheDocument();
  });
});
