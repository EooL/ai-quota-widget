import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import type { MetricsPayload, PlatformId } from "./lib/types";
import { PLATFORM_IDS } from "./lib/types";
import { invokeCommand, isTauriRuntime, mockMetrics } from "./lib/tauri";
import { useHistory } from "./hooks/useHistory";
import { useInterval } from "./hooks/useInterval";
import { useNow } from "./hooks/useNow";
import { useSettings } from "./hooks/useSettings";
import LargeView from "./components/LargeView";
import type { ModelScenario } from "./components/PlatformRow";
import SettingsPanel from "./components/SettingsPanel";
import CompactView, { type ViewMode } from "./components/CompactView";
import ModelPlanner, { type EffortLevel, type ModelProfile, type PlannedModel } from "./components/ModelPlanner";
import { useClaudeCost } from "./hooks/useClaudeCost";
import { forecastExhaustion } from "./lib/forecast";
import { formatDuration } from "./lib/formatDuration";
import { estimateApiCost, estimateRequestRate, modelStrength } from "./lib/autoEstimate";

const PLANNER_KEY = "aqw:model-planner:v1";
const VIEW_KEY = "aqw:view:v1";
const CORNER_KEY = "aqw:corner:v1";
const POS_KEY = "aqw:pos:v1";
const CORNERS = ["tr", "br", "bl", "tl"] as const;
const VIEW_ORDER: ViewMode[] = ["small", "medium", "large"];
const readCorner = (): string => { try { return localStorage.getItem(CORNER_KEY) ?? ""; } catch { return ""; } };
const EXPANDED_SIZE = { width: 760, height: 860 };
const viewSize = (view: ViewMode, rows: number) => view === "small" ? { width: Math.max(230, Math.max(rows, 1) * 92 + 28), height: 164 } : view === "medium" ? { width: 372, height: 236 + Math.max(rows, 1) * 62 } : EXPANDED_SIZE;
const readView = (): ViewMode => { try { const v = localStorage.getItem(VIEW_KEY); return v === "medium" || v === "large" ? v : "small"; } catch { return "small"; } };

function readPlanner() {
  try {
    const saved = JSON.parse(localStorage.getItem(PLANNER_KEY) ?? "{}");
    const profiles: ModelProfile[] = Array.isArray(saved.profiles) ? saved.profiles.map((profile: any) => ({ id: profile.id, service: profile.service, name: profile.name ?? "" })) : [];
    return { profiles, planned: Array.isArray(saved.planned) ? saved.planned as PlannedModel[] : [] };
  } catch { return { profiles: [] as ModelProfile[], planned: [] as PlannedModel[] }; }
}

type AvailableModels = Record<PlatformId, { id: string; name: string }[]>;
const EMPTY_CATALOG: AvailableModels = { chatgpt: [], gemini: [], claude: [] };

