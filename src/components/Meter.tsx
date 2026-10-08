import { useEffect, useRef } from "react";
import { registerMeter } from "../lib/meters";

interface Props {
  /** "master", "mic", "procIn", "procOut" or "app:<key>" */
  source: string;
  orientation?: "horizontal" | "vertical";
  className?: string;
}

/** Segmented ladder meter driven by the rAF meter engine (no React renders). */
export function Meter({ source, orientation = "horizontal", className = "" }: Props) {
  const cover = useRef<HTMLDivElement>(null);
  const peak = useRef<HTMLDivElement>(null);
  const vertical = orientation === "vertical";

  useEffect(() => {
    if (!cover.current || !peak.current) return;
    return registerMeter(source, vertical, cover.current, peak.current);
  }, [source, vertical]);

  return (
    <div aria-hidden className={`ladder ${vertical ? "vertical h-full w-[6px]" : "h-1 w-full"} ${className}`}>
      <div className="ladder-fill" />
      <div ref={cover} className="ladder-cover" />
      <div ref={peak} className="ladder-peak" />
    </div>
  );
}
