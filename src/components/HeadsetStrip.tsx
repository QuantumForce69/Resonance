import { AMBIENT_OPTIONS, connectionLabel, isConnected } from "../lib/headset";
import { runHeadset, useStore } from "../lib/store";
import { BatteryGauge } from "./BatteryGauge";
import { LeverSwitch } from "./LeverSwitch";

/** Tray-flyout strip: headphone name, battery and the ambient-mode lever. */
export function HeadsetStrip() {
  const headset = useStore((s) => s.headset);
  if (!headset) return null;
  const connected = isConnected(headset);
  return (
    <section aria-label="Headphones" className="border-b border-groove px-[18px] py-3">
      <div className="mb-2 flex items-center justify-between gap-3">
        <span className="truncate text-[13px] font-medium">{headset.name ?? "Space One"}</span>
        {connected && headset.battery ? (
          <BatteryGauge battery={headset.battery} size="sm" />
        ) : (
          <span className="text-[12px] text-dim">{connectionLabel(headset.connection)}</span>
        )}
      </div>
      {connected && headset.ambientMode && (
        <LeverSwitch
          className="w-full"
          label="Noise control"
          value={headset.ambientMode}
          options={AMBIENT_OPTIONS}
          onChange={(mode) => runHeadset({ type: "setAmbientMode", mode })}
        />
      )}
    </section>
  );
}
