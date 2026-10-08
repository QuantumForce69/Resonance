import { formatDb } from "../lib/levels";
import { CALL_OFF_HINT, pct } from "../lib/processing";
import { run, setAppSend, toggleAppMute, toggleAppSend } from "../lib/store";
import type { App } from "../lib/types";
import { AppIcon } from "../components/AppIcon";
import { DbScale } from "../components/DbScale";
import { DeviceSelect } from "../components/DeviceSelect";
import { Knob } from "../components/Knob";
import { Meter } from "../components/Meter";
import { MuteButton } from "../components/MuteButton";
import { Slider } from "../components/Slider";

export type CallState = "ready" | "off" | "unavailable";

/** Fixed height of the Call key + send knob, so every strip's fader stays the same length. */
const CALL_AREA = "h-[118px] shrink-0";

/** The strip's "Call" key and, while sent, its send-level knob. */
function CallSend({ app, ready }: { app: App; ready: boolean }) {
  const sent = app.send !== null;
  return (
    <div className={`flex flex-col items-center gap-1.5 ${CALL_AREA}`}>
      <button
        type="button"
        aria-pressed={sent}
        aria-label={`Send ${app.name} into calls`}
        disabled={!ready}
        title={ready ? undefined : CALL_OFF_HINT}
        onClick={() => void toggleAppSend(app)}
        className={`key h-7 w-full text-[12px] disabled:cursor-default disabled:opacity-45 ${sent && ready ? "mic-live text-lilac" : ""}`}
      >
        Call
      </button>
      {sent && ready && (
        <Knob size="small" label="Send" value={app.send ?? 0} min={0} max={1} step={0.05} format={pct} onChange={(v) => setAppSend(app, v)} />
      )}
    </div>
  );
}

/** One app channel on the console: identity, fader with engraved scale, ladder meter, readout, mute key, output patch. */
export function ChannelStrip({ app, call }: { app: App; call: CallState }) {
  return (
    <section
      aria-label={app.name}
      className="flex w-[124px] shrink-0 flex-col gap-3.5 border-r border-groove px-3.5 pt-[18px] pb-4 shadow-[1px_0_0_rgb(255_255_255/0.025)]"
    >
      <div className="flex min-h-16 flex-col items-center gap-2">
        <AppIcon app={app} size={34} />
        <div className="w-full truncate text-center text-[13px] font-medium" title={app.name}>
          {app.name}
        </div>
      </div>
      <div className="flex min-h-[300px] flex-1 justify-center gap-1.5">
        <DbScale kind="fader" />
        <Slider orientation="vertical" value={app.volume} label={`${app.name} volume`} onChange={(v) => run({ type: "setAppVolume", key: app.key, volume: v })} />
        <Meter orientation="vertical" source={`app:${app.key}`} className="ml-0.5" />
      </div>
      <div className={`num text-center text-[13px] ${app.muted ? "text-coral" : "text-fog"}`}>{app.muted ? "Muted" : formatDb(app.volume)}</div>
      <div className="flex justify-center">
        <MuteButton variant="key" muted={app.muted} label={app.name} onToggle={() => toggleAppMute(app)} />
      </div>
      {call !== "unavailable" && (app.isSystem ? <div aria-hidden className={CALL_AREA} /> : <CallSend app={app} ready={call === "ready"} />)}
      {app.isSystem ? <div className="py-1.5 text-center text-[11px] text-dim">Follows default</div> : <DeviceSelect app={app} />}
    </section>
  );
}
