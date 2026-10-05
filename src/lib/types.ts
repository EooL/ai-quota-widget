export type PlatformId = "chatgpt" | "gemini" | "claude";

export type PlatformStatus = "available" | "warning" | "blocked" | "unknown";

export interface WindowQuota {
  resetAtMs: number;
  remaining: number;
  limit: number;
}

export interface ExtraWindow {
  key: string;
  label: string;
  window: WindowQuota;
}

export interface PlatformMetric {
  id: PlatformId | string;
  label: string;
  shortWindow: WindowQuota | null;
  weeklyWindow: WindowQuota | null;
  status: PlatformStatus;
  error: string | null;
  source: string;
  fetchedAtMs: number;
  extraWindows?: ExtraWindow[];
}

export interface MetricsPayload {
  chatgpt: PlatformMetric;
  gemini: PlatformMetric;
  claude: PlatformMetric;
}

export interface AppSettings {
  pollIntervalSec: number;
  warningThresholdPct: number;
  autostart: boolean;
  notifyPace: boolean;
  notifyReset: boolean;
  ghost: boolean;
  visible: Record<PlatformId, boolean>;
}

export interface HistoryPoint {
  t: number;
  remaining: number;
  limit: number;
  /** procent rămas din fereastra weekly (0–100), dacă platforma o raportează */
  w?: number;
}

export type HistoryState = Record<PlatformId, HistoryPoint[]>;

export const PLATFORM_META: Record<
  PlatformId,
  { label: string; accent: string }
> = {
  chatgpt: { label: "ChatGPT Plus", accent: "#10a37f" },
  gemini: { label: "Gemini Models", accent: "#4285f4" },
  claude: { label: "Claude", accent: "#d97706" },
};

export const PLATFORM_IDS: PlatformId[] = ["chatgpt", "gemini", "claude"];
