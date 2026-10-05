import type { HistoryPoint } from "./types";

const MIN = 60_000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;
export const RETENTION_MS = 7 * DAY;

/** Cât de rar păstrăm puncte, în funcție de vârstă: recent = tot, apoi tot mai rar. */
function bucketMs(age: number): number {
  if (age <= HOUR) return 0;          // ultima oră: toate (prognoza are nevoie de ele)
  if (age <= 36 * HOUR) return 5 * MIN;
  return 30 * MIN;
}

/** Elimină ce e mai vechi de 7 zile și rărește punctele vechi (un punct pe găleată). */
export function compact(points: HistoryPoint[], now: number): HistoryPoint[] {
  const out: HistoryPoint[] = [];
  let lastBucket = -1;
  let lastSize = -1;
  for (const p of points) {
    const age = now - p.t;
    if (age > RETENTION_MS) continue;
    const size = bucketMs(age);
    if (size === 0) { out.push(p); lastBucket = -1; lastSize = 0; continue; }
    const bucket = Math.floor(p.t / size);
    if (size === lastSize && bucket === lastBucket) continue;
    out.push(p);
    lastBucket = bucket;
    lastSize = size;
  }
  return out;
}

export function appendPoint(points: HistoryPoint[], point: HistoryPoint, now: number): HistoryPoint[] {
  const last = points[points.length - 1];
  // sondaje foarte apropiate (refresh manual) nu aduc informație nouă
  const base = last && point.t - last.t < 20_000 ? points.slice(0, -1) : points;
  return compact([...base, point], now);
}
