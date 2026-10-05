import { useEffect, useState } from "react";
import { invokeCommand, isTauriRuntime } from "../lib/tauri";
import { summarizeClaudeCost, type ClaudeCostSummary, type ClaudeLogRow } from "../lib/claudeCost";

/** Citește jurnalele Claude Code doar cât timp panoul e vizibil; reîmprospătare la 5 minute. */
export function useClaudeCost(enabled: boolean): ClaudeCostSummary | null {
  const [summary, setSummary] = useState<ClaudeCostSummary | null>(null);
  useEffect(() => {
    if (!enabled || !isTauriRuntime()) return;
    let cancelled = false;
    const load = () => invokeCommand<ClaudeLogRow[]>("claude_usage_totals")
      .then((rows) => { if (!cancelled) setSummary(summarizeClaudeCost(rows)); })
      .catch(() => undefined);
    void load();
    const timer = setInterval(() => void load(), 5 * 60_000);
    return () => { cancelled = true; clearInterval(timer); };
  }, [enabled]);
  return summary;
}
