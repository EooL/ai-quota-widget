import { useState } from "react";
import type { AppSettings, PlatformId } from "../lib/types";
import { PLATFORM_IDS, PLATFORM_META } from "../lib/types";
import { invokeCommand } from "../lib/tauri";

export default function SettingsPanel({ initial, onSave, onCancel }: { initial: AppSettings; onSave: (settings: AppSettings) => void; onCancel: () => void }) {
  const [draft, setDraft] = useState(initial);
  const set = (patch: Partial<AppSettings>) => setDraft({ ...draft, ...patch });
  const save = () => { void invokeCommand<boolean>("set_autostart", { enable: draft.autostart }).catch(() => undefined); onSave(draft); };
  return <div className="st-overlay" data-testid="settings-panel">
    <section className="st-sheet">
      <header data-tauri-drag-region><h2>Settings</h2><button onClick={onCancel} aria-label="Close">×</button></header>
      <label className="st-field"><span>Refresh interval <b>{draft.pollIntervalSec}s</b></span><input data-testid="poll-slider" type="range" min="10" max="300" step="10" value={draft.pollIntervalSec} onChange={(e) => set({ pollIntervalSec: Number(e.target.value) })} /></label>
      <label className="st-field"><span>Warning threshold <b>{draft.warningThresholdPct}%</b></span><input data-testid="warning-slider" type="range" min="5" max="50" value={draft.warningThresholdPct} onChange={(e) => set({ warningThresholdPct: Number(e.target.value) })} /></label>
      <div className="st-field"><span>Platforms shown</span><div className="st-chips">{PLATFORM_IDS.map((id: PlatformId) => <button key={id} type="button" className={draft.visible[id] ? "on" : ""} style={{ "--accent": PLATFORM_META[id].accent } as React.CSSProperties} aria-pressed={draft.visible[id]} onClick={() => set({ visible: { ...draft.visible, [id]: !draft.visible[id] } })}>{PLATFORM_META[id].label}</button>)}</div></div>
      <label className="st-check"><input type="checkbox" checked={draft.notifyPace} onChange={(e) => set({ notifyPace: e.target.checked })} /> Notify if the pace will exhaust 5h before it resets</label>
      <label className="st-check"><input type="checkbox" checked={draft.notifyReset} onChange={(e) => set({ notifyReset: e.target.checked })} /> Notify when a window resets</label>
      <label className="st-check"><input type="checkbox" checked={draft.ghost} onChange={(e) => set({ ghost: e.target.checked })} /> Ghost mode: almost invisible until a quota matters or you hover</label>
      <label className="st-check"><input type="checkbox" checked={draft.autostart} onChange={(e) => set({ autostart: e.target.checked })} /> Start at login</label>
      <footer><button className="ghost" onClick={onCancel}>Cancel</button><button className="primary" onClick={save}>Save</button></footer>
    </section>
  </div>;
}
