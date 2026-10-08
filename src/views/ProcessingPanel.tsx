import { Power } from "lucide-react";
import { openVbCable } from "../lib/api";
import { amountCurve, autoMakeup, compressorCurve, processingSources } from "../lib/processing";
import { setProcessing } from "../lib/store";
import type { CompressorParams, DspParams, EngineStatus, GateParams, LimiterParams, ProcessingSettings, UiState } from "../lib/types";
import { DbScale } from "../components/DbScale";
import { Knob } from "../components/Knob";
import { Meter } from "../components/Meter";
import { Slider } from "../components/Slider";
import { StageModule } from "../components/StageModule";
import { Banner, Toggle } from "../components/ui";

const round1 = (v: number) => Math.round(v * 10) / 10;
const pct = (v: number) => `${Math.round(v * 100)}%`;
const db = (v: number) => `${v > 0 ? "+" : ""}${round1(v)} dB`;
const ms = (v: number) => `${round1(v)} ms`;
const ratio = (v: number) => `${round1(v)}:1`;

function statusText(status: EngineStatus): string {
  switch (status.state) {
    case "running":
      return "On. Apps hear your processed mic through CABLE Output";
    case "paused":
      return `Paused: ${status.reason}`;
    case "noCable":
      return "VB-Cable isn't available";
    case "off":
      return "Off. Apps hear your mic directly";
  }
}

