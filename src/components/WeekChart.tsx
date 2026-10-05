import type { HistoryPoint, PlatformId } from "../lib/types";
import { PLATFORM_META } from "../lib/types";
import { SHORT_NAMES } from "./CompactView";

const DAY = 86_400_000;

export default function WeekChart({ history, ids, now }: { history: Record<PlatformId, HistoryPoint[]>; ids: PlatformId[]; now: number }) {
  const from = now - 7 * DAY;
  const series = ids.map((id) => ({ id, pts: (history[id] ?? []).filter((p) => p.t >= from && p.w !== undefined) })).filter((s) => s.pts.length >= 2);
  if (!series.length) return <div className="history-empty">7-day history: collecting (shows up after a few hours)…</div>;
  const x = (t: number) => ((t - from) / (7 * DAY)) * 300;
  const y = (w: number) => 4 + (1 - Math.max(0, Math.min(100, w)) / 100) * 52;
  return <div className="week-chart">
    <svg viewBox="0 0 300 60" preserveAspectRatio="none" role="img" aria-label="Weekly remaining, last 7 days">
      {[0, 25, 50, 75, 100].map((g) => <line key={g} x1="0" x2="300" y1={y(g)} y2={y(g)} className="wc-grid"/>)}
      {[1, 2, 3, 4, 5, 6].map((d) => <line key={d} x1={x(from + d * DAY)} x2={x(from + d * DAY)} y1="0" y2="60" className="wc-day"/>)}
      {series.map((s) => <polyline key={s.id} fill="none" stroke={PLATFORM_META[s.id].accent} strokeWidth="1.6" vectorEffect="non-scaling-stroke" strokeLinejoin="round" points={s.pts.map((p) => `${x(p.t).toFixed(1)},${y(p.w as number).toFixed(1)}`).join(" ")}/>)}
    </svg>
    <div className="wc-legend">{series.map((s) => <span key={s.id} style={{ "--accent": PLATFORM_META[s.id].accent } as React.CSSProperties}><i/>{SHORT_NAMES[s.id]}</span>)}</div>
  </div>;
}
