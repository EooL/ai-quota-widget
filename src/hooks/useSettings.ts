import { useEffect, useState } from "react";
import type { AppSettings, PlatformId } from "../lib/types";
import { invokeCommand } from "../lib/tauri";

const KEY = "aqw:settings:v1";
const defaults: AppSettings = {
  pollIntervalSec: 60,
  warningThresholdPct: 20,
  autostart: false,
  notifyPace: true,
  notifyReset: true,
  ghost: false,
  visible: { chatgpt: true, gemini: true, claude: true },
};

export function useSettings() {
  const [settings, setSettings] = useState<AppSettings>(() => {
    try {
      return { ...defaults, ...JSON.parse(localStorage.getItem(KEY) ?? "{}") };
    } catch { return defaults; }
  });

  useEffect(() => {
    void invokeCommand<boolean>("get_autostart").then((autostart) =>
      setSettings((current) => ({ ...current, autostart })),
    ).catch(() => undefined);
  }, []);

  const update = (next: AppSettings) => {
    setSettings(next);
    localStorage.setItem(KEY, JSON.stringify(next));
  };

  const togglePlatform = (id: PlatformId) => update({ ...settings, visible: { ...settings.visible, [id]: !settings.visible[id] } });
  return { settings, update, togglePlatform };
}
