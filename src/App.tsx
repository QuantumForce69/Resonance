import { useBootstrap } from "./lib/bootstrap";
import { useStore } from "./lib/store";
import { useTheme } from "./lib/theme";
import { Flyout } from "./views/Flyout";

const isFlyout = new URLSearchParams(window.location.search).get("view") === "flyout";

export function App() {
  useBootstrap();
  const state = useStore((s) => s.state);
  useTheme(state);
  return isFlyout ? <Flyout /> : <div className="p-6">Main window — Task 13</div>;
}
