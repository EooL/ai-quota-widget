import type { PlatformId } from "../lib/types";
import { PLATFORM_META } from "../lib/types";
import { estimateRequestRate } from "../lib/autoEstimate";

export type EffortLevel = "low" | "medium" | "high";
export type ModelProfile = { id: string; service: PlatformId; name: string };
export type PlannedModel = { id: string; profileId: string; requests: number };

const services: PlatformId[] = ["chatgpt", "gemini", "claude"];
const effortLevels: EffortLevel[] = ["low", "medium", "high"];
const windows = ["short", "weekly"] as const;

export default function ModelPlanner({ profiles, plannedModels, availableModels, onDetectModels, onProfilesChange, onPlanChange, onClose }: {
  profiles: ModelProfile[];
  plannedModels: PlannedModel[];
  availableModels: Record<PlatformId, { id: string; name: string }[]>;
  onDetectModels: () => void;
  onProfilesChange: (profiles: ModelProfile[]) => void;
  onPlanChange: (plan: PlannedModel[]) => void;
  onClose: () => void;
}) {
  const focusedProfiles = profiles.filter((profile) => services.includes(profile.service));
  const addProfile = (service: PlatformId = "chatgpt", name = "") => {
    if (!services.includes(service)) return;
    if (name && profiles.some((profile) => profile.service === service && profile.name === name)) return;
    onProfilesChange([...profiles, { id: crypto.randomUUID(), service, name }]);
  };
  const isAdded = (service: PlatformId, name: string) => profiles.some((profile) => profile.service === service && profile.name.toLowerCase() === name.toLowerCase());
  const addPlanLine = () => {
    if (!focusedProfiles.length) return;
    onPlanChange([...plannedModels, { id: crypto.randomUUID(), profileId: focusedProfiles[0].id, requests: 1 }]);
  };
  const changeProfile = (id: string, change: (profile: ModelProfile) => ModelProfile) => onProfilesChange(profiles.map((profile) => profile.id === id ? change(profile) : profile));

  return <div className="planner-overlay" data-testid="model-planner">
    <section className="planner-sheet">
      <header className="planner-header"><div><span className="planner-kicker">QUOTA ESTIMATOR</span><h2>Codex, Antigravity and Claude</h2></div><button onClick={onClose} aria-label="Close">×</button></header>
      <p className="planner-note">Models are detected from the CLIs (for Claude the list is fixed, since Claude Code exposes no catalog). Quota use and API-equivalent cost are estimated automatically from the model and effort; the cost assumes a typical token volume per request and is not a charge on your ChatGPT Plus, Claude or Antigravity subscription.</p>
      <button className="planner-add" onClick={onDetectModels}>↻ Re-detect models</button>
      <div className="detected-catalog">{services.map((service) => <div className="detected-service" key={service}><span>{PLATFORM_META[service].label}</span><div>{availableModels[service].length ? availableModels[service].map((model) => <button key={model.id} title={model.id} disabled={isAdded(service, model.name)} onClick={() => addProfile(service, model.name)}>{isAdded(service, model.name) ? "✓" : "＋"} {model.name}</button>) : <small>Models appear here once the CLI is detected.</small>}</div></div>)}</div>
      <div className="planner-columns"><span>Service / model</span><span>5h Low</span><span>5h Medium</span><span>5h High</span><span>7d Low</span><span>7d Medium</span><span>7d High</span><span/></div>
      <div className="profile-list">{focusedProfiles.map((profile) => <div className="profile-row" key={profile.id}>
        <div className="profile-identity">
          <select aria-label="Service" value={profile.service} onChange={(event) => changeProfile(profile.id, (item) => ({ ...item, service: event.target.value as PlatformId }))}>{services.map((id) => <option key={id} value={id}>{PLATFORM_META[id].label}</option>)}</select>
          <input aria-label="Model name" list={`detected-models-${profile.id}`} placeholder="Pick or type a model" value={profile.name} onChange={(event) => changeProfile(profile.id, (item) => ({ ...item, name: event.target.value }))}/>
          <datalist id={`detected-models-${profile.id}`}>{availableModels[profile.service].map((model) => <option key={model.id} value={model.name}>{model.id}</option>)}</datalist>
        </div>
        {windows.flatMap((window) => effortLevels.map((effort) => <span className="rate-estimate" key={`${window}-${effort}`} title="Automatic estimate for a typical request">{estimateRequestRate(profile.service, profile.name, effort, window).toFixed(1)}%</span>))}
        <button className="remove-profile" aria-label={`Remove ${profile.name || "model"}`} onClick={() => { onProfilesChange(profiles.filter((item) => item.id !== profile.id)); onPlanChange(plannedModels.filter((line) => line.profileId !== profile.id)); }}>×</button>
      </div>)}</div>
      <button className="planner-add" onClick={() => addProfile()}>＋ Add model</button>
      <div className="planner-plan-heading"><h3>Usage mix</h3><button onClick={addPlanLine} disabled={!focusedProfiles.length}>＋ Add model to plan</button></div>
      {plannedModels.filter((line) => focusedProfiles.some((profile) => profile.id === line.profileId)).length === 0 ? <p className="planner-empty">With no models selected, the dials use a typical request for each service. Add models here to estimate a mix.</p> : <div className="planned-list">{plannedModels.filter((line) => focusedProfiles.some((profile) => profile.id === line.profileId)).map((line) => <div className="planned-row" key={line.id}>
        <select aria-label="Model in mix" value={line.profileId} onChange={(event) => onPlanChange(plannedModels.map((item) => item.id === line.id ? { ...item, profileId: event.target.value } : item))}>{focusedProfiles.map((profile) => <option key={profile.id} value={profile.id}>{PLATFORM_META[profile.service].label} · {profile.name || "Default model"}</option>)}</select>
        <label><input aria-label="Number of requests" type="number" min="1" max="999" value={line.requests} onChange={(event) => onPlanChange(plannedModels.map((item) => item.id === line.id ? { ...item, requests: Math.max(1, Number(event.target.value) || 1) } : item))}/> requests</label>
        <button className="remove-profile" aria-label="Remove line from plan" onClick={() => onPlanChange(plannedModels.filter((item) => item.id !== line.id))}>×</button>
      </div>)}</div>}
      <footer className="planner-footer"><span>Estimates are approximate; real usage depends on context and response length.</span><button onClick={onClose}>Done</button></footer>
    </section>
  </div>;
}
