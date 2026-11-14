// App state: the dashboard (last 90 nights + stats), the night on screen,
// rating/corrections with undo, prefs, and the Android usage import.
import { on, showToast } from "@selfkit/ui";
import { isPermissionGranted, requestPermission } from "@tauri-apps/plugin-notification";
import { api, type Connection, type Dashboard, type DevicePrefs, type Goals, type LogPatch, type Night, type Settings, type Sources } from "./api";
import { QUALITY, addDays, dayShort, dur } from "./fmt";

export type Tab = "night" | "trends" | "connections" | "settings";
export type Focus = "bed" | "wake" | "note_before" | "note_after";
/** Adjust times / add nap (phone: a page; desktop: a side panel). */
export type Edit = { kind: "adjust" | "nap"; night: string; focus?: Focus };

export const app = $state({
  dash: null as Dashboard | null,
  /** Night key on screen (null = the current one). */
  shown: null as string | null,
  /** Detail for a night outside the dashboard range. */
  extra: null as Night | null,
  tab: "night" as Tab,
  connections: null as Connection[] | null,
  prefs: { winddown_on: false, theme: "system", import_cursor: 0 } as DevicePrefs,
  settings: null as Settings | null,
  sources: null as Sources | null,
  /** Android: Usage access granted (null = not Android). */
  usageAccess: null as boolean | null,
  /** Where the settings page returns to (phone). */
  prevTab: "night" as Tab,
  edit: null as Edit | null,
  error: "",
  demo: {} as Record<string, string>,
  /** Set by an open sheet: returns true when it handled a close request (asked to discard). */
  closeGuard: null as null | (() => boolean),
});

type Bridge = { hasAccess(): boolean; openAccessSettings(): void; events(from: number, to: number): string; setDark(d: boolean): void };
export const bridge = () => (window as unknown as { DuskUsage?: Bridge }).DuskUsage;

export const currentKey = () => app.dash?.current ?? "";
export const shownKey = () => app.shown ?? currentKey();
export function nightByKey(k: string): Night | undefined {
  return app.dash?.nights.find((n) => n.night === k) ?? (app.extra?.night === k ? app.extra : undefined);
}
export const shownNight = () => nightByKey(shownKey());

async function guard<T>(p: Promise<T>): Promise<T | undefined> {
  try {
    const v = await p;
    app.error = "";
    return v;
  } catch (e) {
    app.error = String(e);
    console.error(e);
    return undefined;
  }
}

export async function refresh() {
  const d = await guard(api.dashboard());
  if (d) app.dash = d;
  const k = shownKey();
  if (k && !app.dash?.nights.some((n) => n.night === k)) app.extra = (await guard(api.night(k))) ?? null;
  else if (k) {
    // The dashboard trims activity for older nights; fetch full detail for the one on screen.
    const n = nightByKey(k);
    if (n && !n.estimate.activity.length && n.estimate.status !== "no_data") app.extra = (await guard(api.night(k))) ?? null;
  }
  if (app.tab === "connections") loadConnections();
}

export function openTab(t: Tab) {
  if (t === app.tab) return;
  if (t === "settings") app.prevTab = app.tab === "settings" ? "night" : app.tab;
  app.tab = t;
  if (t === "connections") loadConnections();
}

export async function loadConnections() {
  app.connections = (await guard(api.connections())) ?? app.connections;
}

export async function show(k: string) {
  app.shown = k === currentKey() ? null : k;
  app.extra = null;
  const n = nightByKey(k);
  if (!n || !n.estimate.activity.length) app.extra = (await guard(api.night(k))) ?? null;
}
export function step(by: number) {
  const next = addDays(shownKey(), by);
  if (next > currentKey()) return;
  show(next);
}

function replaceNight(n: Night) {
  if (app.dash) {
    const i = app.dash.nights.findIndex((x) => x.night === n.night);
    if (i >= 0) app.dash.nights[i] = n;
  }
  if (app.extra?.night === n.night || !app.dash?.nights.some((x) => x.night === n.night)) app.extra = n;
}

