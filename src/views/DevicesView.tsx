import { useState } from "react";
import { Eye, EyeOff, Mic, Pencil, Phone, Speaker, Star } from "lucide-react";
import { formatDb } from "../lib/levels";
import { run, toggleDeviceMute } from "../lib/store";
import type { Device, UiState } from "../lib/types";
import { MuteButton } from "../components/MuteButton";
import { Slider } from "../components/Slider";
import { Badge, IconButton, Panel, ViewHeader } from "../components/ui";

function DeviceRow({ device: d }: { device: Device }) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(d.alias ?? d.name);
  const active = d.state === "active";

  return (
    <div className={`flex items-center gap-4 border-b border-groove px-5 py-3.5 last:border-b-0 ${active ? "" : "opacity-55"}`}>
      <div className="grid h-9 w-9 shrink-0 place-items-center rounded-[9px] bg-raised text-fog">
        {d.flow === "capture" ? <Mic size={17} /> : <Speaker size={17} />}
      </div>
      <div className="min-w-0 flex-1">
        {editing ? (
          <form
            onSubmit={(e) => {
              e.preventDefault();
              run({ type: "setDeviceAlias", id: d.id, alias: draft });
              setEditing(false);
            }}
          >
            <input
              autoFocus
              aria-label="Device name"
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={(e) => e.key === "Escape" && setEditing(false)}
              onBlur={() => setEditing(false)}
              placeholder={d.originalName}
              className="w-full rounded-md border border-groove bg-night px-2 py-1 text-[14px] select-text"
            />
          </form>
        ) : (
          <div className="flex items-baseline gap-2">
            <span className="truncate font-medium">{d.name}</span>
            {d.alias && <span className="truncate text-[12px] text-dim">{d.originalName}</span>}
          </div>
        )}
        <div className="mt-1 flex flex-wrap gap-1.5">
          {d.defaultConsole && <Badge tone="accent">Default</Badge>}
          {d.defaultComms && <Badge>Calls</Badge>}
          {!active && <Badge tone="muted">Unplugged</Badge>}
        </div>
      </div>
      {active && (
        <div className="flex w-64 items-center gap-2.5">
          <MuteButton kind={d.flow === "capture" ? "mic" : "speaker"} muted={d.muted} label={d.name} onToggle={() => toggleDeviceMute(d)} />
          <Slider value={d.volume} label={`${d.name} volume`} onChange={(v) => run({ type: "setDeviceVolume", id: d.id, volume: v })} />
          <span className="num w-16 text-right text-[12px] text-fog">{formatDb(d.volume)}</span>
        </div>
      )}
      <div className="flex w-[136px] shrink-0 items-center justify-end gap-0.5">
        {active && !d.defaultConsole && (
          <IconButton label={`Set ${d.name} as default`} onClick={() => run({ type: "setDefaultDevice", id: d.id, role: "console" })}>
            <Star size={16} />
          </IconButton>
        )}
        {active && !d.defaultComms && (
          <IconButton label={`Use ${d.name} for calls`} onClick={() => run({ type: "setDefaultDevice", id: d.id, role: "communications" })}>
            <Phone size={16} />
          </IconButton>
        )}
        <IconButton
          label={`Rename ${d.name}`}
          onClick={() => {
            setDraft(d.alias ?? d.name);
            setEditing(true);
          }}
        >
          <Pencil size={16} />
        </IconButton>
        <IconButton label={d.hidden ? `Show ${d.name}` : `Hide ${d.name}`} onClick={() => run({ type: "setDeviceHidden", id: d.id, hidden: !d.hidden })}>
          {d.hidden ? <Eye size={16} /> : <EyeOff size={16} />}
        </IconButton>
      </div>
    </div>
  );
}

function DeviceSection({ title, devices }: { title: string; devices: Device[] }) {
  const [showHidden, setShowHidden] = useState(false);
  const visible = devices.filter((d) => !d.hidden);
  const hidden = devices.filter((d) => d.hidden);
  return (
    <section className="flex flex-col gap-2.5">
      <h2 className="m-0 text-[15px] font-medium text-fog">{title}</h2>
      <Panel>
        {visible.map((d) => (
          <DeviceRow key={d.id} device={d} />
        ))}
        {visible.length === 0 && <p className="m-0 px-5 py-4 text-[13px] text-dim">None connected.</p>}
        {showHidden && hidden.map((d) => <DeviceRow key={d.id} device={d} />)}
      </Panel>
      {hidden.length > 0 && (
        <button type="button" onClick={() => setShowHidden(!showHidden)} className="self-start text-[12px] text-dim hover:text-paper">
          {showHidden ? "Collapse hidden devices" : `Show ${hidden.length} hidden`}
        </button>
      )}
    </section>
  );
}

export function DevicesView({ state }: { state: UiState }) {
  return (
    <div className="flex max-w-5xl flex-col gap-7">
      <ViewHeader title="Devices" subtitle="Choose defaults, rename and set levels for every output and input" />
      <DeviceSection title="Outputs" devices={state.devices.filter((d) => d.flow === "render")} />
      <DeviceSection title="Inputs" devices={state.devices.filter((d) => d.flow === "capture")} />
    </div>
  );
}
