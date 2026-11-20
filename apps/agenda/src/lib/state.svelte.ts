// App state. All tasks are loaded once and filtered/grouped in the UI, so a
// task can never "disappear" because of the view (the v1 bug).
import { on, showToast } from "@selfkit/ui";
import { isPermissionGranted, requestPermission } from "@tauri-apps/plugin-notification";
import { api, dataOf, type FocusSession, type ListInfo, type NewTask, type Prefs, type Settings, type Task } from "./api";
import { groupOf, parseWhen, shortDay, hm, sortTasks, type GroupKey, type Sort, dayLabel, GROUP_LABEL } from "./time";

export type Filter = "active" | "all" | "done" | "overdue";
export type Panel = { kind: "none" } | { kind: "detail"; id: string } | { kind: "new"; start?: string } | { kind: "edit"; id: string };
/** Main views. "list" is the grouped Today list; history and stats are their own screens. */
export type View = "list" | "upcoming" | "calendar" | "board" | "history" | "stats";

export const app = $state({
  tasks: [] as Task[],
  lists: [] as ListInfo[],
  filter: "active" as Filter,
  sort: "start" as Sort,
  panel: { kind: "none" } as Panel,
  searching: false,
  query: "",
  results: [] as Task[],
  prefs: {} as Prefs,
  settings: null as Settings | null,
  settingsOpen: false,
  /** Settings → Sync layer and its conflict screen (shared ui/src/sync). */
  syncOpen: false,
  conflictsOpen: false,
  view: "list" as View,
  /** Tag filter for every view (null = all). */
  tag: null as string | null,
  /** Running focus (Pomodoro) session, and the one that just finished. */
  focus: null as FocusSession | null,
  focusDone: null as FocusSession | null,
  focusOpen: false,
  /** Full-screen photo viewer. */
  photo: null as string | null,
  now: new Date(),
  error: "",
  loaded: false,
  /** Test hook (AGENDA_DEMO) for screenshots; empty in normal use. */
  demo: {} as Record<string, string>,
  /** Set by an open form: returns true when it handled a close request (e.g. asked to discard). */
  closeGuard: null as null | (() => boolean),
});

export const defaultReminder = () => (app.prefs.reminder === undefined ? 30 : app.prefs.reminder);
export const defaultSeverity = () => app.prefs.severity ?? 3;

export function counts() {
  const now = app.now;
  let active = 0, overdue = 0, doneToday = 0;
  for (const t of tagged()) {
    if (t.done_at) {
      if (new Date(t.done_at).toDateString() === now.toDateString()) doneToday++;
    } else {
      active++;
      if (groupOf(t, now) === "overdue") overdue++;
    }
  }
  return { active, overdue, doneToday };
}

/** All tags in use, most used first. */
export function allTags(): string[] {
  const n = new Map<string, number>();
  for (const t of app.tasks) for (const g of t.tags ?? []) n.set(g, (n.get(g) ?? 0) + (t.done_at ? 0.01 : 1));
  return [...n.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])).map(([g]) => g);
}

/** Tasks passing the tag filter. */
export const tagged = () => (app.tag ? app.tasks.filter((t) => t.tags?.includes(app.tag!)) : app.tasks);

export function groups(): { key: GroupKey; label: string; tasks: Task[] }[] {
  const now = app.now;
  const shown = tagged().filter((t) =>
    app.filter === "all" ? true : app.filter === "done" ? !!t.done_at : app.filter === "overdue" ? groupOf(t, now) === "overdue" : !t.done_at,
  );
  const order: GroupKey[] = ["overdue", "today", "tomorrow", "later", "nodate", "done"];
  const by = new Map<GroupKey, Task[]>();
  for (const t of shown) {
    const g = groupOf(t, now);
    if (!by.has(g)) by.set(g, []);
    by.get(g)!.push(t);
  }
  return order
    .filter((k) => by.has(k))
    .map((k) => ({
      key: k,
      label: GROUP_LABEL[k],
      tasks: k === "done" ? [...by.get(k)!].sort((a, b) => (b.done_at ?? 0) - (a.done_at ?? 0)) : sortTasks(by.get(k)!, app.sort),
    }));
}

