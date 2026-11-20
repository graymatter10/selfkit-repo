// Typed wrappers over the Rust commands in src-tauri/src/lib.rs.
import { call } from "@selfkit/ui";

export interface Step { id: string; title: string; done: boolean }
export interface Task {
  id: string;
  title: string;
  notes?: string;
  severity: number;
  start?: string; // "YYYY-MM-DDTHH:MM" or "YYYY-MM-DD" (all day)
  stop?: string;
  reminder_min?: number;
  repeat?: string;
  /** Day of month a monthly/yearly repeat returns to after a short month clamped `start`. */
  repeat_day?: number;
  list?: string;
  steps?: Step[];
  tags?: string[];
  stage?: "doing";
  photos?: string[];
  last_completion?: string;
  done_at?: number | null;
  created_at?: number;
  updated_at: number;
}
export type NewTask = Omit<Task, "id" | "updated_at" | "done_at" | "created_at" | "severity"> & { severity?: number };
export interface ListInfo { id: string; name: string }
export interface Prefs { theme?: "system" | "light" | "dark"; reminder?: number | null; severity?: number; focus_min?: number; break_min?: number }
export interface Completion { id: string; task: string; title: string; done_at: number; tz_offset_min?: number; due?: string; on_time?: boolean; tags?: string[] }
export interface FocusSession { task?: string | null; title: string; kind: "focus" | "break"; start_ms: number; end_ms: number; planned_min: number }
export interface FocusLog { id: string; task?: string; title: string; kind: "focus" | "break"; start: number; end: number; minutes: number; planned_min: number; completed: boolean }
export interface ImportResult { added: number; updated: number; unchanged: number; skipped: number; kept: number }
export interface Settings { day_start_min: number; main_currency: string; theme: unknown; shortcuts: Record<string, string> }

export function dataOf(t: Task): Record<string, unknown> {
  const { id: _i, updated_at: _u, ...data } = t;
  return data;
}

export const api = {
  list: (filter: "active" | "done" | "all" = "all") => call<Task[]>("list_tasks", { filter }),
  create: (task: NewTask) => call<Task>("create_task", { task }),
  update: (id: string, patch: Record<string, unknown>) => call<Task>("update_task", { id, patch }),
  replace: (id: string, data: Record<string, unknown>) => call<Task>("replace_task", { id, data }),
  toggle: (id: string) => call<Task>("toggle_task", { id }),
  get: (id: string) => call<Task | null>("get_task", { id }),
  remove: (id: string) => call<void>("delete_task", { id }),
  restore: (id: string) => call<void>("restore_task", { id }),
  search: (q: string) => call<Task[]>("search", { q }),
  lists: () => call<ListInfo[]>("lists"),
  addList: (name: string) => call<string>("add_list", { name }),
  prefs: () => call<Prefs>("prefs_get"),
  savePrefs: (prefs: Prefs) => call<void>("prefs_set", { prefs }),
  settings: () => call<Settings>("settings_get"),
  saveSettings: (settings: Settings) => call<void>("settings_set", { settings }),
  closeCapture: () => call<void>("close_capture"),
  devHooks: () => call<string | null>("dev_hooks"),
  history: () => call<Completion[]>("history"),
  startupNotice: () => call<string | null>("startup_notice"),
  focusState: () => call<{ current: FocusSession | null; finished: FocusSession | null }>("focus_state"),
  focusStart: (task: string | null, title: string, kind: "focus" | "break", minutes: number) => call<FocusSession>("focus_start", { task, title, kind, minutes }),
  focusStop: () => call<void>("focus_stop"),
  focusSessions: () => call<FocusLog[]>("focus_sessions"),
  photoSave: (data: string, mime: string, w: number, h: number) => call<string>("photo_save", { data, mime, w, h }),
  photoGet: (id: string) => call<string>("photo_get", { id }),
  photoExport: (id: string) => call<string>("photo_export", { id }),
  icsExport: (includeDone: boolean) => call<string>("ics_export", { includeDone }),
  icsImport: (text: string) => call<ImportResult>("ics_import", { text }),
  saveExport: (name: string, text: string) => call<string>("save_export", { name, text }),
};
