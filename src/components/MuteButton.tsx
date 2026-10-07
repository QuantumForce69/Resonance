import { Mic, MicOff, VolumeX } from "lucide-react";

interface Props {
  muted: boolean;
  onToggle: () => void;
  label: string;
  kind?: "speaker" | "mic";
  /** "key": a labelled console key; "icon": a compact square key */
  variant?: "key" | "icon";
  className?: string;
}

export function MuteButton({ muted, onToggle, label, kind = "speaker", variant = "icon", className = "" }: Props) {
  const Icon = kind === "mic" ? (muted ? MicOff : Mic) : VolumeX;
  return (
    <button
      type="button"
      aria-pressed={muted}
      aria-label={`${muted ? "Unmute" : "Mute"} ${label}`}
      title={muted ? "Unmute" : "Mute"}
      onClick={onToggle}
      className={`key ${muted ? "key-lit" : ""} ${variant === "icon" ? "w-[30px] shrink-0 px-0!" : ""} ${className}`}
    >
      {variant === "key" ? (muted ? "Muted" : "Mute") : <Icon size={14} strokeWidth={2} />}
    </button>
  );
}
