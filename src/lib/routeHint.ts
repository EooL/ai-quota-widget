import type { MetricsPayload, PlatformId, WindowQuota } from "./types";

/** Oglindește regulile din cli-orchestrator (orch/orchestrator.py): epuizat ≤0,5%, redus <15%. */
const BLOCKED_PCT = 0.5;
const LOW_PCT = 15;
const EASY_COMFORT_PCT = 30;
/** Ordinea de capabilitate pentru task-uri grele (T2–T3), ca în catalogul orchestratorului. */
const HARD_ORDER: PlatformId[] = ["claude", "chatgpt", "gemini"];

const pct = (q: WindowQuota | null | undefined) => (q ? (q.remaining / Math.max(q.limit, 1)) * 100 : null);

export type Headroom = { id: PlatformId; headroom: number; blocked: boolean; low: boolean };
export type RouteHint = { easy: PlatformId | null; hard: PlatformId | null };

export function headrooms(metrics: MetricsPayload | null, ids: PlatformId[]): Headroom[] {
  if (!metrics) return [];
  return ids.flatMap((id) => {
    const values = [pct(metrics[id]?.shortWindow), pct(metrics[id]?.weeklyWindow)].filter((v): v is number => v !== null);
    if (!values.length) return [];
    const headroom = Math.min(...values);
    return [{ id, headroom, blocked: headroom <= BLOCKED_PCT, low: headroom < LOW_PCT }];
  });
}

export function routeHint(metrics: MetricsPayload | null, ids: PlatformId[]): RouteHint {
  const usable = headrooms(metrics, ids).filter((h) => !h.blocked);
  if (!usable.length) return { easy: null, hard: null };
  const roomy = usable.filter((h) => !h.low);
  const pool = roomy.length ? roomy : usable;
  // Task-uri ușoare: întâi platformele „ieftine" cât au ≥30% rămas; altfel cea mai liberă.
  const cheapFirst = [...HARD_ORDER].reverse().find((id) => pool.some((h) => h.id === id && h.headroom >= EASY_COMFORT_PCT));
  const easy = cheapFirst ?? [...pool].sort((a, b) => b.headroom - a.headroom)[0].id;
  const hard = roomy.length
    ? HARD_ORDER.find((id) => roomy.some((h) => h.id === id)) ?? null
    : [...usable].sort((a, b) => b.headroom - a.headroom)[0].id;
  return { easy, hard };
}
