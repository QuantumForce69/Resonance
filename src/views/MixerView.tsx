import { ChevronDown } from "lucide-react";
import { formatDb } from "../lib/levels";
import { defaultOutput, outputs, run, toggleDeviceMute } from "../lib/store";
import type { UiState } from "../lib/types";
import { DbScale } from "../components/DbScale";
import { Meter } from "../components/Meter";
import { MuteButton } from "../components/MuteButton";
import { Slider } from "../components/Slider";
import { Banner, ViewHeader } from "../components/ui";
import { callReady } from "../lib/processing";
import { ChannelStrip } from "./ChannelStrip";

function subtitle(state: UiState, outName: string | undefined) {
  const playing = state.apps.filter((a) => !a.isSystem).length;
  if (!outName) return "No output device";
  if (playing === 0) return `Nothing playing through ${outName}`;
  return `${playing} ${playing === 1 ? "app" : "apps"} playing through ${outName}`;
}

export function MixerView({ state }: { state: UiState }) {
  const out = defaultOutput(state);
  const outs = outputs(state);
  const call = !state.processing.callAvailable ? "unavailable" : callReady(state) ? "ready" : "off";

  return (
    <div className="flex h-full flex-col gap-5">
      <ViewHeader title="Mixer" subtitle={subtitle(state, out?.name)} />
      {!state.routingAvailable && (
        <Banner>Per-app output selection isn't available on this Windows build. Volume and mute still work.</Banner>
      )}
      <div className="console flex min-h-[560px] flex-1 overflow-hidden">
        <div className="flex min-w-0 flex-1 overflow-x-auto">
          {state.apps.length === 0 ? (
            <div className="grid flex-1 place-items-center text-[13px] text-dim">Apps appear here as soon as they play sound.</div>
          ) : (
            state.apps.map((a) => <ChannelStrip key={a.key} app={a} call={call} />)
          )}
        </div>

        <section
          aria-label="Master output"
          className="flex w-[212px] shrink-0 flex-col gap-3.5 border-l border-groove bg-linear-to-b from-raised to-console px-5 pt-[18px] pb-4"
        >
          <div className="flex min-h-16 flex-col justify-center gap-0.5">
            <div className="text-[12px] text-fog">Master output</div>
            {out ? (
              <label className="relative block">
                <select
                  aria-label="Master output device"
                  value={out.id}
                  onChange={(e) => run({ type: "setDefaultDevice", id: e.currentTarget.value, role: "console" })}
                  className="w-full appearance-none truncate border-0 bg-transparent py-0.5 pr-5 text-[15px] font-semibold tracking-[-0.01em] text-paper"
                >
                  {outs.map((d) => (
                    <option key={d.id} value={d.id} className="bg-[var(--option-bg)]">
                      {d.name}
                    </option>
                  ))}
                </select>
                <ChevronDown size={12} className="pointer-events-none absolute top-2 right-0 text-fog" />
              </label>
            ) : (
              <div className="text-[15px] font-semibold text-dim">None</div>
            )}
          </div>
          {out && (
            <>
              <div className="flex min-h-[300px] flex-1 justify-center gap-2">
                <DbScale kind="fader" />
                <Slider orientation="vertical" value={out.volume} label="Master volume" onChange={(v) => run({ type: "setDeviceVolume", id: out.id, volume: v })} />
                <Meter orientation="vertical" source="master" className="ml-1 w-2!" />
              </div>
              <div className={`num text-center text-[22px] font-medium tracking-[-0.02em] ${out.muted ? "text-coral" : ""}`}>
                {out.muted ? "Muted" : formatDb(out.volume)}
              </div>
              <MuteButton variant="key" muted={out.muted} label={out.name} className="w-full" onToggle={() => toggleDeviceMute(out)} />
            </>
          )}
        </section>
      </div>
    </div>
  );
}
