// App state: snapshot from Rust, navigation (page stack + sheet stack),
// prefs, the add-screen draft (persisted so nothing typed is ever lost),
// and save/delete helpers with undo toasts.
import { on, showToast } from "@selfkit/ui";
import { api, type Account, type Category, type Coll, type Prefs, type Snapshot, type Tx } from "./api";
import { monthIso, todayIso } from "./fmt";

export type Page = "home" | "history" | "insights" | "accounts" | "loans" | "goals" | "recurring" | "categories" | "settings" | "more" | "sync" | "conflicts";
export type Sheet =
  | { kind: "tx"; id: string }
  | { kind: "account"; id?: string }
  | { kind: "transfer"; from?: string }
  | { kind: "category"; id?: string; type?: "expense" | "income"; pick?: boolean }
  | { kind: "loan"; id?: string; edit?: boolean }
  | { kind: "goal"; id?: string }
  | { kind: "recurring"; id?: string }
  | { kind: "export" }
  | { kind: "import" }
  | { kind: "currency"; value: string; onpick: (c: string) => void }
  | { kind: "split" }
  | { kind: "photo"; id: string };

export interface Draft {
  kind: "expense" | "income" | "transfer";
  amount: string;
  currency: string;
  account: string;
  to_account: string;
  category: string;
  splits: { category: string; amount: string }[];
  note: string;
  tags: string[];
  photos: string[];
  date: string;
  time: string;
}

const DRAFT_KEY = "hisab.draft";

export function emptyDraft(): Draft {
  // date "" = today at the moment of saving (a fixed date went stale overnight:
  // an entry typed the next morning was silently saved on the previous day).
  return { kind: "expense", amount: "", currency: "", account: "", to_account: "", category: "", splits: [], note: "", tags: [], photos: [], date: "", time: "", };
}

/** An untouched draft (no amount) follows today (older versions stored today's
 *  date in it) and the main currency: a currency picked by hand lasts for that
 *  one entry only, so a restart never brings back an old "USD" (QA 2026-11-15). */
function freshDate(d: Draft): Draft {
  return d.amount ? d : { ...d, date: "", currency: "" };
}

function loadDraft(): Draft {
  try {
    const d = JSON.parse(localStorage.getItem(DRAFT_KEY) || "null");
    if (d && typeof d === "object") return freshDate({ ...emptyDraft(), ...d });
  } catch {}
  return emptyDraft();
}

export const app = $state({
  ready: false,
  snap: null as Snapshot | null,
  month: monthIso(),
  pages: ["home"] as Page[],
  sheets: [] as Sheet[],
  prefs: {} as Prefs,
  draft: loadDraft(),
  error: "",
  demo: {} as Record<string, string>,
  /** Set by an open form with unsaved input; returns true when it handled Back/Escape itself. */
  guard: null as null | (() => boolean),
  rev: 0,
});

export const page = () => app.pages[app.pages.length - 1];
export const go = (p: Page) => {
  if (p === "home") app.pages = ["home"];
  else if (page() !== p) app.pages = [...app.pages.filter((x) => x !== p), p];
};
export const back = () => {
  if (app.pages.length > 1) app.pages = app.pages.slice(0, -1);
};
export const openSheet = (s: Sheet) => (app.sheets = [...app.sheets, s]);
export const closeSheet = () => (app.sheets = app.sheets.slice(0, -1));

let saveTimer: ReturnType<typeof setTimeout>;
function writeDraft() {
  const s = JSON.stringify(app.draft);
  try {
    localStorage.setItem(DRAFT_KEY, s);
  } catch {}
  if (app.ready) api.draftSet(JSON.parse(s)).catch(() => {});
}
export function persistDraft() {
  clearTimeout(saveTimer);
  saveTimer = setTimeout(writeDraft, 120);
}
// Leaving the app (Back, home, task switch): write now, not after the delay.
document.addEventListener("visibilitychange", () => document.hidden && (clearTimeout(saveTimer), writeDraft()));
window.addEventListener("pagehide", () => (clearTimeout(saveTimer), writeDraft()));
export function resetDraft(keep: Partial<Draft> = {}) {
  app.draft = { ...emptyDraft(), ...keep };
  writeDraft();
}

// Global main currency (shared setting, read by Rust). Before the first
// snapshot, or if one fails, the last known value is used, never a silent USD
// (USD is only the shared first-start default, when nothing was ever stored).
const MAIN_KEY = "hisab.main";
let lastMain = (() => {
  try {
    return localStorage.getItem(MAIN_KEY) || "";
  } catch {
    return "";
  }
})();
export const main = () => app.snap?.main || lastMain || "USD";
export const accounts = () => (app.snap?.accounts ?? []).filter((a) => !a.archived);
export const accountById = (id: string): Account | undefined => app.snap?.accounts.find((a) => a.id === id);
export const catById = (id: string): Category | undefined => app.snap?.categories.find((c) => c.id === id);
export const txCurrency = (t: Pick<Tx, "currency">) => t.currency || main();

