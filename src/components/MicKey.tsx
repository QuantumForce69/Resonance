import { run } from "../lib/store";
import type { Device } from "../lib/types";
import { Meter } from "./Meter";
import { Keycaps } from "./ui";

/** The always-visible mic key: violet glow when live, lit coral when muted. */
export function MicKey({ mic, hotkey, size = "compact" }: { mic: Device | undefined; hotkey: string | null; size?: "compact" | "wide" }) {
  if (!mic) return <div className="px-2 text-[12px] text-dim">No microphone</div>;
  const live = !mic.muted;
  const shortcut = hotkey ? hotkey.split("+").join(" ") : null;
  return (
    <button
      type="button"
      aria-pressed={mic.muted}
      aria-label={live ? "Microphone is live. Mute microphone" : "Microphone is muted. Unmute microphone"}
      onClick={() => run({ type: "setMicMute", muted: live })}
      className={`flex items-center gap-2.5 rounded-lg border text-[12px] transition-colors ${
        size === "wide" ? "w-full px-3 py-2.5 text-left" : "h-8 px-3"
      } ${live ? "mic-live" : "mic-muted"}`}
    >
      <span className={`h-2 w-2 shrink-0 rounded-full ${live ? "led-live" : "led-muted"}`} />
      <span className="flex min-w-0 flex-1 flex-col items-start gap-px">
        <span className="text-[13px] font-medium text-paper">{live ? "Mic live" : "Mic muted"}</span>
        {size === "wide" && (
          <span className="w-full truncate text-[11px] text-fog">
            {shortcut ? `${mic.name}, press ${shortcut} to ${live ? "mute" : "unmute"}` : mic.name}
          </span>
        )}
      </span>
      {size === "compact" && hotkey && <Keycaps combo={hotkey} small />}
      {size === "wide" && <Meter source="mic" className="w-16!" />}
    </button>
  );
}
