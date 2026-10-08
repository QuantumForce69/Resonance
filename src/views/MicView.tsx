import { Mic, MicOff } from "lucide-react";
import { defaultMic, run, saveMicConfig, toggleMicMute } from "../lib/store";
import type { UiState } from "../lib/types";
import { BoostKnob } from "../components/BoostKnob";
import { DbScale } from "../components/DbScale";
import { HotkeyRecorder } from "../components/HotkeyRecorder";
import { Meter } from "../components/Meter";
import { ModeSwitch } from "../components/ModeSwitch";
import { Slider } from "../components/Slider";
import { Banner, Field, Panel, ViewHeader } from "../components/ui";
import { isProcessed } from "../lib/processing";
import { ProcessingPanel } from "./ProcessingPanel";

export function MicView({ state }: { state: UiState }) {
  const mic = defaultMic(state);
  const all = state.devices.filter((d) => d.flow === "capture" && d.state === "active");
  const comms = all.find((d) => d.defaultComms);

  if (!mic) {
    return (
      <div className="flex max-w-5xl flex-col gap-5">
        <ViewHeader title="Microphone" />
        <Banner>No microphone found. Plug one in or enable it in Windows sound settings.</Banner>
      </div>
    );
  }
  const live = !mic.muted;

  return (
    <div className="flex flex-col gap-5">
      <ViewHeader
        title="Microphone"
        subtitle={isProcessed(state) ? "Apps hear your mic through Resonance's processing" : `${mic.name} is the mic every app hears by default`}
      />
      <div className="flex flex-wrap items-stretch gap-5">
        <section aria-label="Input channel" className="console flex min-w-0 flex-[3_1_560px] flex-col gap-6 px-7 py-6">
          <div className="flex items-center gap-4">
            <button
              type="button"
              aria-pressed={mic.muted}
              aria-label={live ? "Mute microphone" : "Unmute microphone"}
              onClick={() => toggleMicMute(mic)}
              className={`grid h-16 w-16 shrink-0 place-items-center rounded-xl border ${live ? "mic-live text-lilac" : "mic-muted text-coral"}`}
            >
              {live ? <Mic size={26} strokeWidth={1.8} /> : <MicOff size={26} strokeWidth={1.8} />}
            </button>
            <div className="min-w-0 flex-1">
              <div className={`flex items-center gap-2 text-[13px] ${live ? "text-lilac" : "text-coral"}`}>
                <span className={`h-2 w-2 rounded-full ${live ? "led-live" : "led-muted"}`} />
                {live ? "Live, apps can hear you" : "Muted, apps can't hear you"}
              </div>
              <div className="mt-1 truncate text-[20px] font-semibold tracking-[-0.015em]">{mic.name}</div>
            </div>
          </div>

          <div>
            <Meter source="mic" className="h-3!" />
            <div className="mt-1.5">
              <DbScale kind="meter" orientation="horizontal" />
            </div>
          </div>

          <div className="flex items-center gap-6 border-t border-groove pt-6">
            <div className="min-w-0 flex-1">
              <div className="mb-1.5 flex items-baseline justify-between">
                <span className="text-[13px] text-fog">Input level</span>
                <span className="num text-[14px]">{Math.round(mic.volume * 100)}%</span>
              </div>
              <Slider value={mic.volume} label="Input level" onChange={(v) => run({ type: "setDeviceVolume", id: mic.id, volume: v })} />
            </div>
            {mic.boost && <BoostKnob mic={mic} />}
          </div>
        </section>

        <Panel className="min-w-0 flex-[2_1_380px] px-6 py-1.5">
          <Field label="Default microphone" hint="What apps hear when they use the default mic" htmlFor="mic-default">
            <select id="mic-default" className="field-select" value={mic.id} onChange={(e) => run({ type: "setDefaultDevice", id: e.currentTarget.value, role: "console" })}>
              {all.map((d) => (
                <option key={d.id} value={d.id}>
                  {d.name}
                </option>
              ))}
            </select>
          </Field>
          <Field label="For calls" hint="Discord, Teams and Zoom use this when set to Default" htmlFor="mic-comms">
            <select
              id="mic-comms"
              className="field-select"
              value={comms?.id ?? ""}
              onChange={(e) => e.currentTarget.value && run({ type: "setDefaultDevice", id: e.currentTarget.value, role: "communications" })}
            >
              {!comms && <option value="">None</option>}
              {all.map((d) => (
                <option key={d.id} value={d.id}>
                  {d.name}
                </option>
              ))}
            </select>
          </Field>
          <Field label="Mute shortcut" hint="Works everywhere, including fullscreen games">
            <HotkeyRecorder config={state.mic} />
          </Field>
          <Field label="Shortcut behaviour" hint="Push to talk keeps you muted until you hold the keys">
            <ModeSwitch value={state.mic.mode} onChange={(mode) => saveMicConfig({ ...state.mic, mode })} />
          </Field>
        </Panel>
      </div>
      <ProcessingPanel state={state} />
    </div>
  );
}