export function taskById(id: string) {
  return app.tasks.find((t) => t.id === id);
}

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

/** Reload tasks. Calls that arrive while one runs are folded into one more
 * run (a save used to reload twice: once itself, once via "tasks-changed"). */
let inflight: Promise<void> | null = null;
let again = false;
export function refresh(): Promise<void> {
  if (inflight) {
    again = true;
    return inflight;
  }
  inflight = (async () => {
    do {
      again = false;
      await load();
    } while (again);
  })().finally(() => (inflight = null));
  return inflight;
}

async function load() {
  const [tasks, lists] = await Promise.all([guard(api.list("all")), guard(api.lists())]);
  if (tasks) app.tasks = tasks;
  if (lists) app.lists = lists;
  app.loaded = true;
  if (app.searching && app.query.trim()) app.results = (await guard(api.search(app.query))) ?? [];
}

function whereLabel(t: Task) {
  const s = parseWhen(t.start);
  if (!s) return "no date";
  return s.allDay ? dayLabel(s.date, app.now) : `${dayLabel(s.date, app.now) === "Today" ? "today" : shortDay(s.date)} ${hm(s.date)}`;
}

/** Android 13+ needs runtime permission before reminders can show. Ask once, when first needed. */
async function ensureNotifyPermission(t: { start?: string; reminder_min?: number | null }) {
  if (!t.start || t.reminder_min == null) return;
  try {
    if (!(await isPermissionGranted())) await requestPermission();
  } catch (e) {
    console.warn("notification permission", e);
  }
}

/** Create; returns the task or throws a readable message for the form. */
export async function createTask(t: NewTask): Promise<Task> {
  const created = await api.create(t);
  ensureNotifyPermission(created);
  if (app.filter === "done") app.filter = "active";
  showToast(`Added · ${whereLabel(created)}`, async () => api.remove(created.id));
  await refresh();
  return created;
}

export async function saveEdit(id: string, patch: Record<string, unknown>): Promise<Task> {
  const before = taskById(id);
  const t = await api.update(id, patch);
  ensureNotifyPermission(t);
  if (before) showToast(`Saved · ${whereLabel(t)}`, async () => void (await api.replace(id, dataOf(before))));
  await refresh();
  return t;
}

/** Tasks with a toggle/move in flight: a double tap must not complete a repeating task twice. */
const busy = new Set<string>();

export async function toggle(t: Task) {
  if (busy.has(t.id)) return;
  busy.add(t.id);
  try {
    const before = dataOf(t);
    const after = await guard(api.toggle(t.id));
    if (!after) return;
    const moved = !t.done_at && !after.done_at;
    showToast(t.done_at ? `Not done · ${t.title}` : moved ? `Done · next ${whereLabel(after)}` : `Done · ${t.title}`, async () => {
      await api.replace(t.id, before);
    });
    await refresh();
  } finally {
    busy.delete(t.id);
  }
}

export async function remove(t: Task) {
  await guard(api.remove(t.id));
  if ("id" in app.panel && app.panel.id === t.id) app.panel = { kind: "none" };
  showToast(`Deleted · ${t.title}`, async () => api.restore(t.id));
  await refresh();
}

export type Stage = "todo" | "doing" | "done";
export const stageOf = (t: Task): Stage => (t.done_at ? "done" : t.stage === "doing" ? "doing" : "todo");
export const STAGE_LABEL: Record<Stage, string> = { todo: "To Do", doing: "Doing", done: "Done" };

