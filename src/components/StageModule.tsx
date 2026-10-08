import { useId, useState, type ReactNode } from "react";
import { ChevronDown } from "lucide-react";
import { Toggle } from "./ui";

interface Props {
  title: string;
  enabled: boolean;
  onToggle: (enabled: boolean) => void;
  /** The stage's main control. */
  children: ReactNode;
  /** Extra controls shown in the Advanced drawer. */
  advanced?: ReactNode;
}

/** One processing stage: on/off switch, its main control, and an optional Advanced drawer. */
export function StageModule({ title, enabled, onToggle, children, advanced }: Props) {
  const [open, setOpen] = useState(false);
  const drawer = useId();
  return (
    <section aria-label={title} className="flex min-w-0 flex-col rounded-xl border border-groove bg-night/40 p-4">
      <div className="flex items-center justify-between gap-3">
        <h3 className={`m-0 text-[14px] font-medium ${enabled ? "text-paper" : "text-dim"}`}>{title}</h3>
        <Toggle checked={enabled} onChange={onToggle} label={title} />
      </div>
      <div className={`flex flex-1 justify-center py-4 ${enabled ? "" : "opacity-50"}`}>{children}</div>
      {advanced && (
        <>
          <button
            type="button"
            aria-expanded={open}
            aria-controls={drawer}
            onClick={() => setOpen(!open)}
            className="flex items-center justify-center gap-1 border-0 bg-transparent text-[12px] text-fog hover:text-paper"
          >
            Advanced
            <ChevronDown size={14} aria-hidden className={`transition-transform ${open ? "rotate-180" : ""}`} />
          </button>
          {open && (
            <div id={drawer} className={`mt-3 flex flex-wrap justify-center gap-3 border-t border-groove pt-3 ${enabled ? "" : "opacity-50"}`}>
              {advanced}
            </div>
          )}
        </>
      )}
    </section>
  );
}
