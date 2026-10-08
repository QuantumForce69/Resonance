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
  const fill = useRef<HTMLDivElement>(null);
  const peak = useRef<HTMLDivElement>(null);
  const vertical = orientation === "vertical";

  useEffect(() => {
    if (!fill.current || !peak.current) return;
    return registerMeter(source, vertical, fill.current, peak.current);
  }, [source, vertical]);

  return (
    <div aria-hidden className={`ladder ${vertical ? "vertical h-full w-[6px]" : "h-1 w-full"} ${className}`}>
      <div className="ladder-off" />
      <div ref={fill} className="ladder-fill" style={{ clipPath: vertical ? "inset(100% 0 0 0)" : "inset(0 100% 0 0)" }} />
      <div ref={peak} className="ladder-peak" />
    </div>
  );
}
