import { AMBIENT_OPTIONS, LEVEL_OPTIONS, connectionLabel, isConnected } from "../lib/headset";
import { runHeadset, useStore } from "../lib/store";
import type { HeadsetState, NcMode } from "../lib/types";
import { BatteryGauge } from "../components/BatteryGauge";
import { EqFaders } from "../components/EqFaders";
import { LeverSwitch } from "../components/LeverSwitch";
import { Panel, Segmented, Toggle, ViewHeader } from "../components/ui";

const NC_MODES: { value: NcMode; label: string }[] = [
  { value: "adaptive", label: "Adaptive" },
  { value: "manual", label: "Manual" },
];

function NoiseCancelling({ headset, disabled }: { headset: HeadsetState; disabled: boolean }) {
  return (
    <div className="flex flex-wrap items-end gap-8">
      <div>
        <div className="mb-2 text-[13px] text-fog">Strength</div>
        <Segmented
          label="Noise cancelling strength"
          value={headset.ncMode ?? "manual"}
          options={NC_MODES}
          disabled={disabled || !headset.ncMode}
          onChange={(mode) => runHeadset({ type: "setNcMode", mode })}
        />
      </div>
      {headset.ncMode === "manual" && headset.manualNc !== null && (
        <div>
          <div className="mb-2 text-[13px] text-fog">Level</div>
          <Segmented
            label="Noise cancelling level"
            value={headset.manualNc}
            options={LEVEL_OPTIONS}
            disabled={disabled}
            onChange={(level) => runHeadset({ type: "setManualNc", level })}
          />
        </div>
      )}
      {headset.ncMode === "adaptive" && (
        <p className="m-0 pb-1.5 text-[13px] text-fog">
          {headset.adaptiveNc ? `Adjusting automatically, currently ${headset.adaptiveNc} of 5` : "Adjusting automatically"}
        </p>
      )}
    </div>
  );
}

export function HeadphonesView() {
  const headset = useStore((s) => s.headset);
  if (!headset) {
    return <div className="grid h-full place-items-center text-[13px] text-dim">Connecting to the headphones…</div>;
  }
  const connected = isConnected(headset);
  const name = headset.name ?? "Soundcore Space One";
  const eq = headset.eq;

  return (
    <div className="flex flex-col gap-5">
      <ViewHeader title="Headphones" subtitle={connected ? `${name} is connected` : name} />

      {!connected && (
        <div className="flex items-center justify-between gap-4 rounded-[10px] border border-groove bg-raised px-4 py-3">
          <span className="text-[13px] text-fog">{connectionLabel(headset.connection)}</span>
          <button type="button" className="key" onClick={() => runHeadset({ type: "retry" })}>
            Retry
          </button>
        </div>
      )}

      <div className="flex flex-wrap items-stretch gap-5">
        <section aria-label="Noise control" className="console flex min-w-0 flex-[3_1_520px] flex-col gap-6 px-7 py-6">
          <h2 className="m-0 text-[15px] font-medium text-fog">Noise control</h2>
          <LeverSwitch
            className="w-[420px]"
            label="Noise control"
            value={headset.ambientMode ?? "normal"}
            options={AMBIENT_OPTIONS}
            disabled={!connected || !headset.ambientMode}
            onChange={(mode) => runHeadset({ type: "setAmbientMode", mode })}
          />
          {headset.ambientMode === "noiseCanceling" && <NoiseCancelling headset={headset} disabled={!connected} />}
          {headset.ambientMode === "transparency" && headset.transparency !== null && (
            <div>
              <div className="mb-2 text-[13px] text-fog">Transparency strength</div>
              <Segmented
                label="Transparency strength"
                value={headset.transparency}
                options={LEVEL_OPTIONS}
                disabled={!connected}
                onChange={(level) => runHeadset({ type: "setTransparency", level })}
              />
            </div>
          )}
          {headset.ambientMode && headset.ambientMode !== "normal" && headset.windNoise !== null && (
            <div className="flex items-center justify-between gap-4 border-t border-groove pt-5">
              <div>
                <div className="font-medium">Reduce wind noise</div>
                <div className="mt-0.5 text-[12px] text-fog">Cuts rumble when you're outside</div>
              </div>
              <Toggle
                label="Reduce wind noise"
                checked={headset.windNoise}
                disabled={!connected}
                onChange={(enabled) => runHeadset({ type: "setWindNoise", enabled })}
              />
            </div>
          )}
        </section>

        <Panel className="flex min-w-0 flex-[2_1_300px] flex-col gap-5 px-6 py-6">
          <h2 className="m-0 text-[15px] font-medium text-fog">Battery</h2>
          {headset.battery ? (
            <div className="flex items-center gap-4">
              <BatteryGauge battery={headset.battery} />
              <span className="num text-[14px] text-fog">
                {headset.battery.level} of {headset.battery.max}
              </span>
            </div>
          ) : (
            <p className="m-0 text-[13px] text-dim">Unavailable</p>
          )}
          <dl className="m-0 mt-auto grid grid-cols-[auto_1fr] gap-x-6 gap-y-2 border-t border-groove pt-5 text-[13px]">
            <dt className="text-fog">Firmware</dt>
            <dd className="num m-0">{headset.firmware ?? "—"}</dd>
            <dt className="text-fog">Serial number</dt>
            <dd className="num m-0 truncate">{headset.serial ?? "—"}</dd>
          </dl>
        </Panel>
      </div>

      {eq && (
        <section aria-label="Equalizer" className="console flex flex-col gap-5 px-7 py-6">
          <div className="flex flex-wrap items-center justify-between gap-4">
            <h2 className="m-0 text-[15px] font-medium text-fog">Equalizer</h2>
            <select
              aria-label="EQ preset"
              className="field-select"
              value={eq.preset ?? ""}
              disabled={!connected}
              onChange={(e) => e.currentTarget.value && runHeadset({ type: "setEqPreset", preset: e.currentTarget.value })}
            >
              {eq.preset === null && <option value="">Custom</option>}
              {eq.presets.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </select>
          </div>
          <EqFaders eq={eq} disabled={!connected} onChange={(values) => runHeadset({ type: "setEqBands", values })} />
        </section>
      )}
    </div>
  );
}
