import { useMemo, useRef, useState } from "react";
import { formatDb } from "../lib/levels";
import { throttle } from "../lib/throttle";

interface Props {
  value: number;
  onChange: (value: number) => void;
  label: string;
  orientation?: "horizontal" | "vertical";
  disabled?: boolean;
  className?: string;
}

/** Hardware-style fader (0..1). Holds its local value while dragging so backend echoes can't make it jump. */
export function Slider({ value, onChange, label, orientation = "horizontal", disabled, className = "" }: Props) {
  const [local, setLocal] = useState<number | null>(null);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;
  const emit = useMemo(() => throttle((v: number) => onChangeRef.current(v), 16), []);
  const shown = local ?? value;

  const release = () => {
    if (local !== null) onChangeRef.current(local);
    setLocal(null);
  };

  return (
    <input
      type="range"
      min={0}
      max={100}
      step={1}
      value={Math.round(shown * 100)}
      aria-label={label}
      aria-valuetext={formatDb(shown)}
      disabled={disabled}
      className={`fader ${orientation === "vertical" ? "vertical" : ""} ${className}`}
      onChange={(e) => {
        const v = Number(e.currentTarget.value) / 100;
        setLocal(v);
        emit(v);
      }}
      onPointerUp={release}
      onKeyUp={release}
      onBlur={() => setLocal(null)}
    />
  );
}
