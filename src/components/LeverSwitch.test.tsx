import { fireEvent, render, screen } from "@testing-library/react";
import { LeverSwitch } from "./LeverSwitch";

const OPTIONS = [
  { value: "a", label: "Alpha" },
  { value: "b", label: "Bravo" },
  { value: "c", label: "Charlie" },
];

function setup(onChange = vi.fn(), disabled = false) {
  render(<LeverSwitch label="Pick" value="b" options={OPTIONS} onChange={onChange} disabled={disabled} />);
  const track = screen.getByTestId("lever-track");
  track.getBoundingClientRect = () => ({ left: 0, width: 300, top: 0, height: 30, right: 300, bottom: 30, x: 0, y: 0, toJSON: () => ({}) });
  return { track, onChange };
}

describe("LeverSwitch", () => {
  it("selects a position by clicking its label", () => {
    const { onChange } = setup();
    fireEvent.click(screen.getByRole("radio", { name: "Alpha" }));
    expect(onChange).toHaveBeenCalledWith("a");
  });

  it("selects the position under a click on the track", () => {
    const { track, onChange } = setup();
    fireEvent.pointerDown(track, { clientX: 20, pointerId: 1 });
    fireEvent.pointerUp(track, { clientX: 20, pointerId: 1 });
    expect(onChange).toHaveBeenCalledWith("a");
  });

  it("follows a drag and commits where it is released", () => {
    const { track, onChange } = setup();
    fireEvent.pointerDown(track, { clientX: 150, pointerId: 1 });
    fireEvent.pointerMove(track, { clientX: 280, pointerId: 1 });
    expect(screen.getByRole("radio", { name: "Charlie" })).toHaveAttribute("data-hover", "true");
    fireEvent.pointerUp(track, { clientX: 280, pointerId: 1 });
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange).toHaveBeenCalledWith("c");
  });

  it("does nothing when released on the current position or when disabled", () => {
    const { track, onChange } = setup();
    fireEvent.pointerDown(track, { clientX: 150, pointerId: 1 });
    fireEvent.pointerUp(track, { clientX: 150, pointerId: 1 });
    expect(onChange).not.toHaveBeenCalled();
  });
});

describe("LeverSwitch disabled", () => {
  it("ignores the track", () => {
    const { track, onChange } = setup(vi.fn(), true);
    fireEvent.pointerDown(track, { clientX: 20, pointerId: 1 });
    fireEvent.pointerUp(track, { clientX: 20, pointerId: 1 });
    expect(onChange).not.toHaveBeenCalled();
  });
});
