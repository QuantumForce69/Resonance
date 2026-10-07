import { run } from "../lib/store";
import type { Theme, UiState } from "../lib/types";
import { Field, Panel, Segmented, Toggle, ViewHeader } from "../components/ui";

const THEMES: { value: Theme; label: string }[] = [
  { value: "system", label: "System" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];

export function SettingsView({ state }: { state: UiState }) {
  const ui = state.ui;
  return (
    <div className="flex max-w-3xl flex-col gap-5">
      <ViewHeader title="Settings" />
      <Panel className="px-6 py-1.5">
        <Field label="Theme">
          <Segmented label="Theme" value={ui.theme} options={THEMES} onChange={(theme) => run({ type: "setUiSettings", ui: { ...ui, theme } })} />
        </Field>
        <Field label="Start in the tray" hint="Only show the tray icon when Resonance launches">
          <Toggle label="Start in the tray" checked={ui.startMinimized} onChange={(startMinimized) => run({ type: "setUiSettings", ui: { ...ui, startMinimized } })} />
        </Field>
        <Field label="Remembered app volumes" hint="Forget saved levels for apps that aren't running. Running apps are remembered again.">
          <button type="button" onClick={() => run({ type: "resetAppMemory" })} className="key">
            Forget saved volumes
          </button>
        </Field>
      </Panel>
      <p className="m-0 text-[12px] text-dim">Resonance 0.1.0. Settings and logs are kept in %APPDATA%\Resonance.</p>
    </div>
  );
}
