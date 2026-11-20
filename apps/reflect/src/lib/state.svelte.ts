// App state: which page is open, the data for it (loaded on demand and
// refreshed every minute while visible), settings, theme.
import { on, showToast } from "@selfkit/ui";
import { api, type Conn, type Health, type Overview, type RSettings, type Review, type Shared, type Sources, type StreakView, type Theme } from "./api";

export type Tab = "today" | "streaks" | "connections" | "week" | "month" | "health" | "settings" | "sync" | "conflicts";

export const app = $state({
  tab: "today" as Tab,
  /** Where Health/Settings return to on a phone. */
  prevTab: "today" as Tab,
  overview: null as Overview | null,
  streaks: null as StreakView[] | null,
  conns: null as { items: Conn[]; lookback_days: number; sym: string } | null,
  review: { week: null, month: null } as Record<"week" | "month", Review | null>,
  /** 0 = the current week/month (opens there; QA 2026-11-15), -1 = the one before. */
  offset: { week: 0, month: 0 } as Record<"week" | "month", number>,
  /** Phone: the Review tab shows the week or the month. */
  reviewKind: "week" as "week" | "month",
  health: null as Health | null,
  settings: null as RSettings | null,
  sources: null as Sources | null,
  shared: null as Shared | null,
  theme: "system" as Theme,
  error: "",
  demo: {} as Record<string, string>,
  /** Open details on a connection card (id). */
  open: {} as Record<string, boolean>,
  /** Settings: the "new streak" chooser is open. */
  adding: false,
});

type Bridge = { setDark(d: boolean): void };
export const bridge = () => (window as unknown as { ReflectBars?: Bridge }).ReflectBars;

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

/** Load what the current page shows (and the health dot). */
export async function load(tab: Tab = app.tab) {
  switch (tab) {
    case "today":
      app.overview = (await guard(api.overview())) ?? app.overview;
      break;
    case "streaks":
      app.streaks = (await guard(api.streaks())) ?? app.streaks;
      break;
    case "connections":
      app.conns = (await guard(api.connections())) ?? app.conns;
      break;
    case "week":
    case "month":
      app.review[tab] = (await guard(api.review(tab, app.offset[tab]))) ?? app.review[tab];
      break;
    case "settings":
      app.settings = (await guard(api.settings())) ?? app.settings;
      app.sources = (await guard(api.sources())) ?? app.sources;
      app.shared = (await guard(api.shared())) ?? app.shared;
      break;
  }
  app.health = (await guard(api.health())) ?? app.health;
}

export function openTab(t: Tab) {
  if (t === app.tab) return;
  const side = (x: Tab) => x === "health" || x === "settings" || x === "sync" || x === "conflicts";
  if (side(t)) {
    if (!side(app.tab)) app.prevTab = app.tab;
  }
  if (t === "week" || t === "month") app.reviewKind = t;
  app.tab = t;
  load(t);
}

export function stepReview(kind: "week" | "month", d: number) {
  app.offset[kind] = Math.min(0, app.offset[kind] + d);
  load(kind);
}

/** Health status for the dot next to Health: "ok" | "warn" | "error" | "none". */
export function healthLevel(h: Health | null): "ok" | "warn" | "error" | "none" {
  if (!h || !h.found) return "none";
  if (!h.running) return "error";
  const d = h.doc;
  if (d?.tracking?.error || d?.sync?.last_error || d?.backup?.last_error || d?.lan?.error) return "error";
  if (d?.sync?.server_db_warn) return "warn";
  return "ok";
}

export async function saveSettings(s: RSettings, undoText?: string, before?: RSettings) {
  app.settings = s;
  const saved = await guard(api.settingsSet(s));
  if (saved) app.settings = saved;
  if (undoText && before) showToast(undoText, () => saveSettings(before));
  // Streak/review numbers depend on settings.
  app.streaks = null;
  app.review = { week: null, month: null };
  app.conns = null;
}

// ---- theme ----------------------------------------------------------------

const darkQuery = window.matchMedia("(prefers-color-scheme: dark)");
export function applyTheme() {
  const t = app.theme;
  if (t === "system") delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = t;
  bridge()?.setDark(t === "dark" || (t === "system" && darkQuery.matches));
}
darkQuery.addEventListener("change", () => applyTheme());

export async function setTheme(t: Theme) {
  app.theme = t;
  applyTheme();
  await guard(api.prefsSet({ theme: t }));
}

export async function agent(cmd: "sync_now" | "backup_now") {
  // The agent starts the job and answers at once; health.json shows the result.
  try {
    const r = await api.agentCmd(cmd);
    if (r?.ok === false) showToast(String(r?.error ?? "The agent said no"));
    else showToast(cmd === "sync_now" ? "Sync started" : "Backup started");
  } catch (e) {
    showToast(String(e));
  }
  for (const ms of [3000, 15000, 65000]) setTimeout(() => load("health"), ms);
}

export async function init() {
  const p = await guard(api.prefs());
  if (p) app.theme = p.theme ?? "system";
  applyTheme();
  await load("today");
  // Android: refresh the read-only copies of Chrono/Hisab/Agenda/Dusk from their
  // providers (android-plugins/selfkit-sync SelfkitMirror), then reload.
  const mirror = () => {
    const b = (window as unknown as { SelfkitSync?: { mirror(apps: string): string } }).SelfkitSync;
    if (!b) return;
    // Also re-read which apps are present: the first mirror creates the copies, and
    // Today's "isn't installed" tiles use `sources` (stale on the first launch before).
    setTimeout(async () => {
      try { b.mirror("chrono,hisab,agenda,dusk"); } catch { /* not available */ }
      app.sources = (await guard(api.sources())) ?? app.sources;
      load();
    }, 50);
  };
  mirror();
  on("data-changed", () => load()); // a sync pass brought new data
  setInterval(() => document.visibilityState === "visible" && load(), 60_000);
  document.addEventListener("visibilitychange", () => { if (document.visibilityState === "visible") { load(); mirror(); } });
}
