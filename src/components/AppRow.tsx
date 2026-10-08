import { formatDb } from "../lib/levels";
import { run, toggleAppMute } from "../lib/store";
import type { App } from "../lib/types";
import { AppIcon } from "./AppIcon";
import { Meter } from "./Meter";
import { MuteButton } from "./MuteButton";
import { Slider } from "./Slider";

export function AppRow({ app }: { app: App }) {
  return (
    <div className="flex items-center gap-3 px-4 py-2">
      <AppIcon app={app} size={30} />
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline justify-between gap-2">
          <span className={`truncate text-[13px] ${app.muted ? "text-dim" : "text-paper"}`}>{app.name}</span>
          <span className={`num text-[11px] ${app.muted ? "text-coral" : "text-dim"}`}>{app.muted ? "Muted" : formatDb(app.volume)}</span>
        </div>
        <Slider value={app.volume} label={`${app.name} volume`} onChange={(v) => run({ type: "setAppVolume", key: app.key, volume: v })} />
        <Meter source={`app:${app.key}`} className="h-[3px]!" />
      </div>
      <MuteButton muted={app.muted} label={app.name} onToggle={() => toggleAppMute(app)} />
    </div>
  );
}
