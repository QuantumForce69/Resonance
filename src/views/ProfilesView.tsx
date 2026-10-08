import { useState } from "react";
import { PARTS, partLabels } from "../lib/profiles";
import { deleteProfile, recaptureProfile, renameProfile, saveProfileAs, switchProfile, useStore } from "../lib/store";
import type { PartChoice, Profile, UiState } from "../lib/types";
import { Badge, Banner, Panel, ViewHeader } from "../components/ui";

const PRIMARY = "key mic-live text-lilac h-8 px-3.5 text-[13px]";
const SECONDARY = "key h-8 px-3 text-[13px]";

function SaveForm() {
  const headsetOn = useStore((s) => s.headset?.connection.state === "connected");
  const [name, setName] = useState("");
  const [parts, setParts] = useState<PartChoice>({ apps: true, processing: true, headphones: false, defaultOutput: false });
  const chosen = { ...parts, headphones: parts.headphones && headsetOn };
  const canSave = name.trim() !== "" && Object.values(chosen).some(Boolean);

  const save = async () => {
    if (await saveProfileAs(name.trim(), chosen)) setName("");
  };

  return (
    <Panel className="flex flex-col gap-4 px-6 py-5">
      <div>
        <h2 className="m-0 text-[16px] font-medium">Save current setup</h2>
        <p className="m-0 mt-0.5 text-[12px] text-fog">Choose what the profile controls. Anything you leave out stays as it is when you switch.</p>
      </div>
      <div className="flex flex-wrap items-end gap-4">
        <label className="flex min-w-[220px] flex-1 flex-col gap-1.5 text-[13px] text-fog">
          Profile name
          <input
            type="text"
            aria-label="Profile name"
            value={name}
            placeholder="e.g. Gaming"
            onChange={(e) => setName(e.currentTarget.value)}
            className="field-select"
          />
        </label>
        <button type="button" className={PRIMARY} disabled={!canSave} onClick={() => void save()}>
          Save profile
        </button>
      </div>
      <div className="flex flex-wrap gap-x-6 gap-y-2">
        {PARTS.map(({ key, label }) => {
          const disabled = key === "headphones" && !headsetOn;
          return (
            <label key={key} className={`flex items-center gap-2 text-[13px] ${disabled ? "text-dim" : "text-paper"}`} title={disabled ? "Connect the Space One to include it" : undefined}>
              <input
                type="checkbox"
                aria-label={label}
                checked={chosen[key]}
                disabled={disabled}
                onChange={(e) => setParts({ ...parts, [key]: e.currentTarget.checked })}
                className="h-4 w-4 accent-[var(--accent)]"
              />
              {label}
            </label>
          );
        })}
      </div>
    </Panel>
  );
}

function ProfileCard({ profile, active }: { profile: Profile; active: boolean }) {
  const [renaming, setRenaming] = useState<string | null>(null);
  const [confirming, setConfirming] = useState(false);

  return (
    <section aria-label={profile.name} className={`flex flex-col gap-4 rounded-[14px] border bg-console px-5 py-4 ${active ? "border-accent/60" : "border-groove"}`}>
      <div className="flex items-start justify-between gap-3">
        {renaming === null ? (
          <h3 className="m-0 truncate text-[16px] font-medium">{profile.name}</h3>
        ) : (
          <div className="flex flex-1 gap-2">
            <input
              type="text"
              aria-label="New name"
              value={renaming}
              onChange={(e) => setRenaming(e.currentTarget.value)}
              className="field-select min-w-0 flex-1"
            />
            <button
              type="button"
              className={PRIMARY}
              disabled={renaming.trim() === ""}
              onClick={() => {
                void renameProfile(profile.id, renaming.trim());
                setRenaming(null);
              }}
            >
              Save
            </button>
            <button type="button" className={SECONDARY} onClick={() => setRenaming(null)}>
              Cancel
            </button>
          </div>
        )}
        {active && renaming === null && <Badge tone="accent">Active</Badge>}
      </div>
      <div className="flex flex-wrap gap-1.5">
        {partLabels(profile).map((label) => (
          <Badge key={label}>{label}</Badge>
        ))}
      </div>
      {confirming ? (
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-[13px] text-fog">Delete this profile?</span>
          <button type="button" className="key h-8 border-coral/60 px-3 text-[13px] text-coral" aria-label={`Delete ${profile.name}`} onClick={() => void deleteProfile(profile.id)}>
            Delete
          </button>
          <button type="button" className={SECONDARY} onClick={() => setConfirming(false)}>
            Cancel
          </button>
        </div>
      ) : (
        <div className="flex flex-wrap gap-2">
          <button type="button" className={PRIMARY} onClick={() => void switchProfile(profile.id)}>
            Apply
          </button>
          <button type="button" className={SECONDARY} title="Save the current setup into this profile" onClick={() => void recaptureProfile(profile.id)}>
            Update
          </button>
          <button type="button" className={SECONDARY} onClick={() => setRenaming(profile.name)}>
            Rename
          </button>
          <button type="button" className={SECONDARY} onClick={() => setConfirming(true)}>
            Delete
          </button>
        </div>
      )}
    </section>
  );
}

export function ProfilesView({ state }: { state: UiState }) {
  return (
    <div className="flex max-w-4xl flex-col gap-5">
      <ViewHeader title="Profiles" subtitle="Switch your whole setup in one click, here or from the tray" />
      <SaveForm />
      {state.profiles.length === 0 ? (
        <Banner>No profiles yet. Set Resonance up the way you like, then save it as a profile.</Banner>
      ) : (
        <div className="grid gap-4 md:grid-cols-2">
          {state.profiles.map((p) => (
            <ProfileCard key={p.id} profile={p} active={p.id === state.activeProfile} />
          ))}
        </div>
      )}
    </div>
  );
}
