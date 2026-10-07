import { fireEvent, render, screen } from "@testing-library/react";
import { EqFaders, formatEqValue, formatHz } from "./EqFaders";
import { makeHeadset } from "../test/fixtures";

describe("EqFaders", () => {
  it("formats values and frequencies", () => {
    expect(formatEqValue(25, 1)).toBe("+2.5");
    expect(formatEqValue(-120, 1)).toBe("−12.0");
    expect(formatEqValue(0, 1)).toBe("0.0");
    expect(formatHz(800)).toBe("800");
    expect(formatHz(12800)).toBe("12.8k");
  });

  it("renders one fader per band and reports the changed curve", () => {
    const onChange = vi.fn();
    const eq = makeHeadset().eq!;
    render(<EqFaders eq={eq} onChange={onChange} />);
    expect(screen.getAllByRole("slider")).toHaveLength(8);
    fireEvent.change(screen.getByRole("slider", { name: "800 Hz" }), { target: { value: "50" } });
    expect(onChange).toHaveBeenCalledWith([0, 0, 0, 7, 0, 0, 0, 0]);
  });
});
