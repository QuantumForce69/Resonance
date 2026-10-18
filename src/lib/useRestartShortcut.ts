import { useEffect } from "react";
import { restartAudio } from "./api";

/** Ctrl+R restarts audio, as in Voicemeeter. Ignored while typing, and it replaces the webview's page reload. */
export function useRestartShortcut() {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!e.ctrlKey || e.shiftKey || e.altKey || e.key.toLowerCase() !== "r") return;
      const t = e.target as HTMLElement | null;
      if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable)) return;
      e.preventDefault();
      void restartAudio();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}
