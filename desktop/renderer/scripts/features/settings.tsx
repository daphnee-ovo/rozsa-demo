import React, { useEffect, useState, type ReactNode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { ProductClient } from "../../product-client.js";
import type { ModelOption, ProductBridge, SettingsScope, SettingsSnapshot, ThemeDefinition, ThemeSummary } from "../../../main/session-contract.js";

type Props = { client: ProductClient; pane: "general" | "appearance" };
type Settings = Record<string, unknown>;
type SettingsRuntimeApi = {
  boundModules: Set<string>;
  byId(id: string): HTMLElement | null;
  switchSettingsTab(pane: string, button?: HTMLElement): void;
  closeSettings(): void;
  selectThemeModeCard(mode: string): void;
  previewFontSize(value: string | number, control?: HTMLElement | null): void;
  commitFontSize(value: string | number): void;
};
const initialThemes: ThemeSummary[] = [];
let appliedThemeVariables = new Set<string>();

export const suspendedSettingsPanes = Object.freeze(["skills", "tools", "keyboard-shortcuts", "permissions"]);
export function isSettingsPaneAvailable(pane: string): boolean { return ["general", "appearance", "extensions", "dev-flow"].includes(pane); }
export function configureSettingsSuspensions(root: ParentNode = document): void {
  for (const pane of suspendedSettingsPanes) {
    const container = root.querySelector<HTMLElement>(`#pane-${pane}`);
    if (container) { container.replaceChildren(); container.dataset.uiSuspended = pane; }
    root.querySelectorAll<HTMLButtonElement>(`[data-settings-pane="${pane}"]`).forEach(button => { button.disabled = true; button.setAttribute("aria-disabled", "true"); });
  }
}
export function syncFontSizeControls(root: ParentNode, value: number, { updateInput = true }: { updateInput?: boolean } = {}): void {
  const range = root.querySelector<HTMLInputElement>("#settingsFontSizeRange");
  const input = root.querySelector<HTMLInputElement>("#settingsFontSizeInput");
  if (input && updateInput) input.value = String(value);
  if (range) {
    range.value = String(value);
    range.style.setProperty("--appearance-progress", `${((value - Number(range.min)) / (Number(range.max) - Number(range.min))) * 100}%`);
  }
}
export function renderThemeModeCards(root: ParentNode, mode: string): void {
  root.querySelectorAll<HTMLElement>("[data-theme-mode-card]").forEach(card => {
    const active = card.dataset.themeModeCard === mode;
    card.classList.toggle("active", active); card.setAttribute("aria-pressed", String(active));
  });
}
export function init(api: SettingsRuntimeApi): void {
  if (api.boundModules.has("settings")) return;
  api.boundModules.add("settings");
  const root = api.byId("settingsPanel");
  if (!root) throw new Error("Settings panel is missing");
  configureSettingsSuspensions(root);
  root.addEventListener("click", event => {
    const target = event.target;
    if (!(target instanceof Element)) return;
    const tab = target.closest<HTMLButtonElement>(".settings-tab");
    if (tab) { if (!tab.disabled && isSettingsPaneAvailable(tab.dataset.settingsPane || "")) api.switchSettingsTab(tab.dataset.settingsPane || "", tab); return; }
    if (target.closest('[data-action="close-settings"]')) api.closeSettings();
  });
}

export function mountSettingsPanes(documentRoot: ParentNode, bridge: ProductBridge): () => void {
  const roots: Root[] = [];
  const client = new ProductClient(bridge);
  for (const pane of ["general", "appearance"] as const) {
    const target = documentRoot.querySelector<HTMLElement>(`#pane-${pane}`);
    if (!target) throw new Error(`Settings pane is missing: ${pane}`);
    const root = createRoot(target);
    root.render(<SettingsPane client={client} pane={pane} />);
    roots.push(root);
  }
  return () => roots.forEach(root => root.unmount());
}

function SettingsPane({ client, pane }: Props) {
  const [scope, setScope] = useState<SettingsScope>("global");
  const [snapshot, setSnapshot] = useState<SettingsSnapshot | null>(null);
  const [themes, setThemes] = useState(initialThemes);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [validation, setValidation] = useState("");

  useEffect(() => {
    let active = true;
    setLoading(true); setError("");
    client.getSettings(scope).then(value => {
      if (!active) return;
      setSnapshot(value);
      setThemes(value.themes);
      if (pane === "appearance") applyAppearance(value);
    }).catch(reason => { if (active) setError(errorText(reason)); })
      .finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [client, pane, scope]);

  useEffect(() => {
    if (snapshot) document.body.classList.toggle("hide-thinking", booleanValue(snapshot.settings.hideThinking));
  }, [snapshot]);

  useEffect(() => {
    if (pane !== "appearance" || !snapshot || snapshot.appearance.themeMode !== "system") return;
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const refresh = () => applyAppearance(snapshot);
    media.addEventListener("change", refresh);
    return () => media.removeEventListener("change", refresh);
  }, [pane, snapshot]);

  async function update(key: string, value: unknown): Promise<void> {
    if (saving) return;
    setSaving(true); setError(""); setValidation("");
    try {
      const next = await client.updateSetting(scope, key, value);
      setSnapshot(next); setThemes(next.themes);
      if (pane === "appearance") applyAppearance(next);
    } catch (reason) { setError(errorText(reason)); }
    finally { setSaving(false); }
  }

  async function reset(key: string): Promise<void> {
    if (saving) return;
    setSaving(true); setError("");
    try {
      const next = await client.resetSetting(scope, key);
      setSnapshot(next); setThemes(next.themes);
      if (pane === "appearance") applyAppearance(next);
    } catch (reason) { setError(errorText(reason)); }
    finally { setSaving(false); }
  }

  async function saveTheme(mode: "light" | "dark", field: keyof ThemeDefinition, value: string): Promise<void> {
    if (!snapshot || saving) return;
    const current = mode === "light" ? snapshot.appearance.lightTheme : snapshot.appearance.darkTheme;
    const suffix = mode === "light" ? "light" : "dark";
    const next: ThemeDefinition = { ...current, [field]: value };
    if (["accent", "background", "foreground"].includes(field) && !/^#[0-9A-Fa-f]{6}$/.test(value)) {
      setValidation("Enter a six digit hex color, such as #7D4F59."); return;
    }
    setValidation(""); setSaving(true); setError("");
    try {
      if (next.id === "rozsa" || next.id === "rozsa-dark") {
        next.id = mode === "light" ? "rozsa-custom" : "rozsa-dark-custom";
        next.name = mode === "light" ? "Rozsa Custom" : "Rozsa Dark Custom";
      }
      await client.saveTheme(next);
      await client.updateSetting(scope, `appearance_${suffix}_theme`, next.id);
      const refreshed = await client.getSettings(scope);
      setSnapshot(refreshed); setThemes(refreshed.themes);
      applyAppearance(refreshed);
    } catch (reason) { setError(errorText(reason)); }
    finally { setSaving(false); }
  }

  if (loading) return <Pane status="Loading settings…" pane={pane} scope={scope} setScope={setScope} />;
  if (error && !snapshot) return <Pane status="Settings could not be loaded." pane={pane} scope={scope} setScope={setScope} error={error} />;
  if (!snapshot) return <Pane status="No settings are available." pane={pane} scope={scope} setScope={setScope} />;
  const settings = snapshot.settings;
  return <>
    <header className="settings-pane-header">
      <div className="settings-pane-title" role="heading" aria-level={1}>{pane === "general" ? "General" : "Appearance"}</div>
      <label className="setting-scope-control">Save to <select aria-label="Settings scope" value={scope} disabled={saving} onChange={event => setScope(event.currentTarget.value as SettingsScope)}>
        <option value="global">Global</option><option value="project">Project</option>
      </select></label>
    </header>
    {error && <p className="settings-error" role="alert">{error}</p>}
    {validation && <p className="settings-error" role="alert">{validation}</p>}
    {pane === "general" ? <General settings={settings} models={snapshot.models} saving={saving} update={update} reset={reset} /> :
      <Appearance snapshot={snapshot} themes={themes} saving={saving} update={update} reset={reset} saveTheme={saveTheme} />}
  </>;
}

function Pane({ pane, scope, setScope, status, error = "" }: { pane: Props["pane"]; scope: SettingsScope; setScope(scope: SettingsScope): void; status: string; error?: string }) {
  return <>
    <header className="settings-pane-header"><div className="settings-pane-title" role="heading" aria-level={1}>{pane === "general" ? "General" : "Appearance"}</div>
      <label className="setting-scope-control">Save to <select aria-label="Settings scope" value={scope} onChange={event => setScope(event.currentTarget.value as SettingsScope)}><option value="global">Global</option><option value="project">Project</option></select></label>
    </header>
    <p role={error ? "alert" : "status"}>{error || status}</p>
  </>;
}

function General({ settings, models, saving, update, reset }: { settings: Settings; models: ModelOption[]; saving: boolean; update(key: string, value: unknown): Promise<void>; reset(key: string): Promise<void> }) {
  const compaction = object(settings.compaction);
  const retry = object(settings.retry);
  const provider = stringValue(settings.defaultProvider);
  const modelId = stringValue(settings.defaultModel);
  const selectedModel = models.find(model => model.provider === provider && model.id === modelId);
  const selectedModelKey = selectedModel ? `${selectedModel.provider}\u0000${selectedModel.id}` : "";
  const smallModelId = stringValue(settings.smallModel);
  return <>
    <Group title="Models">
      <ModelSetting label="Model" models={models} value={selectedModelKey} disabled={saving} onSave={value => {
        const [selectedProvider, selectedID] = value.split("\u0000");
        if (selectedProvider && selectedID) return update("model", { provider: selectedProvider, id: selectedID });
        return Promise.resolve();
      }} />
      <ModelSetting label="Small model" models={models} value={models.find(model => model.id === smallModelId)?.provider.concat("\u0000", smallModelId) || ""} disabled={saving} onSave={value => update("small_model", value ? value.split("\u0000")[1] : "")} onReset={() => reset("small_model")} />
      <StaticSetting label="Provider" value={selectedModel?.provider || provider || "—"} />
      <StaticSetting label="Context window" value={selectedModel ? selectedModel.contextWindow.toLocaleString() : "—"} />
      <SelectSetting label="Thinking effort" value={stringValue(settings.defaultThinkingEffort, "off")} disabled={saving} values={["off", "low", "medium", "high", "xhigh", "max"]} onSave={value => update("thinking", value)} onReset={() => reset("thinking")} />
    </Group>
    <Group title="Behavior">
      <ToggleSetting label="Auto compact" value={booleanValue(compaction.enabled)} disabled={saving} onSave={value => update("auto_compact", value)} onReset={() => reset("auto_compact")} />
      <NumberSetting label="Compaction trigger ratio" value={numberValue(compaction.triggerRatio, 0.9)} min={0.01} max={1} step={0.01} disabled={saving} onSave={value => update("compaction_trigger_ratio", value)} onReset={() => reset("compaction_trigger_ratio")} />
      <NumberSetting label="Compaction target ratio" value={numberValue(compaction.targetRatio, 0.7)} min={0.01} max={1} step={0.01} disabled={saving} onSave={value => update("compaction_target_ratio", value)} onReset={() => reset("compaction_target_ratio")} />
      <ToggleSetting label="Automatic session naming" value={booleanValue(settings.autoSessionNaming, true)} disabled={saving} onSave={value => update("auto_session_naming", value)} onReset={() => reset("auto_session_naming")} />
      <SelectSetting label="Steering mode" value={stringValue(settings.steeringMode, "one-at-a-time")} disabled={saving} values={["one-at-a-time", "all"]} onSave={value => update("steering_mode", value)} onReset={() => reset("steering_mode")} />
      <SelectSetting label="Follow-up mode" value={stringValue(settings.followUpMode, "one-at-a-time")} disabled={saving} values={["one-at-a-time", "all"]} onSave={value => update("follow_up_mode", value)} onReset={() => reset("follow_up_mode")} />
      <SelectSetting label="Messages while running" value={stringValue(settings.runningSendMode, "queue")} disabled={saving} values={["queue", "steer"]} onSave={value => update("running_send_mode", value)} onReset={() => reset("running_send_mode")} />
    </Group>
    <Group title="Network">
      <SelectSetting label="Transport" value={stringValue(settings.transport, "auto")} disabled={saving} values={["auto", "sse", "websocket", "websocket-cached"]} onSave={value => update("transport", value)} onReset={() => reset("transport")} />
      <OptionalNumber label="Retry timeout (ms)" value={retry.timeoutMs} disabled={saving} onSave={value => update("retry_timeout_ms", value)} onReset={() => reset("retry_timeout_ms")} />
      <OptionalNumber label="Retry count" value={retry.maxRetries} disabled={saving} onSave={value => update("retry_max_retries", value)} onReset={() => reset("retry_max_retries")} />
      <OptionalNumber label="Maximum retry delay (ms)" value={retry.maxRetryDelayMs} disabled={saving} onSave={value => update("retry_max_delay_ms", value)} onReset={() => reset("retry_max_delay_ms")} />
    </Group>
    <Group title="Content">
      <ToggleSetting label="Block images" value={booleanValue(settings.blockImages)} disabled={saving} onSave={value => update("block_images", value)} onReset={() => reset("block_images")} />
      <ToggleSetting label="Hide thinking blocks" value={booleanValue(settings.hideThinking)} disabled={saving} onSave={value => update("hide_thinking", value)} onReset={() => reset("hide_thinking")} />
    </Group>
  </>;
}

function Appearance({ snapshot, themes, saving, update, reset, saveTheme }: { snapshot: SettingsSnapshot; themes: ThemeSummary[]; saving: boolean; update(key: string, value: unknown): Promise<void>; reset(key: string): Promise<void>; saveTheme(mode: "light" | "dark", field: keyof ThemeDefinition, value: string): Promise<void> }) {
  const appearance = snapshot.appearance;
  const resolved = object(snapshot.settings.appearance);
  const selected = (mode: "light" | "dark") => stringValue(resolved[mode === "light" ? "lightTheme" : "darkTheme"], mode === "light" ? "rozsa" : "rozsa-dark");
  return <>
    <Group title="Theme mode">
      <div className="theme-mode-grid" role="group" aria-label="Theme mode">
        {(["system", "light", "dark"] as const).map(mode => <button key={mode} type="button" className={`theme-mode-card${appearance.themeMode === mode ? " active" : ""}`} aria-pressed={appearance.themeMode === mode} disabled={saving} onClick={() => update("appearance_theme_mode", mode)}><ThemeModePreview mode={mode} /><span>{mode[0]!.toUpperCase() + mode.slice(1)}</span></button>)}
      </div>
    </Group>
    <Group title="Display">
      <FontSizeSetting value={appearance.fontSize} disabled={saving} onSave={value => update("appearance_font_size", value)} onReset={() => reset("appearance_font_size")} />
      <ToggleSetting label="Translucent sidebar" value={appearance.translucentSidebar} disabled={saving || !navigator.userAgent.includes("Mac")} onSave={value => update("appearance_translucent_sidebar", value)} onReset={() => reset("appearance_translucent_sidebar")} />
      <ToggleSetting label="Show rate limits" value={booleanValue(resolved.showRateLimits, true)} disabled={saving} onSave={value => update("appearance_show_rate_limits", value)} onReset={() => reset("appearance_show_rate_limits")} />
      <ToggleSetting label="Show hourly limit" value={booleanValue(resolved.showHourlyRateLimit, true)} disabled={saving || !booleanValue(resolved.showRateLimits, true)} onSave={value => update("appearance_show_hourly_rate_limit", value)} onReset={() => reset("appearance_show_hourly_rate_limit")} />
      <ToggleSetting label="Show weekly limit" value={booleanValue(resolved.showWeeklyRateLimit, true)} disabled={saving || !booleanValue(resolved.showRateLimits, true)} onSave={value => update("appearance_show_weekly_rate_limit", value)} onReset={() => reset("appearance_show_weekly_rate_limit")} />
      <SelectSetting label="Rate limit display" value={stringValue(resolved.rateLimitDisplayMode, "remained")} disabled={saving || !booleanValue(resolved.showRateLimits, true)} values={["remained", "used"]} onSave={value => update("appearance_rate_limit_display_mode", value)} onReset={() => reset("appearance_rate_limit_display_mode")} />
    </Group>
    <ThemeEditor mode="light" theme={appearance.lightTheme} themes={themes} selected={selected("light")} saving={saving} update={update} onSave={saveTheme} />
    <ThemeEditor mode="dark" theme={appearance.darkTheme} themes={themes} selected={selected("dark")} saving={saving} update={update} onSave={saveTheme} />
  </>;
}

function ThemeEditor({ mode, theme, themes, selected, saving, update, onSave }: { mode: "light" | "dark"; theme: ThemeDefinition; themes: ThemeSummary[]; selected: string; saving: boolean; update(key: string, value: unknown): Promise<void>; onSave(mode: "light" | "dark", field: keyof ThemeDefinition, value: string): Promise<void> }) {
  const candidates = themes.filter(item => item.mode === mode);
  return <Group title={`${mode === "light" ? "Light" : "Dark"} theme`}>
    <label className="setting-item"><span className="setting-label">Theme</span><select className="setting-select" aria-label={`${mode} theme`} value={selected} disabled={saving} onChange={event => update(`appearance_${mode}_theme`, event.currentTarget.value)}>
      {!candidates.some(item => item.id === selected) && <option value={selected}>{selected}</option>}{candidates.map(item => <option key={item.id} value={item.id}>{item.name}</option>)}
    </select></label>
    <ColorSetting label="Accent" value={theme.accent} disabled={saving} onSave={value => onSave(mode, "accent", value)} />
    <ColorSetting label="Background" value={theme.background} disabled={saving} onSave={value => onSave(mode, "background", value)} />
    <ColorSetting label="Foreground" value={theme.foreground} disabled={saving} onSave={value => onSave(mode, "foreground", value)} />
    <TextSetting label="UI font" value={theme.uiFont} disabled={saving} onSave={value => onSave(mode, "uiFont", value)} />
    <TextSetting label="Code font" value={theme.codeFont} disabled={saving} onSave={value => onSave(mode, "codeFont", value)} />
  </Group>;
}

function Group({ title, children }: { title: string; children: ReactNode }) { return <section className="settings-group"><h2 className="settings-group-label">{title}</h2><div className="settings-module">{children}</div></section>; }
function ThemeModePreview({ mode }: { mode: "system" | "light" | "dark" }) {
  if (mode === "system") return <div className="theme-preview theme-preview-system"><div className="theme-preview-top" /><div className="theme-preview-system-body"><div className="theme-preview-half light"><span /><span /></div><div className="theme-preview-half dark"><span /><span /></div></div></div>;
  return <div className="theme-preview"><div className="theme-preview-top" /><div className={`theme-preview-body ${mode}`}><span /><span /></div></div>;
}
function FontSizeSetting({ value, disabled, onSave, onReset }: { value: number; disabled: boolean; onSave(value: number): Promise<void>; onReset(): Promise<void> }) {
  const [draft, setDraft] = useState(String(value));
  useEffect(() => setDraft(String(value)), [value]);
  const parsed = Number(draft);
  const valid = draft !== "" && Number.isInteger(parsed) && parsed >= 5 && parsed <= 30;
  const preview = (next: number) => {
    setDraft(String(next));
    document.documentElement.style.setProperty("--ui-font-size", `${next}px`);
    document.documentElement.style.setProperty("--ui-scale", String(next / 14));
  };
  const commit = () => { if (valid && parsed !== value) void onSave(parsed); else if (!valid) setDraft(String(value)); };
  const progress = valid ? `${((parsed - 5) / 25) * 100}%` : `${((value - 5) / 25) * 100}%`;
  return <div className="setting-item"><span className="setting-label">Font Size</span><span className="setting-range-control">
    <input className="appearance-range" type="range" min="5" max="30" step="1" aria-label="Font Size" value={valid ? parsed : value} style={{ "--appearance-progress": progress } as React.CSSProperties} disabled={disabled} onChange={event => preview(Number(event.currentTarget.value))} onMouseUp={commit} onKeyUp={commit} onPointerUp={commit} />
    <input className="setting-input appearance-font-size-input" type="number" min="5" max="30" step="1" aria-label="Font size (px)" value={draft} disabled={disabled} onChange={event => setDraft(event.currentTarget.value)} onBlur={commit} />
    <span className="setting-value">px</span>
  </span><button type="button" className="setting-reset" disabled={disabled} onClick={onReset}>Reset</button></div>;
}
function StaticSetting({ label, value }: { label: string; value: string }) { return <div className="setting-item"><span className="setting-label">{label}</span><span className="setting-value">{value || "—"}</span></div>; }
function ToggleSetting({ label, value, disabled, onSave, onReset }: { label: string; value: boolean; disabled: boolean; onSave(value: boolean): Promise<void>; onReset?(): Promise<void> }) {
  return <div className={`setting-item${disabled ? " is-disabled" : ""}`}><span className="setting-label">{label}</span><button type="button" className={`setting-toggle${value ? " on" : ""}`} role="switch" aria-label={label} aria-checked={value} disabled={disabled} onClick={() => onSave(!value)} />{onReset && <button type="button" className="setting-reset" disabled={disabled} onClick={onReset}>Reset</button>}</div>;
}
function SelectSetting({ label, value, values, disabled, onSave, onReset }: { label: string; value: string; values: string[]; disabled: boolean; onSave(value: string): Promise<void>; onReset?(): Promise<void> }) {
  return <label className="setting-item"><span className="setting-label">{label}</span><select className="setting-select" aria-label={label} value={value} disabled={disabled} onChange={event => onSave(event.currentTarget.value)}>{values.map(item => <option key={item} value={item}>{optionLabel(item)}</option>)}</select>{onReset && <button type="button" className="setting-reset" disabled={disabled} onClick={onReset}>Reset</button>}</label>;
}
function ModelSetting({ label, models, value, disabled, onSave, onReset }: { label: string; models: ModelOption[]; value: string; disabled: boolean; onSave(value: string): Promise<void>; onReset?(): Promise<void> }) {
  return <label className="setting-item"><span className="setting-label">{label}</span><select className="setting-select" aria-label={label} value={value} disabled={disabled || models.length === 0} onChange={event => onSave(event.currentTarget.value)}>
    <option value="">{models.length ? "Choose a model" : "No configured models"}</option>
    {models.map(model => <option key={`${model.provider}/${model.id}`} value={`${model.provider}\u0000${model.id}`}>{model.name} · {model.provider}</option>)}
  </select>{onReset && <button type="button" className="setting-reset" disabled={disabled} onClick={onReset}>Reset</button>}</label>;
}
function optionLabel(value: string): string {
  return ({ "one-at-a-time": "One at a time", all: "All", queue: "Queue", steer: "Steer", auto: "Auto", sse: "SSE", websocket: "WebSocket", "websocket-cached": "WebSocket (cached)", remained: "Remaining", used: "Used", xhigh: "Xhigh" } as Record<string, string>)[value] || value[0]?.toUpperCase() + value.slice(1);
}
function TextSetting({ label, value, disabled, onSave, onReset }: { label: string; value: string; disabled: boolean; onSave(value: string): Promise<void>; onReset?(): Promise<void> }) {
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);
  return <label className="setting-item"><span className="setting-label">{label}</span><input className="setting-input" aria-label={label} value={draft} disabled={disabled} onChange={event => setDraft(event.currentTarget.value)} onBlur={() => { if (draft !== value) void onSave(draft); }} onKeyDown={event => { if (event.key === "Enter") event.currentTarget.blur(); }} />{onReset && <button type="button" className="setting-reset" disabled={disabled} onClick={onReset}>Reset</button>}</label>;
}
function ColorSetting({ label, value, disabled, onSave }: { label: string; value: string; disabled: boolean; onSave(value: string): Promise<void> }) {
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);
  const valid = /^#[0-9A-Fa-f]{6}$/.test(draft);
  return <div className="setting-item"><span className="setting-label">{label}</span><span className="theme-color-control">
    <input className="theme-color-picker" type="color" aria-label={`${label} color picker`} value={valid ? draft : value} disabled={disabled} onChange={event => { setDraft(event.currentTarget.value.toUpperCase()); void onSave(event.currentTarget.value.toUpperCase()); }} />
    <input className="theme-hex-input" aria-label={label} value={draft} disabled={disabled} onChange={event => setDraft(event.currentTarget.value)} onBlur={() => { if (draft !== value) void onSave(draft); }} />
  </span></div>;
}
function NumberSetting({ label, value, min, max, step, disabled, onSave, onReset }: { label: string; value: number; min: number; max: number; step: number; disabled: boolean; onSave(value: number): Promise<void>; onReset?(): Promise<void> }) {
  const [draft, setDraft] = useState(String(value));
  useEffect(() => setDraft(String(value)), [value]);
  return <label className="setting-item"><span className="setting-label">{label}</span><input className="setting-input" type="number" aria-label={label} min={min} max={max} step={step} value={draft} disabled={disabled} onChange={event => setDraft(event.currentTarget.value)} onBlur={() => {
    const parsed = Number(draft);
    if (draft === "" || !Number.isFinite(parsed) || parsed < min || parsed > max) { setDraft(String(value)); return; }
    if (parsed !== value) void onSave(parsed);
  }} />{onReset && <button type="button" className="setting-reset" disabled={disabled} onClick={onReset}>Reset</button>}</label>;
}
function OptionalNumber({ label, value, disabled, onSave, onReset }: { label: string; value: unknown; disabled: boolean; onSave(value: number | null): Promise<void>; onReset(): Promise<void> }) {
  const [draft, setDraft] = useState(value == null ? "" : String(value));
  useEffect(() => setDraft(value == null ? "" : String(value)), [value]);
  return <label className="setting-item"><span className="setting-label">{label}</span><input className="setting-input" type="number" min={0} aria-label={label} value={draft} disabled={disabled} onChange={event => setDraft(event.currentTarget.value)} onBlur={() => {
    if (!draft.trim()) { if (value != null) void onSave(null); return; }
    const parsed = Number(draft);
    if (!Number.isSafeInteger(parsed) || parsed < 0) { setDraft(value == null ? "" : String(value)); return; }
    if (parsed !== value) void onSave(parsed);
  }} /> <button type="button" className="setting-reset" disabled={disabled} onClick={onReset}>Reset</button></label>;
}
function stringValue(value: unknown, fallback = ""): string { return typeof value === "string" ? value : fallback; }
function numberValue(value: unknown, fallback: number): number { return typeof value === "number" ? value : fallback; }
function booleanValue(value: unknown, fallback = false): boolean { return typeof value === "boolean" ? value : fallback; }
function object(value: unknown): Record<string, unknown> { return typeof value === "object" && value !== null && !Array.isArray(value) ? value as Record<string, unknown> : {}; }
function errorText(error: unknown): string { return error instanceof Error ? error.message : String(error); }
function applyAppearance(snapshot: SettingsSnapshot): void {
  const root = document.documentElement;
  const mode = snapshot.appearance.themeMode;
  const effective = mode === "system" ? (matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light") : mode;
  const theme = effective === "light" ? snapshot.appearance.lightTheme : snapshot.appearance.darkTheme;
  const nextVariables = new Set(Object.keys(theme.variables));
  for (const name of appliedThemeVariables) if (!nextVariables.has(name)) root.style.removeProperty(name);
  appliedThemeVariables = nextVariables;
  root.setAttribute("data-theme-mode", effective);
  root.setAttribute("data-theme-id", theme.id);
  root.setAttribute("data-theme-translucent-sidebar", snapshot.appearance.translucentSidebar && navigator.userAgent.includes("Mac") ? "true" : "false");
  for (const [name, value] of Object.entries(theme.variables)) root.style.setProperty(name, value);
  root.style.setProperty("--accent", theme.accent); root.style.setProperty("--semantic-accent", theme.accent);
  root.style.setProperty("--bg", theme.background); root.style.setProperty("--fg", theme.foreground);
  root.style.setProperty("--font-ui", theme.uiFont); root.style.setProperty("--font-mono", theme.codeFont);
  root.style.setProperty("--sidebar-bg", theme.variables["--sidebar-bg"] || theme.background);
  root.style.setProperty("--titlebar-bg", theme.variables["--titlebar-bg"] || theme.background);
  root.style.setProperty("--ui-font-size", `${snapshot.appearance.fontSize}px`);
  root.style.setProperty("--ui-scale", String(snapshot.appearance.fontSize / 14));
}
