import { outputs, run, useStore } from "../lib/store";
import type { App } from "../lib/types";

/** Per-app output patch. Hidden when Windows doesn't support per-app routing. */
export function DeviceSelect({ app }: { app: App }) {
  const state = useStore((s) => s.state);
  if (!state?.routingAvailable) return null;
  const list = outputs(state);
  const def = list.find((d) => d.defaultConsole);
  const routedElsewhere = app.output && !list.some((d) => d.id === app.output);
  const missing = routedElsewhere ? state.devices.find((d) => d.id === app.output) : undefined;

  return (
    <select
      aria-label={`Output device for ${app.name}`}
      title="Output device"
      value={app.output ?? ""}
      onChange={(e) => run({ type: "setAppOutput", key: app.key, deviceId: e.currentTarget.value || null })}
      className="patch"
    >
      <option value="">{def ? `Default (${def.name})` : "Default output"}</option>
      {list.map((d) => (
        <option key={d.id} value={d.id}>
          {d.name}
        </option>
      ))}
      {routedElsewhere && <option value={app.output!}>{`${missing?.name ?? "Device"} (unavailable)`}</option>}
    </select>
  );
}
