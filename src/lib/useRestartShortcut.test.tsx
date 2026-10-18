import { render, fireEvent } from "@testing-library/react";
import { useRestartShortcut } from "./useRestartShortcut";
import * as api from "./api";

vi.mock("./api", () => ({ restartAudio: vi.fn(() => Promise.resolve()) }));

function Host() {
  useRestartShortcut();
  return <input aria-label="field" />;
}

it("Ctrl+R restarts audio", () => {
  render(<Host />);
  fireEvent.keyDown(window, { key: "r", ctrlKey: true });
  expect(api.restartAudio).toHaveBeenCalledTimes(1);
});

it("ignores key repeat", () => {
  vi.mocked(api.restartAudio).mockClear();
  render(<Host />);
  fireEvent.keyDown(window, { key: "r", ctrlKey: true, repeat: true });
  expect(api.restartAudio).not.toHaveBeenCalled();
});

it("is ignored while typing in a field", () => {
  vi.mocked(api.restartAudio).mockClear();
  const { getByLabelText } = render(<Host />);
  fireEvent.keyDown(getByLabelText("field"), { key: "r", ctrlKey: true });
  expect(api.restartAudio).not.toHaveBeenCalled();
});
