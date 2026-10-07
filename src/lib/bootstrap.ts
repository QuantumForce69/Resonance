import { useEffect } from "react";
import { getState, onMeters, onNotice, onState, takeNotices } from "./api";
import { ingestFrame } from "./meters";
import { errorMessage, useStore } from "./store";

/** Loads the initial state and subscribes to backend events for this window. */
export function useBootstrap() {
  useEffect(() => {
    let disposed = false;
    const { setState, pushToast } = useStore.getState();
    const unlisteners = [
      onState((s) => setState(s)),
      onMeters(ingestFrame),
      onNotice((e) => pushToast("error", e.message)),
    ];
    getState()
      .then((s) => !disposed && setState(s))
      .catch((e) => pushToast("error", errorMessage(e)));
    takeNotices()
      .then((ns) => ns.forEach((n) => pushToast("error", n.message)))
      .catch(() => {});
    return () => {
      disposed = true;
      unlisteners.forEach((p) => p.then((u) => u()).catch(() => {}));
    };
  }, []);
}
