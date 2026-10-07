import { useEffect } from "react";
import { Maximize2 } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { showMain } from "../lib/api";
import { formatDb } from "../lib/levels";
import { defaultMic, defaultOutput, run, useStore } from "../lib/store";
import { AppRow } from "../components/AppRow";
import { DbScale } from "../components/DbScale";
import { Meter } from "../components/Meter";
import { MicKey } from "../components/MicKey";
import { MuteButton } from "../components/MuteButton";
import { Slider } from "../components/Slider";
import { Toasts } from "../components/Toasts";

export function Flyout() {
  const state = useStore((s) => s.state);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") getCurrentWindow().hide();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  if (!state) {
    return <div className="grid h-full place-items-center text-[13px] text-dim">Connecting to audio…</div>;
  }
  const out = defaultOutput(state);
  const mic = defaultMic(state);

  return (
    <div className="flex h-full flex-col overflow-hidden rounded-xl border border-groove bg-night">
      <section aria-label="Master output" className="border-b border-groove bg-linear-to-b from-raised to-console px-[18px] pt-[18px] pb-4">
        <div className="flex items-start justify-between gap-3">
          <div className="min-w-0">
            <div className="text-[12px] text-fog">Playing through</div>
            <div className="mt-0.5 truncate text-[16px] font-semibold tracking-[-0.01em]">{out?.name ?? "No output device"}</div>
          </div>
          <button
            type="button"
            aria-label="Open Resonance"
            title="Open Resonance"
            onClick={() => showMain()}
            className="-mt-1 -mr-1.5 grid h-[30px] w-[30px] place-items-center rounded-lg text-fog hover:bg-raised hover:text-paper"
          >
            <Maximize2 size={15} />
          </button>
        </div>
        {out && (
          <div className="mt-3.5 flex items-end gap-3">
            <div className="min-w-0 flex-1">
              <Meter source="master" className="mb-1.5" />
              <Slider value={out.volume} label={`${out.name} volume`} onChange={(v) => run({ type: "setDeviceVolume", id: out.id, volume: v })} />
              <DbScale kind="fader" orientation="horizontal" />
            </div>
            <div className="flex flex-col items-end gap-2">
              <div className={`num text-[22px] leading-none font-medium tracking-[-0.02em] ${out.muted ? "text-coral" : ""}`}>
                {out.muted ? "Muted" : formatDb(out.volume)}
              </div>
              <MuteButton muted={out.muted} label={out.name} onToggle={() => run({ type: "setDeviceMute", id: out.id, muted: !out.muted })} />
            </div>
          </div>
        )}
      </section>

      <div className="border-b border-groove px-3.5 py-3">
        <MicKey mic={mic} hotkey={state.mic.hotkey} size="wide" />
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto py-1.5">
        {state.apps.length === 0 ? (
          <p className="px-6 py-10 text-center text-[13px] text-dim">No apps are playing audio right now.</p>
        ) : (
          state.apps.map((a) => <AppRow key={a.key} app={a} />)
        )}
      </div>
      <Toasts />
    </div>
  );
}
