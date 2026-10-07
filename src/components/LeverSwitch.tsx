interface Props<T extends string> {
  value: T;
  options: { value: T; label: string }[];
  onChange: (value: T) => void;
  label: string;
  disabled?: boolean;
  className?: string;
}

/** Multi-position lever switch (machined lever in a slot) with labelled positions. */
export function LeverSwitch<T extends string>({ value, options, onChange, label, disabled = false, className = "w-[300px]" }: Props<T>) {
  const n = options.length;
  const index = Math.max(0, options.findIndex((o) => o.value === value));
  return (
    <div className={`max-w-full ${className} ${disabled ? "opacity-45" : ""}`}>
      <div aria-hidden className="relative h-[30px] rounded-lg bg-[var(--slot)] shadow-[inset_0_0_0_1px_var(--groove),inset_0_2px_5px_rgb(0_0_0/0.6)]">
        <div
          className="absolute top-[3px] bottom-[3px] rounded-md border border-[var(--cap-edge)] bg-linear-to-b from-[var(--cap-hi)] to-[var(--cap-lo)] shadow-[0_3px_8px_var(--shadow)] transition-[left] duration-150"
          style={{ left: `calc(${(index * 100) / n}% + 3px)`, width: `calc(${100 / n}% - 6px)` }}
        >
          <div className="absolute top-[7px] bottom-[7px] left-1/2 -ml-px w-0.5 rounded-sm bg-accent shadow-[0_0_6px_var(--accent)]" />
        </div>
      </div>
      <div role="radiogroup" aria-label={label} className="mt-1.5 flex text-center">
        {options.map((o, i) => (
          <button
            key={o.value}
            type="button"
            role="radio"
            aria-checked={i === index}
            disabled={disabled}
            onClick={() => onChange(o.value)}
            className={`flex-1 border-0 bg-transparent py-1.5 text-[12px] disabled:cursor-default ${
              i === index ? "font-semibold text-paper" : "text-dim hover:text-fog"
            }`}
          >
            {o.label}
          </button>
        ))}
      </div>
    </div>
  );
}
