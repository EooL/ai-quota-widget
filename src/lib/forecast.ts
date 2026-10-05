import type { HistoryPoint, WindowQuota } from "./types";

export type Forecast = {
  /** procente consumate pe oră, pe baza ultimei ore */
  ratePerHour: number;
  /** când ajunge la 0 dacă ritmul se menține */
  zeroAtMs: number;
  etaMs: number;
  /** true dacă 0 vine înainte de resetul ferestrei */
  beforeReset: boolean;
  /** ce ar rămâne la momentul resetului (0 dacă beforeReset) */
  leftAtResetPct: number;
};

const LOOKBACK_MS = 60 * 60_000;
const MIN_SPAN_MS = 8 * 60_000;
const MIN_POINTS = 3;
const MIN_RATE_PER_HOUR = 0.5;

const pctOf = (p: HistoryPoint) => (p.remaining / Math.max(p.limit, 1)) * 100;

/** Ultimul segment continuu de consum: o creștere a cotei rămase = reset, tot ce e înainte se ignoră. */
export function currentSegment(points: HistoryPoint[], now: number): HistoryPoint[] {
  const recent = points.filter((p) => p.t >= now - LOOKBACK_MS && p.t <= now + 60_000);
  let start = 0;
  for (let i = 1; i < recent.length; i++) {
    if (pctOf(recent[i]) - pctOf(recent[i - 1]) > 2) start = i;
  }
  return recent.slice(start);
}

/** Regresie liniară (cele mai mici pătrate) pe segment; null dacă nu sunt date suficiente sau nu se consumă. */
export function forecastExhaustion(points: HistoryPoint[], quota: WindowQuota | null | undefined, now: number): Forecast | null {
  if (!quota) return null;
  const seg = currentSegment(points, now);
  if (seg.length < MIN_POINTS || seg[seg.length - 1].t - seg[0].t < MIN_SPAN_MS) return null;
  const t0 = seg[0].t;
  const xs = seg.map((p) => (p.t - t0) / 3_600_000);
  const ys = seg.map(pctOf);
  const n = seg.length;
  const mx = xs.reduce((a, b) => a + b, 0) / n;
  const my = ys.reduce((a, b) => a + b, 0) / n;
  let num = 0, den = 0;
  for (let i = 0; i < n; i++) { num += (xs[i] - mx) * (ys[i] - my); den += (xs[i] - mx) ** 2; }
  if (den === 0) return null;
  const consumption = -(num / den); // % pe oră (pozitiv = scade cota)
  if (consumption < MIN_RATE_PER_HOUR) return null;
  const remainingNow = Math.max(0, (quota.remaining / Math.max(quota.limit, 1)) * 100);
  const etaMs = (remainingNow / consumption) * 3_600_000;
  const untilReset = Math.max(0, quota.resetAtMs - now);
  const beforeReset = etaMs < untilReset;
  return {
    ratePerHour: consumption,
    zeroAtMs: now + etaMs,
    etaMs,
    beforeReset,
    leftAtResetPct: beforeReset ? 0 : Math.max(0, remainingNow - consumption * (untilReset / 3_600_000)),
  };
}

export function forecastLabel(f: Forecast | null, fmt: (ms: number) => string): { text: string; tone: "crit" | "ok" } | null {
  if (!f) return null;
  return f.beforeReset
    ? { text: `pace ${f.ratePerHour.toFixed(0)}%/h · hits 0 in ~${fmt(f.etaMs)}, before reset`, tone: "crit" }
    : { text: `pace ${f.ratePerHour.toFixed(0)}%/h · ~${Math.round(f.leftAtResetPct)}% left at reset`, tone: "ok" };
}

/** Propoziție completă pentru tooltip-uri. */
export function forecastSentence(f: Forecast | null, resetInMs: number, fmt: (ms: number) => string): string | null {
  if (!f) return null;
  return f.beforeReset
    ? `⚠ At the current pace (${f.ratePerHour.toFixed(0)}% per hour) the 5h window runs out in ~${fmt(f.etaMs)}, before it resets in ${fmt(resetInMs)}.`
    : `At the current pace (${f.ratePerHour.toFixed(0)}% per hour) about ${Math.round(f.leftAtResetPct)}% of the 5h window will be left when it resets in ${fmt(resetInMs)}.`;
}
