import { useState } from "react";
import { useStore } from "../lib/store";
import { Toasts } from "../components/Toasts";
import { DevicesView } from "./DevicesView";
import { HeadphonesView } from "./HeadphonesView";
import { MicView } from "./MicView";
import { MixerView } from "./MixerView";
import { Onboarding } from "./Onboarding";
import { ProfilesView } from "./ProfilesView";
import { SettingsView } from "./SettingsView";
import { Titlebar, type View } from "./Titlebar";

export function MainWindow() {
  const [view, setView] = useState<View>("mixer");
  const state = useStore((s) => s.state);

  return (
    <div className="flex h-full flex-col">
      <Titlebar view={view} onSelect={setView} />
      <main className="min-h-0 min-w-0 flex-1 overflow-auto p-7">
        {!state ? (
          <div className="grid h-full place-items-center text-[13px] text-dim">Connecting to audio…</div>
        ) : view === "mixer" ? (
          <MixerView state={state} />
        ) : view === "devices" ? (
          <DevicesView state={state} />
        ) : view === "mic" ? (
          <MicView state={state} />
        ) : view === "headphones" ? (
          <HeadphonesView />
        ) : view === "profiles" ? (
          <ProfilesView state={state} />
        ) : (
          <SettingsView state={state} />
        )}
      </main>
      {state && !state.ui.onboarded && <Onboarding state={state} />}
      <Toasts />
    </div>
  );
}
