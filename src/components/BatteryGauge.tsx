import { Zap } from "lucide-react";
import type { Battery } from "../lib/types";

/** Segmented cell gauge; the Space One reports 5 steps, so no fake percentage. */
export function BatteryGauge({ battery, size = "md" }: { battery: Battery; size?: "sm" | "md" }) {
  const low = battery.level <= 1;
  const cell = size === "sm" ? "h-2.5 w-[7px]" : "h-4 w-3";
  const label = `Battery ${battery.level} of ${battery.max}${battery.charging ? ", charging" : ""}`;
  return (
    <div role="img" aria-label={label} title={label} className="flex items-center gap-1.5">
      <div className="flex items-center gap-[3px] rounded-[5px] border border-groove bg-[var(--slot)] p-[3px]">
        {Array.from({ length: battery.max }, (_, i) => (
          <span
            key={i}
            className={`${cell} rounded-[2px] ${
              i < battery.level
                ? low
                  ? "bg-coral shadow-[0_0_6px_var(--coral)]"
                  : "bg-lilac shadow-[0_0_6px_rgb(201_184_255/0.5)]"
                : "bg-[var(--ladder-off)]"
            }`}
          />
        ))}
      </div>
      {battery.charging && <Zap size={size === "sm" ? 12 : 14} className="text-lilac" aria-hidden />}
    </div>
  );
}
