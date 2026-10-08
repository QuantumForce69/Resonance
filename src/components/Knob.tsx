import { useMemo, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import { throttle } from "../lib/throttle";

interface Props {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  onChange: (value: number) => void;
  format: (value: number) => string;
  size?: "main" | "small";
  disabled?: boolean;
}

/** Pixels of vertical drag that sweep the whole range. */
const DRAG_PX = 200;
/** The pointer sweeps ±135° from straight up. */
const SWEEP = 135;

const snap = (v: number, min: number, max: number, step: number) => {
  const stepped = Math.round((v - min) / step) * step + min;
  return Math.min(max, Math.max(min, Math.round(stepped * 1000) / 1000));
};

function point(c: number, r: number, deg: number) {
  const rad = (deg * Math.PI) / 180;
  return { x: c + r * Math.sin(rad), y: c - r * Math.cos(rad) };
}

function arc(c: number, r: number, from: number, to: number) {
  const a = point(c, r, from);
  const b = point(c, r, to);
  return `M ${a.x} ${a.y} A ${r} ${r} 0 ${to - from > 180 ? 1 : 0} 1 ${b.x} ${b.y}`;
}

/** Rotary control: drag up/down or use the arrow keys. Holds its local value while dragging. */
export function Knob({ label, value, min, max, step, onChange, format, size = "main", disabled = false }: Props) {
  const [local, setLocal] = useState<number | null>(null);
  const drag = useRef<{ y: number; start: number } | null>(null);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;
  const emit = useMemo(() => throttle((v: number) => onChangeRef.current(v), 33), []);

  const shown = local ?? value;
  const px = size === "main" ? 76 : 48;
  const stroke = size === "main" ? 4 : 3;
  const c = px / 2;
  const r = c - stroke - 2;
  const cap = r - (size === "main" ? 9 : 6);
  const angle = -SWEEP + ((shown - min) / (max - min)) * 2 * SWEEP;
  const inner = point(c, cap * 0.3, angle);
  const outer = point(c, cap * 0.85, angle);

  const commit = (v: number) => {
    const s = snap(v, min, max, step);
    if (s !== value) onChange(s);
  };

  const onKeyDown = (e: KeyboardEvent) => {
    const keys: Record<string, number> = {
      ArrowUp: value + step,
      ArrowRight: value + step,
      ArrowDown: value - step,
      ArrowLeft: value - step,
      PageUp: value + step * 10,
      PageDown: value - step * 10,
      Home: min,
      End: max,
    };
    const next = keys[e.key];
    if (next === undefined) return;
    e.preventDefault();
    commit(next);
  };

  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    if (disabled) return;
    e.currentTarget.setPointerCapture?.(e.pointerId);
    drag.current = { y: e.clientY, start: value };
    setLocal(value);
  };

  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    if (!drag.current) return;
    const v = snap(drag.current.start + ((drag.current.y - e.clientY) / DRAG_PX) * (max - min), min, max, step);
    setLocal(v);
    emit(v);
  };

  const onPointerUp = () => {
    if (!drag.current) return;
    drag.current = null;
    if (local !== null && local !== value) onChangeRef.current(local);
    setLocal(null);
  };

  return (
    <div className={`flex flex-col items-center gap-1 ${disabled ? "opacity-45" : ""}`}>
      <div
        role="slider"
        tabIndex={disabled ? -1 : 0}
        aria-label={label}
        aria-valuemin={min}
        aria-valuemax={max}
        aria-valuenow={shown}
        aria-valuetext={format(shown)}
        aria-disabled={disabled || undefined}
        onKeyDown={disabled ? undefined : onKeyDown}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
        className={`touch-none rounded-full outline-none focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent ${disabled ? "cursor-default" : "cursor-ns-resize"}`}
        style={{ width: px, height: px }}
      >
        <svg width={px} height={px} viewBox={`0 0 ${px} ${px}`} aria-hidden>
          <path d={arc(c, r, -SWEEP, SWEEP)} fill="none" stroke="var(--groove)" strokeWidth={stroke} strokeLinecap="round" />
          {angle > -SWEEP + 0.5 && (
            <path d={arc(c, r, -SWEEP, angle)} fill="none" stroke="var(--accent)" strokeWidth={stroke} strokeLinecap="round" />
          )}
          <circle cx={c} cy={c} r={cap} fill="var(--raised)" stroke="var(--cap-edge)" strokeWidth="1" />
          <line x1={inner.x} y1={inner.y} x2={outer.x} y2={outer.y} stroke="var(--paper)" strokeWidth="2.5" strokeLinecap="round" />
        </svg>
      </div>
      <div className={`num text-paper ${size === "main" ? "text-[14px]" : "text-[12px]"}`}>{format(shown)}</div>
      <div className="text-[11px] text-dim">{label}</div>
    </div>
  );
}