/** Board move. Into/out of Done completes/reopens (repeating tasks move to their next date). */
export async function moveStage(t: Task, to: Stage) {
  const from = stageOf(t);
  if (from === to || busy.has(t.id)) return;
  busy.add(t.id);
  try {
    await doMove(t, from, to);
  } finally {
    busy.delete(t.id);
  }
}

async function doMove(t: Task, from: Stage, to: Stage) {
  const before = dataOf(t);
  let after: Task | undefined;
  if (to === "done" || from === "done") {
    after = await guard(api.toggle(t.id));
    if (after && to === "doing" && !after.done_at) after = await guard(api.update(t.id, { stage: "doing" }));
  } else {
    after = await guard(api.update(t.id, { stage: to === "doing" ? "doing" : null }));
  }
  if (!after) return;
  const moved = to === "done" && !after.done_at;
  showToast(moved ? `Done · next ${whereLabel(after)}` : `Moved to ${STAGE_LABEL[to]} · ${t.title}`, async () => void (await api.replace(t.id, before)));
  await refresh();
}

// ---- Focus (Pomodoro) ----
export const focusMin = () => app.prefs.focus_min ?? 25;
export const breakMin = () => app.prefs.break_min ?? 5;

export async function startFocus(task: Task | null, kind: "focus" | "break" = "focus", minutes?: number) {
  ensureNotifyPermission({ start: "x", reminder_min: 0 });
  const title = kind === "break" ? "Break" : (task?.title ?? "Focus");
  const s = await guard(api.focusStart(task?.id ?? null, title, kind, minutes ?? (kind === "break" ? breakMin() : focusMin())));
  if (s) {
    app.focus = s;
    app.focusDone = null;
  }
}

export async function stopFocus() {
  await guard(api.focusStop());
  app.focus = null;
}

/** Ask Rust for the session state; it logs a session that reached its end. */
export async function pollFocus() {
  const st = await guard(api.focusState());
  if (!st) return;
  app.focus = st.current;
  if (st.finished) app.focusDone = st.finished;
}

export async function patchSteps(t: Task, steps: Task["steps"]) {
  await guard(api.update(t.id, { steps }));
  await refresh();
}

const darkQuery = window.matchMedia("(prefers-color-scheme: dark)");
export function applyTheme() {
  const t = app.prefs.theme ?? "system";
  if (t === "system") delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = t;
  // Android: match the status-bar icon colour to the app's theme (MainActivity.kt).
  const bars = (window as unknown as { AgendaBars?: { setDark(d: boolean): void } }).AgendaBars;
  bars?.setDark(t === "dark" || (t === "system" && darkQuery.matches));
}
darkQuery.addEventListener("change", () => applyTheme());

export async function savePrefs(p: Prefs) {
  app.prefs = p;
  applyTheme();
  await guard(api.savePrefs(p));
}

export async function saveSettings(s: Settings) {
  app.settings = s;
  await guard(api.saveSettings(s));
}

/** localStorage key of an unsaved New Task form (TaskForm.svelte, P5). */
export const NEW_TASK_DRAFT = "agenda.newTaskDraft";

export function init() {
  guard(api.prefs()).then((p) => {
    app.prefs = p ?? {};
    applyTheme();
  });
  guard(api.settings()).then((s) => s && (app.settings = s));
  refresh();
  // A damaged data file was set aside at startup: say so once.
  guard(api.startupNotice()).then((n) => n && showToast(n, undefined, 15000));
  on("tasks-changed", () => refresh());
  on("data-changed", () => refresh()); // a sync pass brought new data
  on("focus-changed", () => pollFocus());
  pollFocus();
  // While a session runs, check every second so the end is noticed on time.
  setInterval(() => {
    if (app.focus && Date.now() >= app.focus.end_ms) pollFocus();
  }, 1000);
  on<Prefs>("prefs-changed", (p) => ((app.prefs = p), applyTheme()));
  setInterval(() => (app.now = new Date()), 30_000);
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") {
      app.now = new Date();
      refresh();
      pollFocus();
    }
  });
}
