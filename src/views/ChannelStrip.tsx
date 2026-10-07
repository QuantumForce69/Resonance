import { formatDb } from "../lib/levels";
import { run } from "../lib/store";
import type { App } from "../lib/types";
import { AppIcon } from "../components/AppIcon";
import { DbScale } from "../components/DbScale";
import { DeviceSelect } from "../components/DeviceSelect";
import { Meter } from "../components/Meter";
import { MuteButton } from "../components/MuteButton";
import { Slider } from "../components/Slider";

/** One app channel on the console: identity, fader with engraved scale, ladder meter, readout, mute key, output patch. */
export function ChannelStrip({ app }: { app: App }) {
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
        <MuteButton variant="key" muted={app.muted} label={app.name} onToggle={() => run({ type: "setAppMute", key: app.key, muted: !app.muted })} />
      </div>
      {app.isSystem ? <div className="py-1.5 text-center text-[11px] text-dim">Follows default</div> : <DeviceSelect app={app} />}
    </section>
  );
}
