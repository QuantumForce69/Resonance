import type { ReactNode } from "react";
import { Minus, Square, X } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { defaultMic, useStore } from "../lib/store";
import { MicKey } from "../components/MicKey";
import { Logo } from "../components/ui";

export type View = "mixer" | "devices" | "mic" | "headphones" | "profiles" | "settings";

const TABS: { id: View; label: string }[] = [
  { id: "mixer", label: "Mixer" },
  { id: "devices", label: "Devices" },
  { id: "mic", label: "Microphone" },
  { id: "headphones", label: "Headphones" },
  { id: "profiles", label: "Profiles" },
  { id: "settings", label: "Settings" },
];

function WindowButton({ label, onClick, close, children }: { label: string; onClick: () => void; close?: boolean; children: ReactNode }) {
  return (
    <button
      type="button"
      aria-label={label}
      onClick={onClick}
      className={`grid h-full w-[46px] place-items-center border-0 bg-transparent text-fog transition-colors ${
        close ? "hover:bg-[#e81123] hover:text-white" : "hover:bg-raised hover:text-paper"
      }`}
    >
      {children}
    </button>
  );
}

export function Titlebar({ view, onSelect }: { view: View; onSelect: (v: View) => void }) {
  const state = useStore((s) => s.state);
  const win = getCurrentWindow();
  return (
    <header data-tauri-drag-region className="flex h-[52px] shrink-0 items-center gap-8 border-b border-groove pl-5">
      <div data-tauri-drag-region className="flex items-center gap-2.5 text-[15px] font-semibold tracking-[-0.01em]">
        <Logo /> Resonance
      </div>
      <nav aria-label="Sections" className="flex gap-6">
        {TABS.map((t) => (
          <button key={t.id} type="button" className="tab" aria-current={view === t.id ? "page" : undefined} onClick={() => onSelect(t.id)}>
            {t.label}
          </button>
        ))}
      </nav>
      <div data-tauri-drag-region className="h-full flex-1" />
      {state && <MicKey mic={defaultMic(state)} hotkey={state.mic.hotkey} />}
      <div className="flex h-full">
        <WindowButton label="Minimize" onClick={() => win.minimize()}>
          <Minus size={14} />
        </WindowButton>
        <WindowButton label="Maximize" onClick={() => win.toggleMaximize()}>
          <Square size={11} />
        </WindowButton>
        <WindowButton label="Close" close onClick={() => win.close()}>
          <X size={15} />
        </WindowButton>
      </div>
    </header>
  );
}
