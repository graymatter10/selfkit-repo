// Small formatting helpers (the Rust side writes most sentences).
import type { AppKey } from "./api";

export const APP_NAMES: Record<AppKey, string> = { chrono: "Chrono", hisab: "Hisab", agenda: "Agenda", dusk: "Dusk" };
export const APP_WHAT: Record<AppKey, string> = {
  chrono: "Screen time, apps and websites",
  hisab: "Spending and budgets",
  agenda: "Tasks done",
  dusk: "Sleep",
};

/** "3 h 12" / "45 min" from minutes. */
export function dur(min: number): string {
  const m = Math.max(0, Math.round(min));
  return m >= 60 ? `${Math.floor(m / 60)}\u00a0h\u00a0${String(m % 60).padStart(2, "0")}` : `${m}\u00a0min`;
}
/** "3 h", "2 h 30", "45 min". */
export function durShort(min: number): string {
  const m = Math.round(min), h = Math.floor(m / 60), r = m % 60;
  return h === 0 ? `${r} min` : r === 0 ? `${h} h` : `${h} h ${String(r).padStart(2, "0")}`;
}
export function clock(min: number): string {
  const m = ((Math.round(min) % 1440) + 1440) % 1440;
  return `${String(Math.floor(m / 60)).padStart(2, "0")}:${String(m % 60).padStart(2, "0")}`;
}
export function money(sym: string, v: number): string {
  const a = Math.abs(v);
  const body = a < 100 && Math.abs(a - Math.round(a)) > 0.004 ? a.toFixed(2) : Math.round(a).toLocaleString("en-US");
  return `${v < 0 ? "-" : ""}${sym}${body}`;
}
export function bytes(n: number): string {
  if (n >= 1024 ** 3) return `${(n / 1024 ** 3).toFixed(1)} GB`;
  if (n >= 1024 ** 2) return `${Math.round(n / 1024 ** 2)} MB`;
  return `${Math.max(1, Math.round(n / 1024))} KB`;
}
/** "3 min ago", "2 h ago", "yesterday 14:05", "12 Nov". */
export function ago(ms: number | null | undefined, now = Date.now()): string {
  if (!ms) return "never";
  const s = Math.max(0, (now - ms) / 1000);
  if (s < 60) return "just now";
  if (s < 3600) return `${Math.round(s / 60)} min ago`;
  const d = new Date(ms), t = new Date(now);
  const hm = `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
  if (d.toDateString() === t.toDateString()) return `today ${hm}`;
  const y = new Date(now - 86_400_000);
  if (d.toDateString() === y.toDateString()) return `yesterday ${hm}`;
  if (s < 6 * 86400) return `${d.toLocaleDateString("en-GB", { weekday: "short" })} ${hm}`;
  return d.toLocaleDateString("en-GB", { day: "numeric", month: "short" });
}
export function unitValue(unit: string, sym: string, v: number): string {
  return unit === "min" ? dur(v) : unit === "money" ? money(sym, v) : v.toFixed(1);
}
