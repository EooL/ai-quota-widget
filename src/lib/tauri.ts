import type { MetricsPayload, PlatformId, PlatformMetric } from "./types";
import { PLATFORM_META } from "./types";

export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function mockQuota(remaining: number, limit: number, hours: number) {
  return {
    resetAtMs: Date.now() + hours * 3600 * 1000,
    remaining,
    limit,
  };
}

function mockMetric(id: PlatformId, remaining: number, limit: number): PlatformMetric {
  const ratio = remaining / limit;
  const status =
    remaining <= 0 ? "blocked" : ratio <= 0.2 ? "warning" : "available";
  return {
    id,
    label: PLATFORM_META[id].label,
    shortWindow: mockQuota(remaining, limit, id === "gemini" ? 5 : 4),
    weeklyWindow: mockQuota(Math.min(limit * 4, remaining * 8), limit * 8, 168),
    status,
    error: null,
    source: "mock:browser",
    fetchedAtMs: Date.now(),
  };
}

export function mockMetrics(): MetricsPayload {
  return {
    chatgpt: mockMetric("chatgpt", 28, 40),
    gemini: mockMetric("gemini", 12, 50),
    claude: mockMetric("claude", 7, 45),
  };
}

export async function invokeCommand<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!isTauriRuntime()) {
    throw new Error("Not running inside Tauri");
  }
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}
