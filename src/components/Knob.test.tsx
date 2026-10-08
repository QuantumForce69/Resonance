import { fireEvent, render, screen } from "@testing-library/react";
import { Knob } from "./Knob";

const db = (v: number) => `${v} dB`;
const pct = (v: number) => `${Math.round(v * 100)}%`;

describe("Knob", () => {
  it("describes its value", () => {
    render(<Knob label="Threshold" value={-45} min={-80} max={-10} step={1} format={db} onChange={() => {}} />);
    const knob = screen.getByRole("slider", { name: "Threshold" });
    expect(knob).toHaveAttribute("aria-valuenow", "-45");
    expect(knob).toHaveAttribute("aria-valuetext", "-45 dB");
    expect(screen.getByText("-45 dB")).toBeInTheDocument();
  });

  it("steps with the keyboard and stays in range", () => {
    const onChange = vi.fn();
    render(<Knob label="Threshold" value={-11} min={-80} max={-10} step={1} format={db} onChange={onChange} />);
    const knob = screen.getByRole("slider", { name: "Threshold" });
    fireEvent.keyDown(knob, { key: "ArrowUp" });
    expect(onChange).toHaveBeenLastCalledWith(-10);
    fireEvent.keyDown(knob, { key: "PageUp" });
    expect(onChange).toHaveBeenLastCalledWith(-10);
    fireEvent.keyDown(knob, { key: "ArrowDown" });
    expect(onChange).toHaveBeenLastCalledWith(-12);
    fireEvent.keyDown(knob, { key: "Home" });
    expect(onChange).toHaveBeenLastCalledWith(-80);
  });

  it("drags upward to increase", () => {
    const onChange = vi.fn();
    render(<Knob label="Amount" value={0} min={0} max={1} step={0.05} format={pct} onChange={onChange} />);
    const knob = screen.getByRole("slider", { name: "Amount" });
    fireEvent.pointerDown(knob, { clientY: 300, pointerId: 1 });
    fireEvent.pointerMove(knob, { clientY: 200, pointerId: 1 });
    expect(knob).toHaveAttribute("aria-valuetext", "50%");
    fireEvent.pointerUp(knob, { clientY: 200, pointerId: 1 });
    expect(onChange).toHaveBeenLastCalledWith(0.5);
  });

  it("ignores input when disabled", () => {
    const onChange = vi.fn();
    render(<Knob label="Amount" value={0} min={0} max={1} step={0.05} format={pct} onChange={onChange} disabled />);
    const knob = screen.getByRole("slider", { name: "Amount" });
    fireEvent.keyDown(knob, { key: "ArrowUp" });
    fireEvent.pointerDown(knob, { clientY: 300, pointerId: 1 });
    fireEvent.pointerMove(knob, { clientY: 100, pointerId: 1 });
    fireEvent.pointerUp(knob, { clientY: 100, pointerId: 1 });
    expect(onChange).not.toHaveBeenCalled();
  });
});