/** Change a night's entry; toast with undo. Throws a readable message for forms. */
export async function setLog(night: string, patch: LogPatch, toast: string) {
  const before = nightByKey(night)?.log ?? null;
  const n = await api.logSet(night, patch);
  replaceNight(n);
  showToast(toast, async () => {
    await api.logReplace(night, before);
    await refresh();
  });
  refresh();
  return n;
}

export async function rate(night: string, q: number) {
  // Tapping the selected rating again removes it (undo in the toast).
  const n = nightByKey(night);
  if (n?.quality === q) return void (await guard(setLog(night, { quality: null }, "Rating removed")));
  await guard(setLog(night, { quality: q }, `Rated ${q} · ${QUALITY[q]}`));
}

export async function resetToEstimate(night: string) {
  await guard(setLog(night, { bed: null, wake: null }, "Back to the estimate"));
}

export async function addNap(night: string, start: string, end: string) {
  const l = await api.napAdd(night, start, end);
  showToast(`Nap added · ${dur(((l.wake ?? 0) - (l.bed ?? 0)) / 60000)}`, async () => {
    await api.napDelete(l.id);
    await refresh();
  });
  await refresh();
}

export async function deleteNap(id: string, night: string) {
  await guard(api.napDelete(id));
  showToast(`Nap removed · ${dayShort(night)}`, async () => {
    await api.napRestore(id);
    await refresh();
  });
  await refresh();
}

export async function saveGoals(g: Goals) {
  const saved = await api.goalsSet(g);
  if (app.dash) app.dash.goals = saved;
  refresh();
  return saved;
}

// ---- prefs, theme --------------------------------------------------------

const darkQuery = window.matchMedia("(prefers-color-scheme: dark)");
export function applyTheme() {
  const t = app.prefs.theme ?? "system";
  if (t === "system") delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = t;
  bridge()?.setDark(t === "dark" || (t === "system" && darkQuery.matches));
}
darkQuery.addEventListener("change", () => applyTheme());

export async function savePrefs(p: DevicePrefs) {
  if (p.winddown_on && !app.prefs.winddown_on) {
    try {
      if (!(await isPermissionGranted())) await requestPermission();
    } catch (e) {
      console.warn("notification permission", e);
    }
  }
  app.prefs = p;
  applyTheme();
  await guard(api.deviceSet(p));
}

export async function saveSettings(s: Settings) {
  app.settings = s;
  await guard(api.settingsSet(s));
  refresh();
}

// ---- Android usage import ------------------------------------------------

const DAY = 86_400_000;
let importing = false;
/** Read screen events from UsageStatsManager (Kotlin bridge) and store them as phone intervals. */
export async function importUsage() {
  const b = bridge();
  if (!b) return;
  app.usageAccess = b.hasAccess();
  if (!app.usageAccess || importing) return;
  importing = true;
  try {
    const now = Date.now();
    const cursor = app.prefs.import_cursor || 0;
    const from = Math.max(cursor - DAY, now - 10 * DAY);
    const events = JSON.parse(b.events(from, now)) as [number, number][];
    await api.androidImport(events, from, now);
    app.prefs.import_cursor = now;
  } catch (e) {
    console.error("usage import", e);
  } finally {
    importing = false;
  }
}

export async function init() {
  const p = await guard(api.device());
  if (p) app.prefs = p;
  applyTheme();
  guard(api.settings()).then((s) => s && (app.settings = s));
  guard(api.sources()).then((s) => s && (app.sources = s));
  await importUsage();
  await refresh();
  // A damaged data file was set aside at startup: say so once.
  guard(api.startupNotice()).then((n) => n && showToast(n, undefined, 15000));
  on("sleep-changed", () => refresh());
  on<DevicePrefs>("prefs-changed", (p) => ((app.prefs = { ...app.prefs, ...p }), applyTheme()));
  document.addEventListener("visibilitychange", async () => {
    if (document.visibilityState === "visible") {
      await importUsage();
      refresh();
    }
  });
  setInterval(refresh, 5 * 60_000);
}
