// Typed wrappers over the Rust commands in src-tauri/src/lib.rs.
import { call } from "@selfkit/ui";

export type Kind = "a" | "w";
export interface Total { kind: Kind; name: string; package?: string | null; ms: number; opens: number; cat: string; platforms: string[]; prev: number; uninstalled?: number | null }
export interface DeviceTotal { id: string; name: string; platforms: string[]; ms: number; web: boolean }
export interface Goal { id: string; kind: "limit" | "goal"; scope: "total" | "category" | "app" | "domain"; target: string; minutes: number; enabled: boolean }
export interface GoalStatus { goal: Goal; used: number; state: "ok" | "near" | "over" | "todo" | "met" }
export interface DayNote { day: string; text: string; photos: string[]; updated_at: number }
export interface DayView {
  day: string; today: boolean; screen: number; unlocks: number; pickups: number; notifications: number;
  first: number | null; last: number | null; longest: [string, number, number] | null; hours: number[];
  cats: Record<string, number>; focus: number | null; top: Total[]; devices: DeviceTotal[]; goals: GoalStatus[];
  week_avg: number; week_so_far: number; last_week_same: number; strip: [number, number][]; day_start: number; day_end: number;
  notif_top: [string, number][]; phone: string | null; note: DayNote | null;
}
export interface Lane { device: string; name: string; kind: "phone" | "pc" | "browser"; segs: [number, number, string, string][]; ms: number }
export interface SessionRow { kind: Kind; name: string; device: string; device_id: string; device_name: string; platform: string; start: number; end: number; cat: string; uninstalled?: number | null }
export interface AppChange { id: string; package: string; label: string; kind: "installed" | "uninstalled" | "reinstalled" | "updated"; at: number; device: string; device_name: string; icon: string | null; lifetime: number; installed_now: boolean }
export interface TimelineView { day: string; from: number; to: number; lanes: Lane[]; sessions: SessionRow[] }
export interface Summary {
  screen: number; days: number; active_days: number; avg: number; unlocks: number; pickups: number; notifications: number;
  longest: [string, number, number] | null; focus: number | null; cats: Record<string, number>; sessions: number;
}
export interface Bucket { label: string; from: string; ms: number; focus_ms: number; days: number }
export type Period = "day" | "week" | "month" | "year" | "lifetime";
export interface TrendsView {
  period: Period; from: string; to: string; title: string; buckets: Bucket[]; summary: Summary; prev: Summary;
  apps: Total[]; sites: Total[]; devices: DeviceTotal[]; heatmap: number[][]; heat_from: string; heat_to: string;
  this_week: number; last_week: number; has_prev: boolean; prev_anchor: string; next_anchor: string | null;
}
export interface ReportDay { day: string; label: string; ms: number; prev_ms: number }
export interface GoalWeek { goal: Goal; days_ok: number; days: number; avg_used: number; kept: (boolean | null)[] }
export interface Report {
  week_start: string; week_end: string; title: string; cur: Summary; prev: Summary; days: ReportDay[]; apps: Total[]; sites: Total[];
  devices: DeviceTotal[]; busiest: [string, number] | null; quietest: [string, number] | null; peak_hour: number | null;
  goals: GoalWeek[]; prev_week: string | null; next_week: string | null; complete: boolean;
  longest: [string, number, string] | null; late_days: number; notif_top: [string, number] | null; days_counted: number;
}
export interface Source { id: string; name: string; platforms: string[]; last_seen: number; ms_30d: number; this_device: boolean; browsers: string[]; phone_counters: boolean }
export interface ChronoSettings {
  paused_until: number | null; exclude_apps: string[]; exclude_domains: string[]; categories: Record<string, string>;
  goals: Goal[]; alerts: boolean; device_names: Record<string, string>; pause_log: [number, number][];
  weekly_report: boolean; idle_minutes: number | null;
}
export interface Info { device: string; platform: "linux" | "windows" | "android"; day_start: number; today: string }
export interface AndroidStatus { supported: boolean; usage?: boolean; notifications?: boolean; last_import?: number | null }
export type ThemeMode = "system" | "light" | "dark";

export const api = {
  day: (day?: string) => call<DayView>("day_view", { day }),
  timeline: (day?: string) => call<TimelineView>("timeline", { day }),
  trends: (period: Period, anchor?: string) => call<TrendsView>("trends", { period, anchor }),
  report: (anchor?: string) => call<Report>("report", { anchor }),
  names: () => call<Total[]>("names"),
  sources: () => call<Source[]>("sources"),
  info: () => call<Info>("info"),
  settings: () => call<ChronoSettings>("settings_get"),
  saveSettings: (settings: ChronoSettings) => call<ChronoSettings>("settings_save", { settings }),
  pause: (minutes: number | null) => call<ChronoSettings>("pause", { minutes }),
  resume: () => call<ChronoSettings>("resume"),
  theme: () => call<ThemeMode>("theme_get"),
  setTheme: (mode: ThemeMode) => call<void>("theme_set", { mode }),
  prefs: () => call<Record<string, unknown>>("prefs_get"),
  savePrefs: (prefs: Record<string, unknown>) => call<void>("prefs_set", { prefs }),
  androidStatus: () => call<AndroidStatus>("android_status"),
  androidOpen: (which: "usage" | "notifications") => call<void>("android_open", { which }),
  androidImport: () => call<{ granted?: boolean; sessions?: number }>("android_import"),
  note: (day: string) => call<DayNote | null>("note_get", { day }),
  saveNote: (day: string, text: string, photos: string[]) => call<DayNote | null>("note_save", { day, text, photos }),
  photoSave: (data: string, mime: string, w: number, h: number) => call<string>("photo_save", { data, mime, w, h }),
  photoGet: (id: string) => call<string>("photo_get", { id }),
  photoExport: (id: string) => call<string>("photo_export", { id }),
  appChanges: () => call<AppChange[]>("app_changes"),
  devHooks: () => call<string | null>("dev_hooks"),
  startupNotice: () => call<string | null>("startup_notice"),
};
