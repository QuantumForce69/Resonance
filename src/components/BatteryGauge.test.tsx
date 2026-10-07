import { render, screen } from "@testing-library/react";
import { BatteryGauge } from "./BatteryGauge";

describe("BatteryGauge", () => {
  it("names the level for screen readers", () => {
    render(<BatteryGauge battery={{ level: 4, max: 5, charging: null }} />);
    expect(screen.getByRole("img", { name: "Battery 4 of 5" })).toBeInTheDocument();
  });

  it("mentions charging only when the device reports it", () => {
    render(<BatteryGauge battery={{ level: 1, max: 5, charging: true }} />);
    expect(screen.getByRole("img", { name: "Battery 1 of 5, charging" })).toBeInTheDocument();
  });
});
