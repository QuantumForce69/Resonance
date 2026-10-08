import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { SettingsView } from "./SettingsView";
import { makeState } from "../test/fixtures";
import * as api from "../lib/api";

vi.mock("../lib/api", () => ({
  send: vi.fn().mockResolvedValue(undefined),
  getAutostart: vi.fn().mockResolvedValue(false),
  setAutostart: vi.fn().mockResolvedValue(undefined),
}));

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
});
