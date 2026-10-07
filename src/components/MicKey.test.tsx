import { fireEvent, render, screen } from "@testing-library/react";
import { MicKey } from "./MicKey";
import { makeDevice } from "../test/fixtures";
import * as api from "../lib/api";

vi.mock("../lib/api", () => ({ send: vi.fn().mockResolvedValue(undefined) }));

describe("MicKey", () => {
  const mic = makeDevice({ id: "mic1", name: "Desk Mic", flow: "capture", defaultConsole: true });

  it("shows live state and mutes on press", () => {
    render(<MicKey mic={mic} hotkey="Ctrl+Shift+M" />);
    expect(screen.getByText("Mic live")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Microphone is live. Mute microphone" }));
    expect(api.send).toHaveBeenCalledWith({ type: "setMicMute", muted: true });
  });

  it("shows muted state and unmutes on press", () => {
    render(<MicKey mic={{ ...mic, muted: true }} hotkey={null} />);
    expect(screen.getByText("Mic muted")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Microphone is muted. Unmute microphone" }));
    expect(api.send).toHaveBeenLastCalledWith({ type: "setMicMute", muted: false });
  });

  it("says so when there is no microphone", () => {
    render(<MicKey mic={undefined} hotkey={null} />);
    expect(screen.getByText("No microphone")).toBeInTheDocument();
  });

  it("the wide variant names the mic and the shortcut", () => {
    render(<MicKey mic={mic} hotkey="Ctrl+Shift+M" size="wide" />);
    expect(screen.getByText("Desk Mic, press Ctrl Shift M to mute")).toBeInTheDocument();
  });
});
