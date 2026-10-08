import { fireEvent, render, screen } from "@testing-library/react";
import { CallMix } from "./CallMix";
import { makeProcessing, makeState } from "../test/fixtures";
import * as api from "../lib/api";

vi.mock("../lib/api", () => ({
  send: vi.fn().mockResolvedValue(undefined),
  getAppIcon: vi.fn().mockResolvedValue(null),
}));

function lastCall() {
  const cmd = vi.mocked(api.send).mock.calls.at(-1)![0];
  if (cmd.type !== "setProcessing") throw new Error(`unexpected ${cmd.type}`);
  return cmd.processing.call;
}

describe("CallMix", () => {
  beforeEach(() => vi.mocked(api.send).mockClear());

  it("sets the level of all sent apps", () => {
    render(<CallMix state={makeState()} />);
    fireEvent.change(screen.getByRole("slider", { name: "Apps in call" }), { target: { value: "50" } });
    expect(lastCall().master).toBe(0.5);
  });

  it("turns ducking off and sets how far apps duck", () => {
    render(<CallMix state={makeState()} />);
    fireEvent.click(screen.getByRole("switch", { name: "Duck apps while I talk" }));
    expect(lastCall().duck).toBe(false);
    fireEvent.keyDown(screen.getByRole("slider", { name: "Duck by" }), { key: "ArrowDown" });
    expect(lastCall().duckDb).toBe(-13);
  });

  it("lists sent apps and stops sending one", () => {
    const s = makeState();
    s.apps[0] = { ...s.apps[0], send: 0.5 };
    render(<CallMix state={s} />);
    fireEvent.click(screen.getByRole("button", { name: "Stop sending Spotify" }));
    expect(api.send).toHaveBeenCalledWith({ type: "setAppSend", key: "spotify.exe", level: null });
  });

  it("says when nothing is sent, and when Windows can't do it", () => {
    const { rerender } = render(<CallMix state={makeState()} />);
    expect(screen.getByText(/no apps sent yet/i)).toBeInTheDocument();
    rerender(<CallMix state={makeState({ processing: makeProcessing({ callAvailable: false }) })} />);
    expect(screen.getByText(/needs windows 10 version 2004 or newer/i)).toBeInTheDocument();
    expect(screen.queryByRole("slider")).toBeNull();
  });
});
