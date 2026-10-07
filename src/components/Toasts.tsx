import { useEffect } from "react";
import { AlertTriangle, Info, X } from "lucide-react";
import { useStore, type Toast } from "../lib/store";

function ToastItem({ toast }: { toast: Toast }) {
  const dismiss = useStore((s) => s.dismissToast);
  useEffect(() => {
    const t = setTimeout(() => dismiss(toast.id), 6000);
    return () => clearTimeout(t);
  }, [toast.id, dismiss]);
  const Icon = toast.kind === "error" ? AlertTriangle : Info;
  return (
    <div
      role={toast.kind === "error" ? "alert" : "status"}
      className="pointer-events-auto flex items-start gap-3 rounded-[10px] border border-groove bg-raised px-4 py-3 text-[13px] shadow-[0_20px_40px_var(--shadow)]"
    >
      <Icon size={16} className={toast.kind === "error" ? "mt-0.5 text-coral" : "mt-0.5 text-accent"} />
      <span className="flex-1">{toast.message}</span>
      <button type="button" aria-label="Dismiss" onClick={() => dismiss(toast.id)} className="text-dim hover:text-paper">
        <X size={14} />
      </button>
    </div>
  );
}

export function Toasts() {
  const toasts = useStore((s) => s.toasts);
  return (
    <div className="pointer-events-none fixed inset-x-3 bottom-3 z-50 flex flex-col items-end gap-2">
      {toasts.map((t) => (
        <div key={t.id} className="w-full max-w-sm">
          <ToastItem toast={t} />
        </div>
      ))}
    </div>
  );
}
