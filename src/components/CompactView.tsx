import type { HistoryPoint, MetricsPayload, PlatformId, PlatformMetric, WindowQuota } from "../lib/types";
import { formatDuration } from "../lib/formatDuration";
import Sparkline from "./Sparkline";
import { routeHint } from "../lib/routeHint";
import { forecastExhaustion, forecastLabel, forecastSentence } from "../lib/forecast";

export type ViewMode = "small" | "medium" | "large";
type Effort = "low" | "medium" | "high";

export const SHORT_NAMES: Record<PlatformId, string> = { chatgpt: "Codex", gemini: "Antigravity", claude: "Claude" };
export const pct = (q: WindowQuota | null | undefined) => q ? Math.max(0, Math.min(100, q.remaining / Math.max(q.limit, 1) * 100)) : null;
export const level = (value: number | null, warn: number) => value === null ? "none" : value <= 5 ? "crit" : value <= warn ? "warn" : "ok";
export const COLORS = { ok: "#3ddc84", warn: "#f5a524", crit: "#ff5a52", none: "#6b7285" };
export const left = (q: WindowQuota | null | undefined, now: number) => q ? formatDuration(q.resetAtMs - now, true).replace(/ \d+s$/, "") : "—";

const fmtShort = (ms: number) => formatDuration(ms, true).replace(/ \d+s$/, "");

/** Text complet pentru tooltip-ul nativ (apare și în afara ferestrei mici). */
export function platformTooltip(m: PlatformMetric | undefined, history: HistoryPoint[] | undefined, now: number): string {
  if (!m) return "";
  const lines: string[] = [m.label];
  const s = pct(m.shortWindow), w = pct(m.weeklyWindow);
  if (m.shortWindow && s !== null) lines.push(`5h window: ${Math.round(s)}% left, resets in ${fmtShort(m.shortWindow.resetAtMs - now)}`);
  if (m.weeklyWindow && w !== null) lines.push(`Weekly: ${Math.round(w)}% left, resets in ${fmtShort(m.weeklyWindow.resetAtMs - now)}`);
  (m.extraWindows ?? []).forEach((x) => lines.push(`${x.label} weekly: ${Math.round(pct(x.window) as number)}% left`));
  const f = m.shortWindow ? forecastSentence(forecastExhaustion(history ?? [], m.shortWindow, now), m.shortWindow.resetAtMs - now, fmtShort) : null;
  if (f) lines.push("", f);
  if (m.error) lines.push("", m.error);
  lines.push("", `Source: ${m.source}`);
  return lines.join("\n");
}

export function ForecastLine({ points, quota, now }: { points: HistoryPoint[] | undefined; quota: WindowQuota | null | undefined; now: number }) {
  const label = forecastLabel(forecastExhaustion(points ?? [], quota, now), (ms) => formatDuration(ms, true).replace(/ \d+s$/, ""));
  return label ? <small className={`m-fc fc-${label.tone}`}>{label.text}</small> : null;
}

export function RouteHints({ metrics, ids }: { metrics: MetricsPayload | null; ids: PlatformId[] }) {
  const h = routeHint(metrics, ids);
  if (!h.easy && !h.hard) return null;
  const tip = "Which CLI to reach for next, based on your remaining quota (same rules as cli-orchestrator).\n"
    + "Simple tasks: quick, low-effort jobs. Goes to the cheapest platform that still has a comfortable amount left.\n"
    + "Complex tasks: demanding jobs. Goes to the most capable platform that still has quota.";
  return <div className="route-hints" title={tip}>
    {h.easy === h.hard ? <span>Use next <b>{SHORT_NAMES[h.easy as PlatformId]}</b></span> : <>
      {h.easy && <span>simple tasks <b>{SHORT_NAMES[h.easy]}</b></span>}{h.hard && <span>complex tasks <b>{SHORT_NAMES[h.hard]}</b></span>}</>}
  </div>;
}

export function ExtraChips({ m, now, warn }: { m: { extraWindows?: { key: string; label: string; window: WindowQuota }[] } | undefined; now: number; warn: number }) {
  const extras = m?.extraWindows ?? [];
  if (!extras.length) return null;
  return <div className="extra-chips">{extras.map((x) => { const v = pct(x.window) as number; return <span key={x.key} className={`wk-${level(v, warn)}`} title={`${x.label} weekly · resets in ${left(x.window, now)}`}>{x.label} 7d {Math.round(v)}%</span>; })}</div>;
}

export function Ring({ value, tone, weekly, weeklyTone, size }: { value: number | null; tone: keyof typeof COLORS; weekly: number | null; weeklyTone: keyof typeof COLORS; size: number }) {
  const r1 = 17.5, r2 = 11.5, c1 = 2 * Math.PI * r1, c2 = 2 * Math.PI * r2;
  return <div className="mini-ring" style={{ width: size, height: size, "--tone": COLORS[tone], "--tone2": COLORS[weeklyTone] } as React.CSSProperties}>
    <svg viewBox="0 0 40 40" aria-hidden="true">
      <circle className="mr-track" cx="20" cy="20" r={r1}/><circle className="mr-value" cx="20" cy="20" r={r1} strokeDasharray={c1} strokeDashoffset={c1 * (1 - (value ?? 0) / 100)}/>
      <circle className="mr-track mr-thin" cx="20" cy="20" r={r2}/><circle className="mr-value2" cx="20" cy="20" r={r2} strokeDasharray={c2} strokeDashoffset={c2 * (1 - (weekly ?? 0) / 100)}/>
    </svg>
    <span>{value === null ? "—" : Math.round(value)}</span>
  </div>;
}

