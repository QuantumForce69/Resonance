import { useState } from "react";
import { openVbCable } from "../lib/api";
import { defaultMic, finishOnboarding, inputs, run, setProcessing } from "../lib/store";
import type { UiState } from "../lib/types";
import { StartWithWindows } from "../components/StartWithWindows";
import { Banner, Field, Logo, Toggle } from "../components/ui";

const PRIMARY = "key mic-live text-lilac h-9 px-4 text-[13px]";
const STEPS = 3;

/** First-run setup: microphone, VB-Cable, then optional processing and Start with Windows. */
export function Onboarding({ state }: { state: UiState }) {
  const [step, setStep] = useState(0);
  const mic = defaultMic(state);
  const cable = state.processing.cableInstalled;
  const done = () => void finishOnboarding(state.ui);

  const chooseMic = (id: string) => {
    void run({ type: "setDefaultDevice", id, role: "console" });
    void run({ type: "setDefaultDevice", id, role: "communications" });
  };

  return (
    <div role="dialog" aria-modal="true" aria-labelledby="welcome-title" className="fixed inset-0 z-50 grid place-items-center bg-black/60 p-6">
      <div className="console flex w-full max-w-lg flex-col gap-5 px-7 py-6">
        <div className="flex items-center gap-3">
          <Logo size={24} />
          <h2 id="welcome-title" className="m-0 text-[20px] font-medium">
            Welcome to Resonance
          </h2>
        </div>
        <p className="m-0 text-[12px] text-dim">
          Step {step + 1} of {STEPS}
        </p>

        {step === 0 && (
          <Field label="Your microphone" hint="The mic apps and calls hear" htmlFor="welcome-mic">
            <select id="welcome-mic" className="field-select" value={mic?.id ?? ""} onChange={(e) => e.currentTarget.value && chooseMic(e.currentTarget.value)}>
              {!mic && <option value="">Choose a microphone</option>}
              {inputs(state).map((d) => (
                <option key={d.id} value={d.id}>
                  {d.name}
                </option>
              ))}
            </select>
          </Field>
        )}

        {step === 1 &&
          (cable ? (
            <Banner>VB-Cable is installed, so Resonance can clean up your mic for every app and mix apps into calls.</Banner>
          ) : (
            <div className="flex flex-col items-start gap-3">
              <Banner>
                To clean up your mic for every app, Resonance uses VB-Cable, a free virtual audio cable. Install it now or any time
                later, then restart your PC.
              </Banner>
              <button type="button" className="key h-9 px-4 text-[13px]" onClick={() => void openVbCable()}>
                Get VB-Cable
              </button>
            </div>
          ))}

        {step === 2 && (
          <div className="flex flex-col gap-4">
            {cable && (
              <div className="flex items-center justify-between gap-4">
                <span className="text-[13px]">Turn on Processing (noise suppression for your mic)</span>
                <Toggle
                  label="Turn on Processing"
                  checked={state.processing.settings.enabled}
                  onChange={(enabled) => void setProcessing({ ...state.processing.settings, enabled })}
                />
              </div>
            )}
            <div className="flex items-center justify-between gap-4">
              <span className="text-[13px]">Start Resonance with Windows</span>
              <StartWithWindows />
            </div>
          </div>
        )}

        <div className="flex items-center justify-between">
          <button type="button" className="border-0 bg-transparent text-[13px] text-fog hover:text-paper" onClick={done}>
            Skip
          </button>
          {step < STEPS - 1 ? (
            <button type="button" className={PRIMARY} onClick={() => setStep(step + 1)}>
              Next
            </button>
          ) : (
            <button type="button" className={PRIMARY} onClick={done}>
              Done
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
