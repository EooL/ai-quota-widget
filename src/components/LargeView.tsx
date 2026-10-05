import type { HistoryPoint, MetricsPayload, PlatformId } from "../lib/types";
import { PLATFORM_META } from "../lib/types";
import Sparkline from "./Sparkline";
import WeekChart from "./WeekChart";
import type { ClaudeCostSummary } from "../lib/claudeCost";
import type { ModelScenario } from "./PlatformRow";
import { Actions, ExtraChips, ForecastLine, RouteHints, Ring, SHORT_NAMES, left, level, pct, type ViewMode } from "./CompactView";

type Effort = "low" | "medium" | "high";
const usd = (n: number) => n < 0.01 ? `$${n.toFixed(4)}` : `$${n.toFixed(2)}`;
const f1 = (n: number) => n.toFixed(1).replace(/\.0$/, "");

type Props = {
  metrics: MetricsPayload | null; ids: PlatformId[]; now: number; warnPct: number;
  history: Record<PlatformId, HistoryPoint[]>; estimates: Record<PlatformId, ModelScenario[]>;
  claudeCost: ClaudeCostSummary | null;
  effort: Effort; onEffort: (e: Effort) => void;
  onView: (v: ViewMode) => void; onRefresh: () => void; onSettings: () => void; onCorner: () => void; onPlanner: () => void; onRetry: (id: PlatformId) => void;
};

function ScenarioRow({ s, short, weekly, warn }: { s: ModelScenario; short: number | null; weekly: number | null; warn: number }) {
  const afterShort = short === null ? null : short - s.shortConsumption;
  const afterWeekly = weekly === null ? null : weekly - s.weeklyConsumption;
  const fits = afterShort === null ? "none" : afterShort < 0 ? "crit" : level(afterShort, warn);
  const title = s.estimatedCost ? `${s.estimatedCost.inputTokens.toLocaleString("en-US")} input tokens + ${s.estimatedCost.outputTokens.toLocaleString("en-US")} output tokens / request` : "No public API price known for this model";
  return <div className={`lg-scn ${s.recommendation ? "rec" : ""}`}>
    <div className="lg-scn-name"><b>{s.modelName}</b><small>{s.isGeneral ? "typical request" : `${s.requests} request${s.requests === 1 ? "" : "s"}`}{s.recommendation ? " · recommended" : ""}</small></div>
    <div className={`lg-scn-5h tone-${fits}`}><small>5h</small>{short === null ? "—" : `${Math.round(short)}% → ${afterShort! < 0 ? "over limit" : `${f1(afterShort!)}%`}`}</div>
    <div className={`lg-scn-wk wk-${afterWeekly === null ? "none" : level(afterWeekly, warn)}`}><small>7d</small>{weekly === null ? "—" : `${Math.round(weekly)}% → ${f1(Math.max(0, afterWeekly!))}%`}</div>
    <div className="lg-scn-cost" title={title}>{s.estimatedCost ? <><b>{usd(s.estimatedCost.totalUsd)}</b><small>{usd(s.estimatedCost.perRequestUsd)} / request</small></> : <small>—</small>}</div>
  </div>;
}

export default function LargeView(p: Props) {
  const chartId = p.ids[0] ?? "chatgpt";
  return <div className="lg" >
    <header className="lg-head" data-tauri-drag-region><span data-tauri-drag-region>AI quotas</span><RouteHints metrics={p.metrics} ids={p.ids}/><Actions view="large" onView={p.onView} onRefresh={p.onRefresh} onSettings={p.onSettings} onCorner={p.onCorner}/></header>
    <div className="lg-body">
      {p.ids.map((id) => {
        const m = p.metrics?.[id]; const s = pct(m?.shortWindow); const w = pct(m?.weeklyWindow);
        const t = level(s, p.warnPct); const wt = level(w, p.warnPct);
        const note = m?.error ?? (m?.status === "unknown" ? "Unavailable" : null);
        const scn = (p.estimates[id] ?? []).filter((x) => !x.isGeneral);
        return <section className="lg-plat" key={id} style={{ "--accent": PLATFORM_META[id].accent } as React.CSSProperties}>
          <div className={`lg-plat-head tone-${t}`}>
            <Ring value={s} tone={t} weekly={w} weeklyTone={wt} size={64}/>
            <div className="lg-plat-info"><b>{SHORT_NAMES[id]}</b><small className="m-5h">5h · resets in {left(m?.shortWindow, p.now)}</small><small className={`m-wk wk-${wt}`}>weekly · resets in {left(m?.weeklyWindow, p.now)}</small><ForecastLine points={p.history[id]} quota={m?.shortWindow} now={p.now}/></div>
            <div className="lg-plat-stats"><span className="st-5h">{s === null ? "—" : `${Math.round(s)}%`}</span><span className={`st-wk wk-${wt}`}>7d {w === null ? "—" : `${Math.round(w)}%`}</span></div>
            {m?.status === "unknown" && <button className="lg-retry" onClick={() => p.onRetry(id)} title="Retry">↻</button>}
          </div>
          <div className="lg-extra"><ExtraChips m={m} now={p.now} warn={p.warnPct}/></div>
          {note && <div className="lg-note" title={m?.source}>{note}</div>}
          {scn.length > 0 && <div className="lg-scns">{scn.map((x) => <ScenarioRow key={x.id} s={x} short={s} weekly={w} warn={p.warnPct}/>)}</div>}
        </section>;
      })}
      {!p.metrics && <div className="mini-empty">Collecting quotas…</div>}
      <section className="lg-chart"><span>Ultimele 24 h · {SHORT_NAMES[chartId]}</span><div><Sparkline points={p.history[chartId] ?? []} color="#f5a524" id={`lg-${chartId}`}/></div></section>
      <section className="lg-chart"><span>Last 7 days · weekly remaining</span><WeekChart history={p.history} ids={p.ids} now={p.now}/></section>
      {p.claudeCost && <section className="lg-real" title="API-equivalent value, computed from the tokens in ~/.claude/projects. Not a charge on your Claude subscription.">
        <span className="lg-real-title">Claude · measured API-equivalent cost</span>
        <div className="lg-real-nums"><div><small>today</small><b>{usd(p.claudeCost.today)}</b></div><div><small>7 days</small><b>{usd(p.claudeCost.week)}</b></div><div><small>30 days</small><b>{usd(p.claudeCost.month)}</b></div></div>
        {p.claudeCost.byModel.length > 0 && <small className="lg-real-models">{p.claudeCost.byModel.slice(0, 3).map((x) => `${x.model.replace(/^claude-/, "")} ${usd(x.usd)}`).join(" · ")}</small>}
      </section>}
      <section className="lg-effort">
        <div className="mini-effort" role="group" aria-label="Effort">{(["low", "medium", "high"] as const).map((e) => <button key={e} className={p.effort === e ? "on" : ""} onClick={() => p.onEffort(e)}>{e === "low" ? "Low" : e === "medium" ? "Medium" : "High"}</button>)}</div>
        <button className="lg-planner" onClick={p.onPlanner}>Models & combinations ↗</button>
      </section>
    </div>
  </div>;
}