/** Active categories of a kind in the chosen ranking. */
export function rankedCats(kind: "expense" | "income"): Category[] {
  const list = (app.snap?.categories ?? []).filter((c) => c.kind === kind && !c.archived);
  if (app.prefs.rank === "used") return [...list].sort((a, b) => b.usage - a.usage || a.order - b.order);
  return list;
}

export function defaultAccount(): string {
  const list = accounts();
  return list.find((a) => a.id === app.prefs.account)?.id ?? list[0]?.id ?? "";
}

export async function refresh() {
  try {
    app.snap = await api.snapshot(app.month);
    if (app.snap.main && app.snap.main !== lastMain) {
      lastMain = app.snap.main;
      try {
        localStorage.setItem(MAIN_KEY, lastMain);
      } catch {}
    }
    app.rev++;
    app.error = "";
  } catch (e) {
    app.error = String(e);
  }
}

let started = false;
export async function init() {
  if (started) return;
  started = true;
  try {
    const b = await api.boot();
    app.prefs = b.prefs ?? {};
    if (b.draft && typeof b.draft === "object") app.draft = freshDate({ ...emptyDraft(), ...b.draft });
    applyTheme();
    await refresh();
    app.ready = true;
    if (b.generated) showToast(`Added ${b.generated} recurring ${b.generated === 1 ? "entry" : "entries"}`);
    // A damaged data file was set aside at startup: say so once.
    const n = await api.startupNotice().catch(() => null);
    if (n) showToast(n, undefined, 15000);
  } catch (e) {
    app.error = String(e);
  }
  on("data-changed", () => refresh());
  // Main currency changed (here, in the capture popup, or by another app).
  on("settings-changed", () => refresh());
  on<Prefs>("prefs-changed", (p) => {
    app.prefs = p;
    applyTheme();
  });
  // Recurring items that fall due while the app stays open (checked hourly).
  // A new month while the app stays open (Android keeps it alive for days): the
  // home summary follows the calendar instead of staying on the old month.
  let shown = monthIso();
  const rollover = () => {
    const m = monthIso();
    if (m !== shown) {
      shown = m;
      app.month = m;
      refresh();
    }
  };
  setInterval(() => {
    rollover();
    api.generateRecurring().catch(() => {});
  }, 3600_000);
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) return;
    rollover();
    api.generateRecurring().then(refresh).catch(() => {});
  });
}

const mq = window.matchMedia("(prefers-color-scheme: dark)");
export function isDark() {
  const t = app.prefs.theme ?? "system";
  return t === "dark" || (t === "system" && mq.matches);
}
export function applyTheme() {
  const t = app.prefs.theme ?? "system";
  if (t === "system") delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = t;
  // Android: status/navigation bar icon colour follows the app theme.
  (window as unknown as { HisabNative?: { setDark(d: boolean): void } }).HisabNative?.setDark?.(isDark());
}
mq.addEventListener("change", applyTheme);

export async function setPrefs(p: Partial<Prefs>) {
  app.prefs = { ...app.prefs, ...p };
  applyTheme();
  await api.prefsSet(app.prefs);
}

/** Save a record; returns it, or null after showing the error. */
export async function saveRec<T>(coll: Coll, data: object, id?: string): Promise<T | null> {
  try {
    const r = await api.save<T>(coll, data, id);
    await refresh();
    return r;
  } catch (e) {
    showToast(String(e));
    return null;
  }
}

/** Tombstone with "Deleted · Undo". */
export async function removeRec(id: string, label = "Deleted") {
  await api.remove(id);
  await refresh();
  showToast(label, async () => {
    await api.restore(id);
    await refresh();
  });
}

/** Replace with previous data on undo (edits). */
export async function editRec<T extends { id: string }>(coll: Coll, before: T, after: object, label = "Saved") {
  const r = await saveRec<T>(coll, after, before.id);
  if (r)
    showToast(label, async () => {
      await api.save(coll, before, before.id);
      await refresh();
    });
  return r;
}

/** Hook a form into Back/Escape: unsaved input asks before discarding. */
export function guardForm(dirty: () => boolean, ask: () => void) {
  app.guard = () => {
    if (dirty()) {
      ask();
      return true;
    }
    return false;
  };
  return () => (app.guard = null);
}

/** Open History with filters preset (from Insights, Accounts, Categories). */
export const historyPreset = $state<{ v: Record<string, string>; n: number }>({ v: {}, n: 0 });
export function openHistory(f: Record<string, string>) {
  historyPreset.v = f;
  historyPreset.n++;
  go("history");
}
