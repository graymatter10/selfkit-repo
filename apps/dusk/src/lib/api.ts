// Typed wrappers over the Rust commands in src-tauri/src/lib.rs.
import { call } from "@selfkit/ui";

export type Status = "found" | "allnighter" | "ongoing" | "no_data";
export interface LogEntry {
  id: string; night: string; kind: "main" | "nap"; bed: number | null; wake: number | null;
  tz_offset: number; quality: number | null; note: string | null; created_at: number;
}
export interface Estimate {
  status: Status; bed: number | null; wake: number | null; awake_min: number; interruptions: number;
  late_use_min: number; activity: [number, number][]; source: string;
}
export interface Night {
  night: string; window: [number, number]; tz_offset: number; estimate: Estimate; log: LogEntry | null; naps: LogEntry[];
  bed: number | null; wake: number | null; asleep_min: number; nap_min: number; quality: number | null;
  source: "logged" | "estimate" | "allnighter" | "ongoing" | "none"; late_use_min: number;
}
export interface Stats {
  nights: number; avg_min: number | null; prev_avg_min: number | null; consistency: number | null;
  bed_sd_min: number | null; wake_sd_min: number | null; debt_min: number; avg_bed_min: number | null;
  avg_wake_min: number | null; bedtime_hits: number; bedtime_total: number; avg_quality: number | null;
}
export interface Goals { goal_min: number; bedtime: string; winddown_min: number }
export interface Dashboard {
  current: string; nights: Night[]; s7: Stats; s14: Stats; s30: Stats; s90: Stats;
  weekly: { week: string; avg_min: number; nights: number }[]; goals: Goals; day_start_min: number; now: number;
}
export interface Bucket { label: string; nights: number; avg: number | null; avg_sleep_min: number | null }
export interface Connection { id: string; title: string; buckets: Bucket[]; r: number | null; pairs: number; summary: string; available: boolean }
export interface DevicePrefs { winddown_on: boolean; theme: "system" | "light" | "dark"; import_cursor: number }
export interface Settings { day_start_min: number; main_currency: string; theme: unknown; shortcuts: Record<string, string> }
export interface Sources { chrono: boolean; agenda: boolean; hisab: boolean; mobile: boolean; phone_nights: number; data_dir: string }
export type LogPatch = { bed?: string | null; wake?: string | null; quality?: number | null; note?: string | null };

export const api = {
  dashboard: () => call<Dashboard>("dashboard"),
  night: (night: string) => call<Night>("night", { night }),
  sources: () => call<Sources>("sources_status"),
  connections: () => call<Connection[]>("connections"),
  logSet: (night: string, patch: LogPatch) => call<Night>("log_set", { night, patch }),
  logReplace: (night: string, before: LogEntry | null) => call<void>("log_replace", { night, before }),
  napAdd: (night: string, start: string, end: string) => call<LogEntry>("nap_add", { night, start, end }),
  napDelete: (id: string) => call<void>("nap_delete", { id }),
  napRestore: (id: string) => call<void>("nap_restore", { id }),
  goalsSet: (goals: Goals) => call<Goals>("goals_set", { goals }),
  device: () => call<DevicePrefs>("device_get"),
  deviceSet: (prefs: DevicePrefs) => call<void>("device_set", { prefs }),
  settings: () => call<Settings>("settings_get"),
  settingsSet: (settings: Settings) => call<void>("settings_set", { settings }),
  androidImport: (events: [number, number][], from: number, to: number) => call<string[]>("android_import", { events, from, to }),
  closeCapture: () => call<void>("close_capture"),
  devHooks: () => call<string | null>("dev_hooks"),
};
