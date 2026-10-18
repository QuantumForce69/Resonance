import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { restartAudio } from "../lib/api";
import { run } from "../lib/store";
import type { Theme, UiState } from "../lib/types";
import { StartWithWindows } from "../components/StartWithWindows";
import { Field, Panel, Segmented, Toggle, ViewHeader } from "../components/ui";

const THEMES: { value: Theme; label: string }[] = [
  { value: "system", label: "System" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];

export function SettingsView({ state }: { state: UiState }) {
  const ui = state.ui;
  const [restarting, setRestarting] = useState(false);
  const [version, setVersion] = useState("");
  useEffect(() => {
    getVersion().then(setVersion, () => {});
  }, []);
  const restart = async () => {
    setRestarting(true);
    try {
      await restartAudio();
    } catch {
      // The failure arrives as a notice from the backend.
    } finally {
      setRestarting(false);
    }
  };
  return (
    <div className="flex max-w-3xl flex-col gap-5">
      <ViewHeader title="Settings" />
      <Panel className="px-6 py-1.5">
        <Field label="Theme">
          <Segmented label="Theme" value={ui.theme} options={THEMES} onChange={(theme) => run({ type: "setUiSettings", ui: { ...ui, theme } })} />
        </Field>
        <Field label="Start with Windows" hint="Resonance starts in the tray when you sign in">
          <StartWithWindows />
        </Field>
        <Field label="Start in the tray" hint="Only show the tray icon when Resonance launches">
          <Toggle label="Start in the tray" checked={ui.startMinimized} onChange={(startMinimized) => run({ type: "setUiSettings", ui: { ...ui, startMinimized } })} />
        </Field>
        <Field label="Restart audio" hint="Reconnects every device, app and your headphones. Use it if sound stops or a device is missing. Shortcut: Ctrl+R.">
          <button type="button" onClick={restart} disabled={restarting} className="key">
            {restarting ? "Restarting…" : "Restart audio"}
          </button>
        </Field>
        <Field label="Restart audio when a device comes back" hint="If a device Resonance is using disconnects, restart audio once it's plugged back in">
          <Toggle label="Restart audio when a device comes back" checked={ui.autoRestartAudio} onChange={(autoRestartAudio) => run({ type: "setUiSettings", ui: { ...ui, autoRestartAudio } })} />
        </Field>
        <Field label="Remembered app volumes" hint="Forget saved levels for apps that aren't running. Running apps are remembered again.">
          <button type="button" onClick={() => run({ type: "resetAppMemory" })} className="key">
            Forget saved volumes
          </button>
        </Field>
      </Panel>
      <p className="m-0 text-[12px] text-dim">{version ? `Resonance ${version}. ` : ""}Settings and logs are kept in %APPDATA%\Resonance.</p>
    </div>
  );
}
