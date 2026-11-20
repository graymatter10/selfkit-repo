// App state: current screen, selected day, overlays, settings, refresh ticks.
import { on, showToast } from "@selfkit/ui";
import { api, type AndroidStatus, type ChronoSettings, type Info, type ThemeMode } from "./api";
import { dur } from "./fmt";

export type View = "today" | "timeline" | "trends" | "sites" | "report";

export const app = $state({
  view: "today" as View,
  /** selected day for Today / Timeline (null = today) */
  day: null as string | null,
  /** Settings opens at "Goals and limits" (the Today card's Edit/Add — P22). */
  goalsFocus: false,
  /** selected week (any day in it) for the report (null = last complete week) */
  week: null as string | null,
  info: null as Info | null,
  settings: null as ChronoSettings | null,
  android: { supported: false } as AndroidStatus,
  theme: "system" as ThemeMode,
  /** bumps whenever data may have changed; screens reload on it */
  rev: 0,
  now: Date.now(),
  settingsOpen: false,
  sourcesOpen: false,
  changesOpen: false,
  /** Settings → Sync page and the conflict screen (shared ui/src/sync). */
  syncOpen: false,
  conflictsOpen: false,
  /** Websites: period, anchor day (null = today), and the loaded title / neighbours */
  web: { period: "week" as "day" | "week" | "month", anchor: null as string | null, title: "", prev: null as string | null, next: null as string | null },
  /** Websites: open site (detail page on phone, right pane on desktop) */
  site: null as string | null,
  /** Time-rule form: a rule being added/edited (Websites detail or Settings → Categories) */
  rule: null as null | { rule: import("./api").CategoryRule; isNew: boolean; base?: string },
  /** Android first-run setup step (0 = closed) */
  setup: 0,
  noteDay: null as string | null,
  photo: null as string | null,
  pauseMenu: false,
  error: "",
  /** Test hook (CHRONO_DEMO) for screenshots; empty in normal use. */
  demo: {} as Record<string, string>,
  /** Set by an open editor: returns true when it handled a close request. */
  closeGuard: null as null | (() => boolean),
});

export const today = () => app.info?.today ?? new Date().toISOString().slice(0, 10);
export const isPaused = () => !!app.settings?.paused_until && app.settings.paused_until > app.now;
export const desktopPlatform = () => app.info?.platform !== "android";

export async function guard<T>(p: Promise<T>): Promise<T | undefined> {
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

export function bump() {
  app.now = Date.now();
  app.rev++;
}

const darkQuery = window.matchMedia("(prefers-color-scheme: dark)");
export function applyTheme() {
  const dark = app.theme === "dark" || (app.theme === "system" && darkQuery.matches);
  document.documentElement.classList.toggle("dark", dark);
  // Android: status-bar icon colour follows the app theme (MainActivity.kt)
  (window as unknown as { ChronoBars?: { setDark(d: boolean): void } }).ChronoBars?.setDark(dark);
}
darkQuery.addEventListener("change", applyTheme);

export async function setTheme(t: ThemeMode) {
  app.theme = t;
  applyTheme();
  await guard(api.setTheme(t));
}

export async function saveSettings(s: ChronoSettings, undoText?: string, before?: ChronoSettings) {
  app.settings = s;
  await guard(api.saveSettings($state.snapshot(s) as ChronoSettings));
  if (undoText && before) showToast(undoText, async () => saveSettings(before));
  bump();
}

export function pauseLabel(): string {
  const t = app.settings?.paused_until;
  if (!t || t <= app.now) return "";
  if (t > app.now + 300 * 86400000) return "Paused until you resume";
  const d = new Date(t), n = new Date(app.now);
  const hhmm = `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
  return d.toDateString() === n.toDateString() ? `Paused until ${hhmm}` : `Paused until tomorrow ${hhmm}`;
}

export async function pause(minutes: number | null | "tomorrow") {
  app.pauseMenu = false;
  let m = minutes;
  if (m === "tomorrow") {
    const t = new Date();
    t.setDate(t.getDate() + 1);
    t.setHours(Math.floor((app.info?.day_start ?? 0) / 60), (app.info?.day_start ?? 0) % 60, 0, 0);
    m = Math.max(1, Math.round((t.getTime() - Date.now()) / 60000));
  }
  const s = await guard(api.pause(m));
  if (s) {
    app.settings = s;
    app.now = Date.now();
    showToast(m == null ? "Paused until you resume" : `Paused for ${dur((m as number) * 60000)}`, resume);
  }
}

export async function resume() {
  app.pauseMenu = false;
  const s = await guard(api.resume());
  if (s) {
    app.settings = s;
    app.now = Date.now();
    showToast("Recording again");
  }
}

export async function refreshAndroid() {
  if (app.info?.platform !== "android") return;
  const st = await guard(api.androidStatus());
  if (st) app.android = st;
  if (st?.usage) {
    await guard(api.androidImport());
    const st2 = await guard(api.androidStatus());
    if (st2) app.android = st2;
  }
}

let ready: () => void;
export const initDone = new Promise<void>((r) => (ready = r));

export async function init() {
  app.info = (await guard(api.info())) ?? null;
  // A damaged data file was set aside at startup: say so once (L25).
  guard(api.startupNotice()).then((n) => n && showToast(n, undefined, 15000));
  app.settings = (await guard(api.settings())) ?? null;
  app.theme = (await guard(api.theme())) ?? "system";
  applyTheme();
  if (app.info?.platform === "android") {
    await refreshAndroid();
    const prefs = (await guard(api.prefs())) ?? {};
    if (!app.android.usage && !prefs.setup_done) app.setup = 1;
  }
  bump();
  ready();
  on("data-changed", () => bump());
  on("tick", async () => {
    app.info = (await guard(api.info())) ?? app.info;
    bump();
  });
  setInterval(() => (app.now = Date.now()), 30_000);
  document.addEventListener("visibilitychange", async () => {
    if (document.visibilityState !== "visible") return;
    app.info = (await guard(api.info())) ?? app.info;
    await refreshAndroid();
    bump();
  });
}
