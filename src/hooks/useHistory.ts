import { useEffect, useState } from "react";
import type { HistoryState, MetricsPayload, PlatformId } from "../lib/types";
import { appendPoint, compact } from "../lib/history";

const KEY = "aqw:history:v1";
const empty: HistoryState = { chatgpt: [], gemini: [], claude: [] };

export function useHistory(metrics: MetricsPayload | null) {
  const [history, setHistory] = useState<HistoryState>(() => {
    try {
      const saved = JSON.parse(localStorage.getItem(KEY) ?? "{}");
      const now = Date.now();
      return { chatgpt: compact(saved.chatgpt ?? [], now), gemini: compact(saved.gemini ?? [], now), claude: compact(saved.claude ?? [], now) };
    } catch { return empty; }
  });

  useEffect(() => {
    if (!metrics) return;
    setHistory((current) => {
      const now = Date.now();
      const next = { ...current };
      (Object.keys(metrics) as PlatformId[]).forEach((id) => {
        const quota = metrics[id].shortWindow;
        if (!quota) return;
        const weekly = metrics[id].weeklyWindow;
        const w = weekly ? weekly.remaining / Math.max(weekly.limit, 1) * 100 : undefined;
        next[id] = appendPoint(current[id] ?? [], { t: now, remaining: quota.remaining, limit: quota.limit, ...(w === undefined ? {} : { w }) }, now);
      });
      try { localStorage.setItem(KEY, JSON.stringify(next)); } catch { /* storage unavailable */ }
      return next;
    });
  }, [metrics]);
  return history;
}
