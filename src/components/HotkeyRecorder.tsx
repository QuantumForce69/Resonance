import { useEffect, useState } from "react";
import { formatHotkey } from "../lib/hotkey";
import { saveMicConfig } from "../lib/store";
import type { MicConfig } from "../lib/types";
import { Keycaps } from "./ui";

export function HotkeyRecorder({ config }: { config: MicConfig }) {
  const [recording, setRecording] = useState(false);

  useEffect(() => {
    if (!recording) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") return setRecording(false);
      if (e.key === "Backspace" || e.key === "Delete") {
        setRecording(false);
        saveMicConfig({ ...config, hotkey: null });
        return;
      }
      const hk = formatHotkey(e);
      if (hk) {
        setRecording(false);
        saveMicConfig({ ...config, hotkey: hk });
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [recording, config]);

  return (
    <div className="flex flex-col items-start gap-1.5">
      <button
        type="button"
        onClick={() => setRecording((r) => !r)}
        aria-label={recording ? "Recording shortcut" : config.hotkey ? `Mute shortcut is ${config.hotkey}. Change shortcut` : "Set a mute shortcut"}
        className="flex items-center gap-1.5 border-0 bg-transparent p-0"
      >
        {recording ? (
          <span className="mic-live rounded-lg border px-3 py-1.5 text-[13px]">Press a key combination…</span>
        ) : config.hotkey ? (
          <Keycaps combo={config.hotkey} />
        ) : (
          <span className="cap">Not set</span>
        )}
        {!recording && <span className="ml-2 text-[12px] text-lilac">Change</span>}
      </button>
      {recording && <span className="text-[11px] text-dim">Esc to cancel, Backspace to remove the shortcut</span>}
    </div>
  );
}
