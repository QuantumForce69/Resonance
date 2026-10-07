import type { HotkeyMode } from "../lib/types";
import { LeverSwitch } from "./LeverSwitch";

const MODES: { value: HotkeyMode; label: string }[] = [
  { value: "toggle", label: "Toggle" },
  { value: "pushToTalk", label: "Push to talk" },
  { value: "pushToMute", label: "Push to mute" },
];

/** Three-position lever for the mute shortcut's behaviour. */
export function ModeSwitch({ value, onChange }: { value: HotkeyMode; onChange: (m: HotkeyMode) => void }) {
  return <LeverSwitch label="Shortcut behaviour" value={value} options={MODES} onChange={onChange} />;
}