/** The processed-mic console: source, master key, four stages, before/after meters and monitoring. */
export function ProcessingPanel({ state }: { state: UiState }) {
  const { settings: p, status, cableInstalled } = state.processing;
  const save = (patch: Partial<ProcessingSettings>) => setProcessing({ ...p, ...patch });
  const dsp = (patch: Partial<DspParams>) => save({ dsp: { ...p.dsp, ...patch } });
  const gate = (patch: Partial<GateParams>) => dsp({ gate: { ...p.dsp.gate, ...patch } });
  const comp = (patch: Partial<CompressorParams>) => dsp({ compressor: { ...p.dsp.compressor, ...patch } });
  const limiter = (patch: Partial<LimiterParams>) => dsp({ limiter: { ...p.dsp.limiter, ...patch } });

  if (!cableInstalled) {
    return (
      <section aria-label="Processing" className="console flex flex-col items-start gap-4 px-7 py-6">
        <h2 className="m-0 text-[18px] font-medium">Processing</h2>
        <Banner>
          Processing sends your cleaned-up mic to apps through VB-Cable, a free virtual audio cable. Install it, restart your PC, and
          Resonance will find it automatically.
        </Banner>
        <button type="button" className="key key-lit h-9 px-4 text-[13px]" onClick={() => void openVbCable()}>
          Get VB-Cable
        </button>
      </section>
    );
  }

  const c = p.dsp.compressor;
  const curve = compressorCurve(c);
  const g = p.dsp.gate;
  const l = p.dsp.limiter;

  return (
    <section aria-label="Processing" className="console flex flex-col gap-6 px-7 py-6">
      <div className="flex flex-wrap items-center gap-4">
        <button
          type="button"
          aria-pressed={p.enabled}
          aria-label={p.enabled ? "Turn processing off" : "Turn processing on"}
          onClick={() => save({ enabled: !p.enabled })}
          className={`grid h-12 w-12 shrink-0 place-items-center rounded-xl border ${p.enabled ? "mic-live text-lilac" : "border-groove bg-raised text-fog"}`}
        >
          <Power size={20} strokeWidth={1.8} />
        </button>
        <div className="min-w-0 flex-1">
          <h2 className="m-0 text-[18px] font-medium">Processing</h2>
          <p role="status" className={`m-0 text-[13px] ${status.state === "paused" ? "text-coral" : "text-fog"}`}>
            {statusText(status)}
          </p>
        </div>
        <label htmlFor="proc-source" className="text-[13px] text-fog">
          Source
        </label>
        <select
          id="proc-source"
          className="field-select max-w-[280px]"
          value={p.sourceId ?? ""}
          onChange={(e) => save({ sourceId: e.currentTarget.value || null })}
        >
          <option value="">Automatic (your usual mic)</option>
          {processingSources(state).map((d) => (
            <option key={d.id} value={d.id}>
              {d.name}
            </option>
          ))}
        </select>
      </div>

      <div className="grid grid-cols-[repeat(auto-fit,minmax(210px,1fr))] items-start gap-4">
        <StageModule
          title="Noise suppression"
          enabled={p.dsp.denoise.enabled}
          onToggle={(enabled) => dsp({ denoise: { ...p.dsp.denoise, enabled } })}
        >
          <Knob
            label="Strength"
            value={p.dsp.denoise.strength}
            min={0}
            max={1}
            step={0.05}
            format={pct}
            onChange={(strength) => dsp({ denoise: { ...p.dsp.denoise, strength } })}
          />
        </StageModule>

        <StageModule
          title="Noise gate"
          enabled={g.enabled}
          onToggle={(enabled) => gate({ enabled })}
          advanced={
            <>
              <Knob size="small" label="Attack" value={g.attackMs} min={0.5} max={50} step={0.5} format={ms} onChange={(attackMs) => gate({ attackMs })} />
              <Knob size="small" label="Hold" value={g.holdMs} min={0} max={1000} step={10} format={ms} onChange={(holdMs) => gate({ holdMs })} />
              <Knob size="small" label="Release" value={g.releaseMs} min={10} max={1000} step={10} format={ms} onChange={(releaseMs) => gate({ releaseMs })} />
              <Knob size="small" label="Range" value={g.rangeDb} min={-80} max={0} step={1} format={db} onChange={(rangeDb) => gate({ rangeDb })} />
            </>
          }
        >
          <Knob label="Threshold" value={g.thresholdDb} min={-80} max={-10} step={1} format={db} onChange={(thresholdDb) => gate({ thresholdDb })} />
        </StageModule>

        <StageModule
          title="Compressor"
          enabled={c.enabled}
          onToggle={(enabled) => comp({ enabled })}
          advanced={
            <>
              <Knob
                size="small"
                label="Threshold"
                value={curve.thresholdDb}
                min={-60}
                max={0}
                step={0.5}
                format={db}
                onChange={(thresholdDb) => comp({ custom: true, thresholdDb, ratio: curve.ratio })}
              />
              <Knob
                size="small"
                label="Ratio"
                value={curve.ratio}
                min={1}
                max={20}
                step={0.5}
                format={ratio}
                onChange={(r) => comp({ custom: true, thresholdDb: curve.thresholdDb, ratio: r })}
              />
              <Knob size="small" label="Attack" value={c.attackMs} min={0.1} max={100} step={0.1} format={ms} onChange={(attackMs) => comp({ attackMs })} />
              <Knob size="small" label="Release" value={c.releaseMs} min={10} max={1000} step={10} format={ms} onChange={(releaseMs) => comp({ releaseMs })} />
              <div className="flex flex-col items-center gap-1.5">
                <Knob size="small" label="Makeup" value={c.makeupDb ?? round1(autoMakeup(c))} min={-12} max={24} step={0.5} format={db} onChange={(makeupDb) => comp({ makeupDb })} />
                <button
                  type="button"
                  aria-pressed={c.makeupDb === null}
                  onClick={() => comp({ makeupDb: c.makeupDb === null ? Math.round(autoMakeup(c) * 2) / 2 : null })}
                  className={`key h-6 px-2 text-[11px] ${c.makeupDb === null ? "key-lit" : ""}`}
                >
                  Auto
                </button>
              </div>
            </>
          }
        >
          <Knob
            label="Amount"
            value={c.amount}
            min={0}
            max={1}
            step={0.05}
            format={(v) => (c.custom ? "Custom" : pct(v))}
            onChange={(amount) => comp({ amount, custom: false, ...amountCurve(amount) })}
          />
        </StageModule>

        <StageModule
          title="Limiter"
          enabled={l.enabled}
          onToggle={(enabled) => limiter({ enabled })}
          advanced={<Knob size="small" label="Release" value={l.releaseMs} min={5} max={500} step={5} format={ms} onChange={(releaseMs) => limiter({ releaseMs })} />}
        >
          <Knob label="Ceiling" value={l.ceilingDb} min={-12} max={0} step={0.5} format={db} onChange={(ceilingDb) => limiter({ ceilingDb })} />
        </StageModule>
      </div>

      <div className="grid gap-6 border-t border-groove pt-6 md:grid-cols-2">
        <div className="flex flex-col gap-2">
          <div className="flex items-center gap-3">
            <span className="w-12 text-[12px] text-fog">Before</span>
            <Meter source="procIn" className="h-2.5!" />
          </div>
          <div className="flex items-center gap-3">
            <span className="w-12 text-[12px] text-fog">After</span>
            <Meter source="procOut" className="h-2.5!" />
          </div>
          <div className="pl-15">
            <DbScale kind="meter" orientation="horizontal" />
          </div>
        </div>
        <div className="flex items-center gap-4">
          <Toggle checked={p.monitor} onChange={(monitor) => save({ monitor })} label="Hear yourself" />
          <div className="min-w-0 flex-1">
            <div className="mb-1.5 flex items-baseline justify-between">
              <span className="text-[13px] text-fog">Hear yourself</span>
              <span className="num text-[14px]">{pct(p.monitorLevel)}</span>
            </div>
            <Slider value={p.monitorLevel} label="Monitoring level" disabled={!p.monitor} onChange={(monitorLevel) => save({ monitorLevel })} />
          </div>
        </div>
      </div>
    </section>
  );
}