export function Actions({ view, onView, onRefresh, onSettings, onCorner }: { view: ViewMode; onView: (v: ViewMode) => void; onRefresh: () => void; onSettings: () => void; onCorner: () => void }) {
  const sizes: ViewMode[] = ["small", "medium", "large"];
  return <div className="mini-actions">
    <button onClick={onRefresh} aria-label="Refresh" title="Refresh">↻</button>
    <button onClick={onSettings} aria-label="Settings" title="Settings">⚙</button>
    <button onClick={onCorner} aria-label="Corner" title="Move to another corner (Alt+Shift+E)">⌖</button>
    {sizes.map((s) => <button key={s} className={view === s ? "on" : ""} onClick={() => onView(s)} aria-label={s} title={s === "small" ? "Small" : s === "medium" ? "Medium" : "Large (models and costs)"}>{s === "small" ? "S" : s === "medium" ? "M" : "L"}</button>)}
  </div>;
}

type Props = {
  view: "small" | "medium"; metrics: MetricsPayload | null; ids: PlatformId[]; now: number; warnPct: number;
  history: Record<PlatformId, HistoryPoint[]>; effort: Effort; onEffort: (e: Effort) => void; caption: string;
  onView: (v: ViewMode) => void; onRefresh: () => void; onSettings: () => void; onCorner: () => void;
};

export default function CompactView(p: Props) {
  const actions = <Actions view={p.view} onView={p.onView} onRefresh={p.onRefresh} onSettings={p.onSettings} onCorner={p.onCorner}/>;
  if (p.view === "small") {
    return <div className="mini mini-small" data-tauri-drag-region>
      <div className="mini-rings">{p.ids.map((id) => {
        const m = p.metrics?.[id]; const v = pct(m?.shortWindow); const t = level(v, p.warnPct);
        return <div className={`mini-cell ${forecastExhaustion(p.history[id] ?? [], m?.shortWindow, p.now)?.beforeReset ? "fc-warn" : ""}`} key={id} data-tauri-drag-region title={platformTooltip(m, p.history[id], p.now)}>
          <Ring value={v} tone={t} weekly={pct(m?.weeklyWindow)} weeklyTone={level(pct(m?.weeklyWindow), p.warnPct)} size={64}/><b>{SHORT_NAMES[id]}</b><small className="m-5h">{m?.shortWindow ? `5h · reset ${left(m.shortWindow, p.now)}` : "5h · —"}</small><small className={`m-wk wk-${level(pct(m?.weeklyWindow), p.warnPct)}`}>{`weekly · ${pct(m?.weeklyWindow) === null ? "—" : Math.round(pct(m?.weeklyWindow) as number) + "%"}`}</small>
        </div>;
      })}</div>
      {!p.metrics && <div className="mini-empty">Collecting…</div>}
      <RouteHints metrics={p.metrics} ids={p.ids}/>
      {actions}
    </div>;
  }
  const chartId = p.ids[0] ?? "chatgpt";
  return <div className="mini mini-medium" data-tauri-drag-region>
    {p.ids.map((id) => {
      const m = p.metrics?.[id]; const s = pct(m?.shortWindow); const w = pct(m?.weeklyWindow); const t = level(s, p.warnPct);
      return <div className={`mini-row tone-${t}`} key={id} data-tauri-drag-region title={platformTooltip(m, p.history[id], p.now)}>
        <Ring value={s} tone={t} weekly={w} weeklyTone={level(w, p.warnPct)} size={50}/>
        <div className="mini-info"><b>{SHORT_NAMES[id]}</b><small className="m-5h">5h · resets in {left(m?.shortWindow, p.now)}</small><small className="m-wk">weekly · {left(m?.weeklyWindow, p.now)}</small><ForecastLine points={p.history[id]} quota={m?.shortWindow} now={p.now}/><ExtraChips m={m} now={p.now} warn={p.warnPct}/></div>
        <div className="mini-stats"><span className="st-5h">{s === null ? "—" : `${Math.round(s)}%`}</span><span className={`st-wk wk-${level(w, p.warnPct)}`}>7d {w === null ? "—" : `${Math.round(w)}%`}</span></div>
      </div>;
    })}
    {!p.metrics && <div className="mini-empty">Collecting…</div>}
    <div className="mini-chart"><span>Ultimele 24 h · {SHORT_NAMES[chartId]}</span><div><Sparkline points={p.history[chartId] ?? []} color="#f5a524" id={`mini-${chartId}`}/></div></div>
    <div className="mini-effort" role="group" aria-label="Effort">{(["low", "medium", "high"] as const).map((e) => <button key={e} className={p.effort === e ? "on" : ""} onClick={() => p.onEffort(e)}>{e === "low" ? "Low" : e === "medium" ? "Medium" : "High"}</button>)}</div>
    <div className="mini-caption">{p.caption}</div>
    <RouteHints metrics={p.metrics} ids={p.ids}/>
    {actions}
  </div>;
}
