import type { CSSProperties } from "react";
import { dbToFaderPosition, MIN_DB } from "../lib/levels";

const FADER_TICKS = [0, -10, -20, -30, -40];
const METER_TICKS = [-60, -40, -20, -10, 0];

const label = (db: number) => (db === 0 ? "0" : `−${-db}`);

/** Engraved scale next to a fader ("fader": 40·log10 curve) or under a meter ("meter": linear dB). */
export function DbScale({ kind, orientation = "vertical" }: { kind: "fader" | "meter"; orientation?: "horizontal" | "vertical" }) {
  const vertical = orientation === "vertical";
  const ticks = kind === "fader" ? FADER_TICKS : METER_TICKS;
  const pos = (db: number) => (kind === "fader" ? dbToFaderPosition(db) : (db - MIN_DB) / -MIN_DB) * 100;
  const place = (p: number): CSSProperties =>
    vertical
      ? { position: "absolute", right: 0, bottom: `${p}%`, transform: "translateY(50%)" }
      : { position: "absolute", left: `${p}%`, transform: p >= 100 ? "translateX(-100%)" : p <= 0 ? "none" : "translateX(-50%)" };

  return (
    <div aria-hidden className={`num relative text-[10px] text-dim ${vertical ? "h-full w-6" : "h-3.5 w-full"}`}>
      {ticks.map((db) => (
        <span key={db} style={place(pos(db))}>
          {label(db)}
        </span>
      ))}
      {kind === "fader" && <span style={place(0)}>−∞</span>}
    </div>
  );
}
