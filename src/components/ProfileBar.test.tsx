import { fireEvent, render, screen } from "@testing-library/react";
import { ProfileBar } from "./ProfileBar";
import { makeProfile, makeState } from "../test/fixtures";
import * as api from "../lib/api";

vi.mock("../lib/api", () => ({ applyProfile: vi.fn().mockResolvedValue(undefined) }));

describe("ProfileBar", () => {
  it("is hidden without profiles", () => {
    render(<ProfileBar state={makeState()} />);
    expect(screen.queryByRole("navigation", { name: "Profiles" })).toBeNull();
  });

  it("lights the active profile and switches on click", () => {
    const state = makeState({ profiles: [makeProfile({ id: "p1", name: "Gaming" }), makeProfile({ id: "p2", name: "Work" })], activeProfile: "p1" });
    render(<ProfileBar state={state} />);
    expect(screen.getByRole("button", { name: "Gaming" })).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(screen.getByRole("button", { name: "Work" }));
    expect(api.applyProfile).toHaveBeenCalledWith("p2");
  });
});
