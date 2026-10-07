import type { HotkeyMode } from "../lib/types";

const MODES: { value: HotkeyMode; label: string }[] = [
  { value: "toggle", label: "Toggle" },
  { value: "pushToTalk", label: "Push to talk" },
  { value: "pushToMute", label: "Push to mute" },
];

/** Three-position lever switch for the mute shortcut's behaviour. */
export function ModeSwitch({ value, onChange }: { value: HotkeyMode; onChange: (m: HotkeyMode) => void }) {
  const index = Math.max(0, MODES.findIndex((m) => m.value === value));
  return (
    <div className="w-[300px] max-w-full">
      <div aria-hidden className="relative h-[30px] rounded-lg bg-[var(--slot)] shadow-[inset_0_0_0_1px_var(--groove),inset_0_2px_5px_rgb(0_0_0/0.6)]">
        <div
          className="absolute top-[3px] bottom-[3px] w-[calc(33.33%-4px)] rounded-md border border-[var(--cap-edge)] bg-linear-to-b from-[var(--cap-hi)] to-[var(--cap-lo)] shadow-[0_3px_8px_var(--shadow)] transition-[left] duration-150"
          style={{ left: `calc(${index * 33.33}% + 3px)` }}
        >
          <div className="absolute top-[7px] bottom-[7px] left-1/2 -ml-px w-0.5 rounded-sm bg-accent shadow-[0_0_6px_var(--accent)]" />
        </div>
      </div>
      <div role="radiogroup" aria-label="Shortcut behaviour" className="mt-1.5 flex text-center">
        {MODES.map((m, i) => (
          <button
            key={m.value}
            type="button"
            role="radio"
            aria-checked={i === index}
            onClick={() => onChange(m.value)}
            className={`flex-1 border-0 bg-transparent py-1.5 text-[12px] ${i === index ? "font-semibold text-paper" : "text-dim hover:text-fog"}`}
          >
            {m.label}
          </button>
        ))}
      </div>
    </div>
  );
}
