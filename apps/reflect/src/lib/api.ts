// Typed wrappers over the Rust commands in src-tauri/src/lib.rs.
import { call } from "@selfkit/ui";

export type AppKey = "chrono" | "hisab" | "agenda" | "dusk";
export interface Has { chrono: boolean; phone: boolean; hisab: boolean; agenda: boolean; dusk: boolean }
export interface Tile { value: string; unit: string; strong: string; sub: string }
export interface Ribbon { from: number; to: number; now: number; first_hour: number; sleep: [number, number][]; screen: [number, number][]; money: number[]; tasks: number[] }
export interface AppRow { name: string; device: string; phone: boolean; min: number }
export interface Today {
  date_label: string; headline: string; subline: string;
  screen: Tile | null; spent: Tile | null; tasks: Tile | null; sleep: Tile | null;
  ribbon: Ribbon; top_apps: AppRow[]; has: Has;
}
export interface Overview { today: Today; sym: string; now: number; mobile: boolean }
export interface StreakView {
  id: string; kind: string; name: string; app: AppKey; on: boolean; value: number; current: number; best: number;
  pattern: string[]; kept: number; of: number; note: string; available: boolean; missing: string;
}
export interface Group { label: string; value: number; n: number }
export interface Bucket { label: string; value: number | null; n: number }
export interface Conn {
  id: string; topic: string; finding: string; strength: number; groups: Group[]; buckets: Bucket[]; r: number | null;
  pairs: number; per: string; apps: AppKey[]; unit: "min" | "money" | "tasks"; measure: string; available: boolean; missing: string; enough: boolean;
}
export interface Sect { value: string; unit: string; sub: string; series: (number | null)[]; goal: number | null; goal_label: string }
export interface Review {
  kind: "week" | "month"; offset: number; label: string; short_label: string; range_label: string; in_progress: boolean;
  can_next: boolean; can_prev: boolean; headline: string; sub: string; day_labels: string[]; dates: string[]; lead: number;
  screen: Sect | null; spent: Sect | null; tasks: Sect | null; sleep: Sect | null;
  worth: { icon: string; text: string }[]; kept: { name: string; pattern: string[]; kept: number; of: number }[]; top_apps: AppRow[]; empty: boolean;
}
export interface Health { found: boolean; running: boolean; age_s: number; doc: any; mobile: boolean; local_dbs: [string, number][]; data_dir: string }
export interface StreakDef { id: string; kind: string; value: number; on: boolean }
export interface RSettings { streaks: StreakDef[]; lookback_days: number; week_start: number }
export interface Sources { has: Has; found: [AppKey, boolean][]; first: [AppKey, string | null][]; mobile: boolean }
export interface Shared { day_start_min: number; main_currency: string }
export type Theme = "system" | "light" | "dark";

export const api = {
  overview: () => call<Overview>("overview"),
  streaks: () => call<StreakView[]>("streaks"),
  connections: () => call<{ items: Conn[]; lookback_days: number; sym: string }>("connections"),
  review: (kind: string, offset: number) => call<Review>("review", { kind, offset }),
  health: () => call<Health>("health"),
  agentCmd: (cmd: "sync_now" | "backup_now") => call<any>("agent_cmd", { cmd }),
  sources: () => call<Sources>("sources_status"),
  settings: () => call<RSettings>("settings_get"),
  settingsSet: (settings: RSettings) => call<RSettings>("settings_set", { settings }),
  shared: () => call<Shared>("shared_get"),
  prefs: () => call<{ theme: Theme }>("prefs_get"),
  prefsSet: (prefs: { theme: Theme }) => call<void>("prefs_set", { prefs }),
  devHooks: () => call<string | null>("dev_hooks"),
};
