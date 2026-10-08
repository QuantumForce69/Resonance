import { pct, sentApps, signedDb } from "../lib/processing";
import { setAppSend, setProcessing } from "../lib/store";
import type { CallSettings, UiState } from "../lib/types";
import { AppIcon } from "../components/AppIcon";
import { Knob } from "../components/Knob";
import { Slider } from "../components/Slider";
import { Banner, Toggle } from "../components/ui";

/** Processing's Call mix module: level of sent apps, ducking, and the apps currently sent. */
export function CallMix({ state }: { state: UiState }) {
  const p = state.processing.settings;
  const call = p.call;
  const save = (patch: Partial<CallSettings>) => setProcessing({ ...p, call: { ...call, ...patch } });

  if (!state.processing.callAvailable) {
    return (
      <section aria-label="Call mix" className="flex flex-col gap-3 rounded-xl border border-groove bg-night/40 p-4">
        <h3 className="m-0 text-[14px] font-medium">Call mix</h3>
        <Banner>Sending apps into calls needs Windows 10 version 2004 or newer.</Banner>
      </section>
    );
  }

  const sent = sentApps(state);
  return (
    <section aria-label="Call mix" className="flex flex-col gap-4 rounded-xl border border-groove bg-night/40 p-4">
      <div className="flex flex-wrap items-baseline justify-between gap-2">
        <h3 className="m-0 text-[14px] font-medium">Call mix</h3>
        <span className="text-[12px] text-fog">Apps you send with the Call key on the Mixer are heard in calls</span>
      </div>
      <div className="flex flex-wrap items-center gap-6">
        <div className="max-w-[420px] min-w-[220px] flex-1">
          <div className="mb-1.5 flex items-baseline justify-between">
            <span className="text-[13px] text-fog">Apps in call</span>
            <span className="num text-[14px]">{pct(call.master)}</span>
          </div>
          <Slider value={call.master} label="Apps in call" valueText={pct} onChange={(master) => save({ master })} />
        </div>
        <div className="flex items-center gap-3">
          <Toggle checked={call.duck} onChange={(duck) => save({ duck })} label="Duck apps while I talk" />
          <span className="text-[13px] text-fog">Duck while I talk</span>
        </div>
        <Knob
          size="small"
          label="Duck by"
          value={call.duckDb}
          min={-40}
          max={0}
          step={1}
          format={signedDb}
          disabled={!call.duck}
          onChange={(duckDb) => save({ duckDb })}
        />
      </div>
      <div className="flex flex-wrap gap-2">
        {sent.length === 0 ? (
          <p className="m-0 text-[12px] text-dim">No apps sent yet.</p>
        ) : (
          sent.map((a) => (
            <button
              key={a.key}
              type="button"
              aria-label={`Stop sending ${a.name}`}
              title={`Stop sending ${a.name}`}
              onClick={() => void setAppSend(a, null)}
              className="flex items-center gap-2 rounded-full border border-accent/50 px-2.5 py-1 text-[12px] text-lilac hover:border-coral hover:text-coral"
            >
              <AppIcon app={a} size={16} />
              {a.name}
              <span className="num text-fog">{pct(a.send ?? 0)}</span>
            </button>
          ))
        )}
      </div>
    </section>
  );
}
