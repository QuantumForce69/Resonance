import { switchProfile } from "../lib/store";
import type { UiState } from "../lib/types";

/** The flyout's profile buttons; the last applied profile is lit. */
export function ProfileBar({ state }: { state: UiState }) {
  if (state.profiles.length === 0) return null;
  return (
    <nav aria-label="Profiles" className="flex gap-1.5 overflow-x-auto border-b border-groove px-3.5 py-2.5">
      {state.profiles.map((p) => {
        const active = p.id === state.activeProfile;
        return (
          <button
            key={p.id}
            type="button"
            aria-pressed={active}
            onClick={() => void switchProfile(p.id)}
            className={`key h-7 shrink-0 px-3 text-[12px] ${active ? "mic-live text-lilac" : ""}`}
          >
            {p.name}
          </button>
        );
      })}
    </nav>
  );
}