export default function App() {
  const { settings, update, togglePlatform } = useSettings();
  const [metrics, setMetrics] = useState<MetricsPayload | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [plannerOpen, setPlannerOpen] = useState(false);
  const [view, setView] = useState<ViewMode>(readView);
  const corner = useRef(readCorner());
  const programmaticMoveUntil = useRef(0);
  const snapTo = async (next: string) => {
    corner.current = next;
    try { localStorage.setItem(CORNER_KEY, next); } catch { /* storage unavailable */ }
    if (!next || !isTauriRuntime()) return;
    programmaticMoveUntil.current = Date.now() + 1000;
    await invokeCommand("snap_window", { corner: next }).catch(() => undefined);
  };
  const nextCorner = () => { const i = CORNERS.indexOf(corner.current as typeof CORNERS[number]); void snapTo(CORNERS[(i + 1) % CORNERS.length]); };
  const [effort, setEffort] = useState<EffortLevel>("medium");
  const [planner, setPlanner] = useState(readPlanner);
  const [availableModels, setAvailableModels] = useState<AvailableModels>(EMPTY_CATALOG);
  const notified = useRef<Record<string, boolean>>({});
  const now = useNow();
  const history = useHistory(metrics);
  const claudeCost = useClaudeCost(view === "large" && settings.visible.claude);
  useEffect(() => {
    if (!isTauriRuntime()) return;
    if (!corner.current) {
      try {
        const saved = JSON.parse(localStorage.getItem(POS_KEY) ?? "null");
        if (saved && Number.isFinite(saved.x) && Number.isFinite(saved.y) && saved.x > -4000 && saved.y > -4000) {
          programmaticMoveUntil.current = Date.now() + 1000;
          void invokeCommand("set_window_position", { x: Math.round(saved.x), y: Math.round(saved.y) }).catch(() => undefined);
        }
      } catch { /* position unavailable */ }
    }
    let timer: ReturnType<typeof setTimeout> | undefined;
    const unlistenMove = listen<{ x: number; y: number }>("tauri://move", (event) => {
      if (Date.now() < programmaticMoveUntil.current) return;
      if (corner.current) { corner.current = ""; try { localStorage.setItem(CORNER_KEY, ""); } catch { /* storage unavailable */ } }
      clearTimeout(timer);
      timer = setTimeout(() => { try { localStorage.setItem(POS_KEY, JSON.stringify(event.payload)); } catch { /* storage unavailable */ } }, 400);
    });
    const unlistenKeys = Promise.all([
      listen("shortcut:cycle", () => setView((current) => VIEW_ORDER[(VIEW_ORDER.indexOf(current) + 1) % VIEW_ORDER.length])),
      listen("shortcut:corner", () => nextCorner()),
    ]);
    return () => { clearTimeout(timer); void unlistenMove.then((off) => off()); void unlistenKeys.then((offs) => offs.forEach((off) => off())); };
  }, []);
  const visibleCount = PLATFORM_IDS.filter((id) => settings.visible[id]).length;
  useEffect(() => {
    try { localStorage.setItem(VIEW_KEY, view); } catch { /* storage unavailable */ }
    if (!isTauriRuntime()) return;
    const win = getCurrentWindow();
    void (async () => {
      try {
        const size = viewSize(view, visibleCount);
        if (view === "large") await win.setMinSize(new LogicalSize(520, 600));
        else await win.setMinSize(new LogicalSize(200, 100));
        await win.setSize(new LogicalSize(size.width, size.height));
        if (corner.current) await snapTo(corner.current);
      } catch { /* resizing is best effort */ }
    })();
  }, [view, visibleCount]);
  useEffect(() => { localStorage.setItem(PLANNER_KEY, JSON.stringify(planner)); }, [planner]);
  const detectModels = async () => {
    if (!isTauriRuntime()) return;
    try {
      const catalog = await invokeCommand<AvailableModels>("get_available_models");
      setAvailableModels(catalog);
      setPlanner((current) => {
        const additions = (["chatgpt", "gemini", "claude"] as const).flatMap((service) => catalog[service].filter((model) => (service !== "claude" || !/fable|mythos/i.test(`${model.id} ${model.name}`)) && !current.profiles.some((profile) => profile.service === service && profile.name.toLowerCase() === model.name.toLowerCase())).map((model) => ({ id: crypto.randomUUID(), service, name: model.name })));
        return additions.length ? { ...current, profiles: [...current.profiles, ...additions] } : current;
      });
    }
    catch { setAvailableModels(EMPTY_CATALOG); }
  };
  useEffect(() => { void detectModels(); }, []);
  const refresh = async () => { try { const next = isTauriRuntime() ? await invokeCommand<MetricsPayload>("get_platform_metrics") : mockMetrics(); setMetrics(next); } catch { setMetrics(mockMetrics()); } };
  useEffect(() => { void refresh(); }, []);
  useInterval(() => void refresh(), settings.pollIntervalSec * 1000);
  useEffect(() => {
    if (!metrics) return;
    PLATFORM_IDS.forEach((id) => {
      const warning = metrics[id].status === "warning";
      if (warning && !notified.current[id]) {
        notified.current[id] = true;
        void invokeCommand("push_notification", { title: `${metrics[id].label}: quota low`, body: "Your usage window is close to its limit." }).catch(() => undefined);
      } else if (!warning) {
        notified.current[id] = false;
      }
    });
  }, [metrics]);
  const prevShort = useRef<Record<string, number>>({});
  const paceNotified = useRef<Record<string, number>>({});
  const notify = (title: string, body: string) => { void invokeCommand("push_notification", { title, body }).catch(() => undefined); };
  useEffect(() => {
    if (!metrics) return;
    PLATFORM_IDS.forEach((id) => {
      const q = metrics[id].shortWindow;
      if (!q) return;
      const nowPct = q.remaining / Math.max(q.limit, 1) * 100;
      const before = prevShort.current[id];
      if (settings.notifyReset && before !== undefined && before <= settings.warningThresholdPct && nowPct >= before + 30) {
        notify(`${metrics[id].label}: 5h window has reset`, `You have ${Math.round(nowPct)}% available again.`);
      }
      prevShort.current[id] = nowPct;
    });
  }, [metrics]);
  useEffect(() => {
    if (!metrics || !settings.notifyPace) return;
    const t = Date.now();
    PLATFORM_IDS.forEach((id) => {
      const q = metrics[id].shortWindow;
      const f = forecastExhaustion(history[id] ?? [], q, t);
      if (!q || !f || !f.beforeReset || f.etaMs > 90 * 60_000) return;
      const windowKey = Math.round(q.resetAtMs / 600_000);
      if (paceNotified.current[id] === windowKey) return;
      paceNotified.current[id] = windowKey;
      notify(`${metrics[id].label}: pace too high`, `At this pace the 5h window runs out in ~${formatDuration(f.etaMs, true).replace(/ \d+s$/, "")}; it resets in ${formatDuration(q.resetAtMs - t, true).replace(/ \d+s$/, "")}.`);
    });
  }, [history]);
  useEffect(() => {
    if (!isTauriRuntime()) return;
    const unlisten = Promise.all([
      listen("tray:refresh", () => void refresh()),
      listen("tray:open-settings", () => setSettingsOpen(true)),
      listen<string>("tray:toggle-platform", (event) => togglePlatform(event.payload as PlatformId)),
    ]);
    return () => { void unlisten.then((cleanups) => cleanups.forEach((cleanup) => cleanup())); };
  }, [settings.visible]);
  const visible = metrics ? PLATFORM_IDS.filter((id) => settings.visible[id]) : [];
  const estimates = Object.fromEntries(PLATFORM_IDS.map((id) => {
    const profiles = planner.profiles.filter((profile) => profile.service === id && profile.name.trim());
    const lines = planner.planned.filter((line) => planner.profiles.find((profile) => profile.id === line.profileId)?.service === id);
    const groupedLines = lines.reduce((groups, line) => {
      const existing = groups.find((group) => group.profileId === line.profileId);
      if (existing) existing.requests += line.requests;
      else groups.push({ profileId: line.profileId, requests: line.requests });
      return groups;
    }, [] as { profileId: string; requests: number }[]);
    const quotaLeft = metrics?.[id]?.shortWindow ? metrics[id].shortWindow!.remaining / Math.max(metrics[id].shortWindow!.limit, 1) * 100 : null;
    const candidates = profiles.length ? profiles.map((profile) => {
      const requests = groupedLines.find((line) => line.profileId === profile.id)?.requests ?? 1;
      return { id: profile.id, modelName: profile.name, requests, shortConsumption: estimateRequestRate(id, profile.name, effort, "short") * requests, weeklyConsumption: estimateRequestRate(id, profile.name, effort, "weekly") * requests, estimatedCost: estimateApiCost(id, profile.name, effort, requests), isGeneral: false, strength: modelStrength(id, profile.name) };
    }).sort((a, b) => b.strength - a.strength) : [{ id: `${id}-general`, modelName: "General estimate", requests: 1, shortConsumption: estimateRequestRate(id, "", effort, "short"), weeklyConsumption: estimateRequestRate(id, "", effort, "weekly"), estimatedCost: null, isGeneral: true, strength: 0 }];
    const modelCandidates = candidates.filter((item) => !item.isGeneral);
    const fitting = quotaLeft === null ? [] : modelCandidates.filter((item) => item.shortConsumption <= quotaLeft);
    const safeBuffer = quotaLeft !== null && quotaLeft > 10 ? 10 : 0;
    const safe = fitting.filter((item) => quotaLeft !== null && quotaLeft - item.shortConsumption >= safeBuffer);
    const recommendedId = (safe[0] ?? fitting[0])?.id;
    const recommendationType = safe.length ? "RECOMMENDED · SAFE" : fitting.length ? "RECOMMENDED · TIGHT" : undefined;
    return [id, candidates.map((item) => ({ ...item, recommendation: item.id === recommendedId ? recommendationType : undefined }))];
  })) as unknown as Record<PlatformId, ModelScenario[]>;
  if (view !== "large") {
    const caption = (() => {
      const label = effort === "low" ? "Low" : effort === "medium" ? "Medium" : "High";
      const picks = visible.map((id) => (estimates[id] ?? []).find((item) => item.recommendation)).filter(Boolean) as ModelScenario[];
      if (picks.length) return `${label} effort: ${picks.slice(0, 2).map((item) => item.modelName).join(" · ")} fits in 5h`;
      const planned = visible.some((id) => (estimates[id] ?? []).some((item) => !item.isGeneral));
      return planned ? `${label} effort: no planned model fits in 5h` : `${label} effort: typical request, no models picked`;
    })();
    const alert = visible.some((id) => {
      const q = metrics?.[id]?.shortWindow;
      const v = q ? q.remaining / Math.max(q.limit, 1) * 100 : null;
      return (v !== null && v <= settings.warningThresholdPct) || forecastExhaustion(history[id] ?? [], q, now)?.beforeReset === true;
    });
    return <main className={`compact-shell${settings.ghost && !alert ? " ghost" : ""}${alert ? " alert" : ""}`}>
      <CompactView view={view} metrics={metrics} ids={visible} now={now} warnPct={settings.warningThresholdPct} history={history} effort={effort} onEffort={setEffort} caption={caption} onView={setView} onRefresh={() => void refresh()} onSettings={() => { setView("large"); setSettingsOpen(true); }} onCorner={nextCorner} />
    </main>;
  }
  return <main className="compact-shell lg-shell">
    <LargeView claudeCost={claudeCost} metrics={metrics} ids={visible} now={now} warnPct={settings.warningThresholdPct} history={history} estimates={estimates} effort={effort} onEffort={setEffort} onView={setView} onRefresh={() => void refresh()} onSettings={() => setSettingsOpen(true)} onCorner={nextCorner} onPlanner={() => setPlannerOpen(true)} onRetry={(id) => void invokeCommand("refresh_platform", { platform: id }).then((metric) => setMetrics((current) => current ? { ...current, [id]: metric as MetricsPayload[typeof id] } : current)).catch(() => undefined)} />
    {settingsOpen && <SettingsPanel initial={settings} onCancel={() => setSettingsOpen(false)} onSave={(next) => { update(next); setSettingsOpen(false); }} />}
    {plannerOpen && <ModelPlanner profiles={planner.profiles} plannedModels={planner.planned} availableModels={availableModels} onDetectModels={() => void detectModels()} onProfilesChange={(profiles) => setPlanner((current) => ({ ...current, profiles }))} onPlanChange={(planned) => setPlanner((current) => ({ ...current, planned }))} onClose={() => setPlannerOpen(false)} />}
    <button className="window-resize-grip" aria-label="Resize window" title="Drag to resize" onMouseDown={(event) => { event.preventDefault(); if (isTauriRuntime()) void getCurrentWindow().startResizeDragging("SouthEast").catch(() => undefined); }}><span/><span/><span/></button>
  </main>;
}
