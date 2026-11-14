// Formatting helpers: durations, times, dates, category names/colours.

export const CATS = ["work", "study", "social", "entertainment", "other"] as const;
export const CAT_NAME: Record<string, string> = { work: "Work", study: "Study", social: "Social", entertainment: "Entertainment", other: "Other" };
export const catName = (c: string) => CAT_NAME[c] ?? c.charAt(0).toUpperCase() + c.slice(1);
export const catColor = (c: string) =>
  ({ work: "var(--c-work)", study: "var(--c-study)", social: "var(--c-social)", entertainment: "var(--c-ent)" })[c] ?? "var(--c-other)";

/** "6 h 12" or "46 min" (ms). */
export function dur(ms: number): string {
  const m = Math.round(Math.max(0, ms) / 60000);
  return m >= 60 ? `${Math.floor(m / 60)} h ${String(m % 60).padStart(2, "0")}` : `${m} min`;
}

/** Same with seconds for tiny values. */
export function durShort(ms: number): string {
  return ms < 60000 ? `${Math.max(1, Math.round(ms / 1000))} s` : dur(ms);
}

export function hm(t: number | null | undefined): string {
  if (t == null) return "–";
  const d = new Date(t);
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

export function parseDay(s: string): Date {
  const [y, m, d] = s.split("-").map(Number);
  return new Date(y, m - 1, d, 12);
}
export function dayStr(d: Date): string {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}
export function addDays(s: string, n: number): string {
  const d = parseDay(s);
  d.setDate(d.getDate() + n);
  return dayStr(d);
}

const WD = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const MO = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
export const weekdayName = (s: string) => WD[parseDay(s).getDay()];
export const weekdayShort = (s: string) => WD[parseDay(s).getDay()].slice(0, 3);
export const monthName = (m: number) => MO[m];
/** "Sunday, 15 November" */
export const longDay = (s: string) => `${weekdayName(s)}, ${parseDay(s).getDate()} ${MO[parseDay(s).getMonth()]}`;
/** "15 November" */
export const dayMonth = (s: string) => `${parseDay(s).getDate()} ${MO[parseDay(s).getMonth()]}`;
/** "Sun 15" */
export const shortDay = (s: string) => `${weekdayShort(s)} ${parseDay(s).getDate()}`;
/** "2 – 8 November" or "28 Oct – 3 Nov" */
export function weekRange(a: string, b: string): string {
  const x = parseDay(a), y = parseDay(b);
  if (x.getMonth() === y.getMonth()) return `${x.getDate()} – ${y.getDate()} ${MO[y.getMonth()]}`;
  return `${x.getDate()} ${MO[x.getMonth()].slice(0, 3)} – ${y.getDate()} ${MO[y.getMonth()].slice(0, 3)}`;
}

export function ago(t: number | null | undefined, now = Date.now()): string {
  if (!t) return "never";
  const m = Math.round((now - t) / 60000);
  if (m < 1) return "just now";
  if (m < 60) return `${m} min ago`;
  const h = Math.round(m / 60);
  if (h < 48) return `${h} h ago`;
  return `${Math.round(h / 24)} days ago`;
}

export function pct(a: number, b: number): number {
  return b > 0 ? Math.round((a / b) * 100) : 0;
}

/** Device glyph for a platform list. */
export function devIcon(platforms: string[]): "phone" | "pc" | "globe" {
  if (platforms.includes("android")) return "phone";
  if (platforms.includes("linux") || platforms.includes("windows")) return "pc";
  return "globe";
}

export function goalName(g: { scope: string; target: string }): string {
  return g.scope === "total" ? "All screen time" : g.scope === "category" ? catName(g.target) : g.target;
}

/** Monday of the week containing `s`. */
export function mondayOf(s: string): string {
  const d = parseDay(s);
  return addDays(s, -((d.getDay() + 6) % 7));
}
