import type { ReactNode } from "react";

export function Logo({ size = 20 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden>
      <circle cx="12" cy="12" r="2.4" fill="var(--accent)" />
      <circle cx="12" cy="12" r="6" fill="none" stroke="var(--accent)" strokeWidth="1.6" />
      <path d="M12 2.4a9.6 9.6 0 0 1 9.6 9.6" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
      <path d="M12 21.6A9.6 9.6 0 0 1 2.4 12" fill="none" stroke="currentColor" strokeOpacity="0.45" strokeWidth="1.6" strokeLinecap="round" />
    </svg>
  );
}

export function ViewHeader({ title, subtitle }: { title: string; subtitle?: string }) {
  return (
    <div className="flex flex-wrap items-baseline gap-x-5 gap-y-1">
      <h1 className="m-0 text-[30px] font-medium tracking-[-0.025em]">{title}</h1>
      {subtitle && <p className="m-0 text-[14px] text-fog">{subtitle}</p>}
    </div>
  );
}

export function Panel({ children, className = "" }: { children: ReactNode; className?: string }) {
  return <div className={`rounded-[14px] border border-groove bg-console ${className}`}>{children}</div>;
}

export function Banner({ children }: { children: ReactNode }) {
  return <div className="rounded-[10px] border border-groove bg-raised px-4 py-3 text-[13px] text-fog">{children}</div>;
}

export function Badge({ children, tone = "neutral" }: { children: ReactNode; tone?: "neutral" | "accent" | "muted" }) {
  const tones = {
    neutral: "border-groove text-fog",
    accent: "border-accent/50 text-lilac",
    muted: "border-groove text-dim",
  };
  return <span className={`rounded-[5px] border px-1.5 py-px text-[11px] ${tones[tone]}`}>{children}</span>;
}

export function IconButton({ label, onClick, children }: { label: string; onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      className="grid h-8 w-8 place-items-center rounded-lg text-fog transition-colors hover:bg-raised hover:text-paper"
    >
      {children}
    </button>
  );
}

export function Keycaps({ combo, small = false }: { combo: string; small?: boolean }) {
  return (
    <span className="flex items-center gap-1">
      {combo.split("+").map((k) => (
        <kbd key={k} className={`cap ${small ? "small" : ""}`}>
          {k}
        </kbd>
      ))}
    </span>
  );
}

export function Segmented<T extends string | number>({
  value,
  options,
  onChange,
  label,
  disabled = false,
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (v: T) => void;
  label: string;
  disabled?: boolean;
}) {
  return (
    <div role="radiogroup" aria-label={label} className={`inline-flex rounded-[9px] border border-groove bg-night p-0.5 ${disabled ? "opacity-45" : ""}`}>
      {options.map((o) => (
        <button
          key={String(o.value)}
          type="button"
          role="radio"
          aria-checked={o.value === value}
          disabled={disabled}
          onClick={() => onChange(o.value)}
          className={`rounded-[7px] px-3 py-1 text-[13px] transition-colors disabled:cursor-default ${
            o.value === value ? "bg-accent text-white" : "text-fog hover:text-paper"
          }`}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function Toggle({
  checked,
  onChange,
  label,
  disabled = false,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={`relative h-6 w-11 rounded-full border transition-colors disabled:opacity-45 ${checked ? "border-accent bg-accent" : "border-groove bg-night"}`}
    >
      <span
        className={`absolute top-[3px] left-0 h-4 w-4 rounded-full bg-white shadow transition-transform ${checked ? "translate-x-[22px]" : "translate-x-[3px]"}`}
      />
    </button>
  );
}

export function Field({ label, hint, htmlFor, children }: { label: string; hint?: string; htmlFor?: string; children: ReactNode }) {
  return (
    <div className="border-b border-groove py-5 last:border-b-0">
      <label htmlFor={htmlFor} className="block font-medium">
        {label}
      </label>
      {hint && <p className="mt-0.5 mb-3 text-[12px] text-fog">{hint}</p>}
      {children}
    </div>
  );
}
