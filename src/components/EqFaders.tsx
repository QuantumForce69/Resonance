import type { HeadsetEq } from "../lib/types";
import { Slider } from "./Slider";

export function formatHz(hz: number): string {
  return hz >= 1000 ? `${hz / 1000}k` : `${hz}`;
}

/** Device units → signed dB text, e.g. 25 with 1 fraction digit → "+2.5". */
export function formatEqValue(value: number, fractionDigits: number): string {
  const db = value / 10 ** fractionDigits;
  const sign = db > 0 ? "+" : db < 0 ? "−" : "";
  return `${sign}${Math.abs(db).toFixed(fractionDigits)}`;
}

/** One vertical console fader per EQ band, scaled to the device's own range. */
export function EqFaders({ eq, disabled, onChange }: { eq: HeadsetEq; disabled?: boolean; onChange: (values: number[]) => void }) {
  const span = eq.max - eq.min;
  const toDevice = (x: number) => Math.round(eq.min + x * span);
  return (
    <div className="flex min-h-[260px] justify-between gap-2">
      {eq.bandHz.map((hz, i) => {
        const v = eq.values[i] ?? 0;
        return (
          <div key={hz} className="flex w-14 flex-col items-center gap-2">
            <div className="num text-[11px] text-fog">{formatEqValue(v, eq.fractionDigits)}</div>
            <div className="flex min-h-[190px] flex-1 justify-center">
              <Slider
                orientation="vertical"
                value={(v - eq.min) / span}
                label={`${formatHz(hz)} Hz`}
                disabled={disabled}
                valueText={(x) => `${formatEqValue(toDevice(x), eq.fractionDigits)} dB`}
                onChange={(x) => {
                  const next = [...eq.values];
                  next[i] = toDevice(x);
                  onChange(next);
                }}
              />
            </div>
            <div className="num text-[11px] text-dim">{formatHz(hz)}</div>
          </div>
        );
      })}
    </div>
  );
}
