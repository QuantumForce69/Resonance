import { fireEvent, render, screen, within } from "@testing-library/react";
import { ProfilesView } from "./ProfilesView";
import { makeHeadset, makeProfile, makeState } from "../test/fixtures";
import { useStore } from "../lib/store";
import * as api from "../lib/api";

vi.mock("../lib/api", () => ({
  send: vi.fn().mockResolvedValue(undefined),
  saveProfile: vi.fn().mockResolvedValue(undefined),
  updateProfile: vi.fn().mockResolvedValue(undefined),
  applyProfile: vi.fn().mockResolvedValue(undefined),
}));

const withProfiles = () =>
  makeState({
    profiles: [makeProfile({ id: "p1", name: "Gaming" }), makeProfile({ id: "p2", name: "Work", parts: { apps: null, processing: null, headphones: null, defaultOutput: "out1" } })],
    activeProfile: "p1",
  });

describe("ProfilesView", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useStore.setState({ headset: makeHeadset() });
  });

  it("explains profiles when there are none", () => {
    render(<ProfilesView state={makeState()} />);
    expect(screen.getByText(/no profiles yet/i)).toBeInTheDocument();
  });

  it("saves the current setup with the chosen parts", () => {
    render(<ProfilesView state={makeState()} />);
    const save = screen.getByRole("button", { name: "Save profile" });
    expect(save).toBeDisabled();
    fireEvent.change(screen.getByRole("textbox", { name: "Profile name" }), { target: { value: "Gaming" } });
    fireEvent.click(screen.getByRole("checkbox", { name: "Default output" }));
    fireEvent.click(save);
    expect(api.saveProfile).toHaveBeenCalledWith("Gaming", { apps: true, processing: true, headphones: false, defaultOutput: true });
  });

  it("only offers headphones while they're connected", () => {
    useStore.setState({ headset: makeHeadset({ connection: { state: "notFound" } }) });
    render(<ProfilesView state={makeState()} />);
    expect(screen.getByRole("checkbox", { name: "Headphones" })).toBeDisabled();
  });

  it("shows each profile's parts and marks the active one", () => {
    render(<ProfilesView state={withProfiles()} />);
    const gaming = screen.getByRole("region", { name: "Gaming" });
    expect(within(gaming).getByText("Active")).toBeInTheDocument();
    expect(within(gaming).getByText("Apps")).toBeInTheDocument();
    const work = screen.getByRole("region", { name: "Work" });
    expect(within(work).getByText("Default output")).toBeInTheDocument();
    expect(within(work).queryByText("Active")).toBeNull();
  });

  it("applies and updates a profile", () => {
    render(<ProfilesView state={withProfiles()} />);
    const work = screen.getByRole("region", { name: "Work" });
    fireEvent.click(within(work).getByRole("button", { name: "Apply" }));
    expect(api.applyProfile).toHaveBeenCalledWith("p2");
    fireEvent.click(within(work).getByRole("button", { name: "Update" }));
    expect(api.updateProfile).toHaveBeenCalledWith("p2");
  });

  it("renames a profile", () => {
    render(<ProfilesView state={withProfiles()} />);
    const gaming = screen.getByRole("region", { name: "Gaming" });
    fireEvent.click(within(gaming).getByRole("button", { name: "Rename" }));
    fireEvent.change(within(gaming).getByRole("textbox", { name: "New name" }), { target: { value: "Games" } });
    fireEvent.click(within(gaming).getByRole("button", { name: "Save" }));
    expect(api.send).toHaveBeenCalledWith({ type: "renameProfile", id: "p1", name: "Games" });
  });

  it("asks before deleting", () => {
    render(<ProfilesView state={withProfiles()} />);
    const work = screen.getByRole("region", { name: "Work" });
    fireEvent.click(within(work).getByRole("button", { name: "Delete" }));
    expect(api.send).not.toHaveBeenCalled();
    fireEvent.click(within(work).getByRole("button", { name: "Delete Work" }));
    expect(api.send).toHaveBeenCalledWith({ type: "deleteProfile", id: "p2" });
  });
});
