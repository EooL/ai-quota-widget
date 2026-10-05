import type { PlatformId } from "./types";
import type { EffortLevel } from "../components/ModelPlanner";

export type QuotaWindow = "short" | "weekly";
export type EstimatedApiCost = { perRequestUsd: number; totalUsd: number; inputTokens: number; outputTokens: number };

const effortFactor: Record<EffortLevel, number> = { low: 0.55, medium: 1, high: 1.8 };

// Transparent, illustrative token budgets for one coding request. The actual
// amount depends on prompt/context size, tool loops, and generated output.
const tokenBudget: Record<EffortLevel, { input: number; output: number }> = {
  low: { input: 8_000, output: 1_000 },
  medium: { input: 24_000, output: 3_000 },
  high: { input: 64_000, output: 8_000 },
};

/** "Claude Opus 4.1", "claude-sonnet-5-5", "claude-haiku-4-5-20251001", "opus", "sonnet[1m]" -> 4.1 / 5.5 / 4.5 / 0 / 0 */
function parseVersion(name: string): number {
  const cleaned = name.replace(/\[.*?\]/g, "").replace(/\d{8}/g, "");
  const match = cleaned.match(/(\d+)(?:[.-](\d{1,2}))?/);
  return match ? Number(match[1]) + (match[2] ? Number(match[2]) / 10 : 0) : 0;
}

/** Public API prices (USD per million tokens, platform.claude.com pricing). An alias without a version means the current model. */
export function claudeRates(name: string): { input: number; output: number } | null {
  const version = parseVersion(name);
  if (/fable|mythos/.test(name)) return { input: 10, output: 50 };
  if (/opus/.test(name)) return version === 0 || version >= 5.5 ? { input: 4, output: 20 } : version >= 4.5 ? { input: 5, output: 25 } : { input: 15, output: 75 };
  if (/sonnet/.test(name)) return version === 0 || version >= 5 ? { input: 2, output: 10 } : { input: 3, output: 15 };
  if (/haiku/.test(name)) return version === 0 || version >= 4.5 ? { input: 1, output: 5 } : { input: 0.8, output: 4 };
  return null;
}

function apiRates(service: PlatformId, modelName: string): { input: number; output: number } | null {
  const name = modelName.toLowerCase();
  if (service === "chatgpt") {
    if (/^gpt-?5\.5$/.test(name.trim())) return { input: 5, output: 30 }; // public API price, openrouter.ai/openai/gpt-5.5
    if (/astra/.test(name)) return { input: 10, output: 50 };
    if (/sol/.test(name)) return { input: 4, output: 20 };
    if (/terra/.test(name)) return { input: 2, output: 12 };
    if (/luna/.test(name)) return { input: 0.2, output: 1.2 };
    return null;
  }
  if (service === "gemini") {
    if (/3\.8\s*flash|3\.7\s*flash|3\.6\s*flash/.test(name)) return { input: 0.75, output: 3.75 };
    if (/3\.1\s*pro/.test(name)) {
      // Promotional standard pricing through 2026-12-31, then regular rates.
      return new Date() < new Date("2027-01-01T00:00:00")
        ? { input: 1, output: 5 }
        : { input: 2, output: 10 };
    }
    return null;
  }
  if (service === "claude") return claudeRates(name);
  return null;
}

export function estimateApiCost(service: PlatformId, modelName: string, effort: EffortLevel, requests: number): EstimatedApiCost | null {
  const rates = apiRates(service, modelName);
  if (!rates || !modelName.trim()) return null;
  const budget = tokenBudget[effort];
  const perRequestUsd = (budget.input * rates.input + budget.output * rates.output) / 1_000_000;
  return { perRequestUsd, totalUsd: perRequestUsd * requests, inputTokens: budget.input, outputTokens: budget.output };
}

/** Heuristic for a typical request. Providers do not expose per-request quota prices. */
export function estimateRequestRate(service: PlatformId, modelName: string, effort: EffortLevel, window: QuotaWindow): number {
  const name = modelName.toLowerCase();
  let modelFactor = 1;
  if (service === "chatgpt") {
    if (/luna|mini|nano/.test(name)) modelFactor = 0.65;
    else if (/terra/.test(name)) modelFactor = 0.9;
    else if (/sol|5\.5/.test(name)) modelFactor = 1.2;
    else if (/astra|pro|max/.test(name)) modelFactor = 1.5;
  } else if (service === "gemini") {
    if (/flash|flash-lite/.test(name)) modelFactor = 0.65;
    else if (/pro/.test(name)) modelFactor = 1.4;
  } else if (service === "claude") {
    // quota burn scales with price: Sonnet 5.5 ($2 input) = 1x
    const rates = claudeRates(name);
    modelFactor = rates ? Math.min(5, Math.max(0.4, rates.input / 2)) : 1;
  }

  const typicalRequest = service === "chatgpt"
    ? { short: 5, weekly: 1.4 }
    : service === "gemini"
      ? { short: 4, weekly: 1.1 }
      : { short: 5, weekly: 1.4 };

  return Number((typicalRequest[window] * modelFactor * effortFactor[effort]).toFixed(1));
}

export function modelStrength(service: PlatformId, modelName: string): number {
  const name = modelName.toLowerCase();
  if (service === "claude") {
    const family = /fable|mythos/.test(name) ? 700 : /opus/.test(name) ? 600 : /sonnet/.test(name) ? 500 : /haiku/.test(name) ? 300 : 100;
    return family + parseVersion(name) * 5;
  }
  if (service === "chatgpt") {
    const codexRank = name.includes("astra") ? 600 : name.includes("sol") ? 500 : name.includes("terra") ? 400 : name.includes("luna") || name.includes("mini") ? 300 : name.includes("5.5") ? 200 : 100;
    const reasoningRank = /high|max/.test(name) ? 20 : /medium/.test(name) ? 10 : 0;
    return codexRank + reasoningRank;
  }
  const version = name.match(/gemini\s*(\d+(?:\.\d+)?)/)?.[1] ?? name.match(/gemini-(\d+(?:\.\d+)?)/)?.[1] ?? "0";
  const versionRank = Number(version) || 0;
  const familyRank = /pro/.test(name) ? 300 : /flash/.test(name) ? 200 : 100;
  const effortRank = /high/.test(name) ? 30 : /medium/.test(name) ? 20 : /low/.test(name) ? 10 : 0;
  return familyRank + versionRank * 5 + effortRank;
}
