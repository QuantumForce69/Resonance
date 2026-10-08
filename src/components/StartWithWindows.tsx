import { useEffect, useState } from "react";
import { getAutostart, setAutostart } from "../lib/api";
import { errorMessage, useStore } from "../lib/store";
import { Toggle } from "./ui";

/** Switch for the Windows start-up entry; reads the real state, reverts if Windows refuses. */
export function StartWithWindows() {
  const [on, setOn] = useState<boolean | null>(null);

  useEffect(() => {
    let live = true;
    getAutostart()
      .then((v) => live && setOn(v))
      .catch(() => live && setOn(null));
    return () => {
      live = false;
    };
  }, []);

  const change = (v: boolean) => {
    setOn(v);
    setAutostart(v).catch((e) => {
      setOn(!v);
      useStore.getState().pushToast("error", errorMessage(e));
    });
  };

  return <Toggle label="Start with Windows" checked={on ?? false} disabled={on === null} onChange={change} />;
}
