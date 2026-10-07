import { fireEvent, render, screen } from "@testing-library/react";
import { HeadsetStrip } from "./HeadsetStrip";
import { makeHeadset } from "../test/fixtures";
import * as api from "../lib/api";

vi.mock("../lib/api", () => ({ sendHeadset: vi.fn().mockResolvedValue(undefined) }));

describe("HeadsetStrip", () => {
  beforeEach(() => vi.mocked(api.sendHeadset).mockClear());

  it("shows battery and switches the ambient mode", () => {
    render(<HeadsetStrip headset={makeHeadset()} />);
    expect(screen.getByRole("img", { name: "Battery 4 of 5" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("radio", { name: "Normal" }));
    expect(api.sendHeadset).toHaveBeenCalledWith({ type: "setAmbientMode", mode: "normal" });
  });

  it("explains when the headphones aren't connected", () => {
    render(<HeadsetStrip headset={makeHeadset({ connection: { state: "disconnected", retryInSecs: 5 } })} />);
    expect(screen.getByText("Not connected, retrying in 5 s")).toBeInTheDocument();
    expect(screen.queryByRole("radio")).toBeNull();
  });
});
