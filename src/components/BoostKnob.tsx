import { run } from "../lib/store";
import type { Device } from "../lib/types";
import { Segmented } from "./ui";

/** Detent angles for up to four positions, sweeping 270° from bottom-left to bottom-right. */
const ANGLES = [-135, -45, 45, 135];
const LABEL_SPOTS = [
  { left: 4, bottom: 6 },
  { left: 0, top: 4 },
  { right: 0, top: 4 },
  { right: 4, bottom: 6 },
];

/** Rotary mic-boost knob with clickable detents; falls back to a segmented control for unusual ranges. */
export function BoostKnob({ mic }: { mic: Device }) {
  const b = mic.boost!;
  const steps: number[] = [];
  if (b.step > 0) for (let v = b.min; v <= b.max + 1e-3; v += b.step) steps.push(Math.round(v * 10) / 10);
  const current = Math.round(b.db * 10) / 10;
  const label = (v: number) => (v === 0 ? "Off" : `+${v} dB`);
  const set = (db: number) => run({ type: "setMicBoost", id: mic.id, db });

  if (steps.length < 2 || steps.length > 4) {
    return <Segmented label="Microphone boost" value={current} options={steps.map((v) => ({ value: v, label: label(v) }))} onChange={set} />;
  }

  const angles = steps.length === 4 ? ANGLES : steps.map((_, i) => -135 + (270 * i) / (steps.length - 1));
  const index = Math.max(0, steps.findIndex((v) => v === current));
  const rad = ((angles[index] - 90) * Math.PI) / 180;
  const tip = { x: 84 + Math.cos(rad) * 25, y: 78 + Math.sin(rad) * 25 };
  const dot = (a: number) => {
    const r = ((a - 90) * Math.PI) / 180;
    return { x: 84 + Math.cos(r) * 50, y: 78 + Math.sin(r) * 50 };
  };

  return (
    <div className="relative h-[150px] w-[168px] shrink-0" role="radiogroup" aria-label="Microphone boost">
      <svg width="168" height="150" viewBox="0 0 168 150" aria-hidden className="absolute inset-0">
        {angles.map((a, i) => {
          const p = dot(a);
          return <circle key={a} cx={p.x} cy={p.y} r="2.5" fill={i === index ? "var(--accent)" : "var(--cap-edge)"} />;
        })}
        <circle cx="84" cy="78" r="36" fill="var(--raised)" stroke="var(--cap-edge)" strokeWidth="1" />
        <circle cx="84" cy="78" r="33" fill="none" stroke="var(--groove)" strokeWidth="5" strokeDasharray="1.5 3" />
        <circle cx="84" cy="78" r="26" fill="var(--console)" />
        <line x1="84" y1="78" x2={tip.x} y2={tip.y} stroke="var(--paper)" strokeWidth="3" strokeLinecap="round" />
        <circle cx="84" cy="78" r="3" fill="var(--paper)" />
      </svg>
      {steps.map((v, i) => (
        <button
          key={v}
          type="button"
          role="radio"
          aria-checked={i === index}
          onClick={() => set(v)}
          style={{ position: "absolute", ...(steps.length === 4 ? LABEL_SPOTS[i] : { left: `${(i / (steps.length - 1)) * 80}%`, bottom: 0 }) }}
          className={`border-0 bg-transparent p-1 text-[12px] ${i === index ? "font-semibold text-paper" : "text-dim hover:text-fog"}`}
        >
          {label(v)}
        </button>
      ))}
      <div className="absolute right-0 -bottom-[18px] left-0 text-center text-[11px] text-dim">Boost</div>
    </div>
  );
}
