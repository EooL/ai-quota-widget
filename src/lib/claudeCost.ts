import { claudeRates } from "./autoEstimate";

export type ClaudeLogRow = { day: string; model: string; input: number; output: number; cacheWrite: number; cacheRead: number };
export type ClaudeCostSummary = { today: number; week: number; month: number; tokensMonth: number; byModel: { model: string; usd: number }[]; unpriced: number };

const M = 1_000_000;
/** Cache: citirea costă 0,1× intrare, scrierea (TTL 5 min) 1,25× intrare, ca în tarifele publice. */
export function rowCostUsd(row: ClaudeLogRow): number | null {
  const rates = claudeRates(row.model.toLowerCase());
  if (!rates) return null;
  return (row.input * rates.input + row.output * rates.output + row.cacheWrite * rates.input * 1.25 + row.cacheRead * rates.input * 0.1) / M;
}

const localDay = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;

export function summarizeClaudeCost(rows: ClaudeLogRow[], now: Date = new Date()): ClaudeCostSummary {
  const today = localDay(now);
  const weekStart = localDay(new Date(now.getFullYear(), now.getMonth(), now.getDate() - 6));
  const monthStart = localDay(new Date(now.getFullYear(), now.getMonth(), now.getDate() - 29));
  const out: ClaudeCostSummary = { today: 0, week: 0, month: 0, tokensMonth: 0, byModel: [], unpriced: 0 };
  const perModel = new Map<string, number>();
  for (const row of rows) {
    if (row.day < monthStart || row.day > today) continue;
    const usd = rowCostUsd(row);
    out.tokensMonth += row.input + row.output + row.cacheWrite + row.cacheRead;
    if (usd === null) { out.unpriced += 1; continue; }
    out.month += usd;
    if (row.day >= weekStart) out.week += usd;
    if (row.day === today) out.today += usd;
    perModel.set(row.model, (perModel.get(row.model) ?? 0) + usd);
  }
  out.byModel = [...perModel.entries()].map(([model, usd]) => ({ model, usd })).sort((a, b) => b.usd - a.usd);
  return out;
}
