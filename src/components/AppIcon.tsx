import { useEffect, useState } from "react";
import { Bell } from "lucide-react";
import { getAppIcon } from "../lib/api";
import type { App } from "../lib/types";

const cache = new Map<string, Promise<string | null>>();

function loadIcon(path: string): Promise<string | null> {
  let p = cache.get(path);
  if (!p) {
    p = getAppIcon(path).catch(() => null);
    cache.set(path, p);
  }
  return p;
}

/** The program's real icon (extracted from its .exe); a letter tile if it has none. Dimmed when muted. */
export function AppIcon({ app, size = 32 }: { app: App; size?: number }) {
  const [src, setSrc] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    if (app.exePath && !app.isSystem) {
      loadIcon(app.exePath).then((b64) => alive && setSrc(b64 ? `data:image/png;base64,${b64}` : null));
    }
    return () => {
      alive = false;
    };
  }, [app.exePath, app.isSystem]);

  const box = { width: size, height: size, opacity: app.muted ? 0.45 : 1 };
  if (app.isSystem) {
    return (
      <div style={box} className="grid shrink-0 place-items-center rounded-[9px] bg-raised text-fog">
        <Bell size={size * 0.5} />
      </div>
    );
  }
  if (src) return <img src={src} style={box} alt="" draggable={false} className="shrink-0 object-contain" />;
  return (
    <div style={{ ...box, fontSize: size * 0.45 }} className="grid shrink-0 place-items-center rounded-[9px] bg-raised font-semibold text-paper">
      {app.name.charAt(0).toUpperCase()}
    </div>
  );
}
