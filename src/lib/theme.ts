import { useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { UiState } from "./types";

/** Applies light/dark (system or forced). The brand violet accent is fixed by design. */
export function useTheme(state: UiState | null) {
  const pref = state?.ui.theme ?? "system";

  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      const dark = pref === "dark" || (pref === "system" && mq.matches);
      document.documentElement.dataset.theme = dark ? "dark" : "light";
    };
    apply();
    try {
      getCurrentWindow()
        .setTheme(pref === "system" ? null : pref)
        .catch(() => {});
    } catch {
      /* not running inside Tauri (tests) */
    }
    mq.addEventListener("change", apply);
    return () => mq.removeEventListener("change", apply);
  }, [pref]);
}
