// Formatting. Times are shown in the offset they happened in (UTC ms + tz minutes),
// not the browser's current zone, so a night abroad keeps its local clock times.
export const QUALITY = ["", "Awful", "Poor", "OK", "Good", "Great"];
const DAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

export function dur(min: number | null | undefined): string {
  if (min == null) return "–";
  const neg = min < 0;
  const m = Math.abs(Math.round(min));
  const s = m >= 60 ? `${Math.floor(m / 60)}h ${String(m % 60).padStart(2, "0")}m` : `${m}m`;
  return neg ? `−${s}` : s;
}
export const durShort = (min: number) => (min >= 60 ? `${Math.floor(min / 60)}h${min % 60 ? String(min % 60).padStart(2, "0") : ""}` : `${min}m`);

const local = (ms: number, tz: number) => new Date(ms + tz * 60000);
export function hm(ms: number | null | undefined, tz: number): string {
  if (ms == null) return "–";
  const d = local(ms, tz);
  return `${String(d.getUTCHours()).padStart(2, "0")}:${String(d.getUTCMinutes()).padStart(2, "0")}`;
}
/** Minutes after midnight (possibly negative/over 1440) → "HH:MM". */
export function clock(min: number | null | undefined): string {
  if (min == null) return "–";
  const m = ((Math.round(min) % 1440) + 1440) % 1440;
  return `${String(Math.floor(m / 60)).padStart(2, "0")}:${String(m % 60).padStart(2, "0")}`;
}
export function parseDate(s: string) {
  const [y, m, d] = s.split("-").map(Number);
  return new Date(Date.UTC(y, m - 1, d));
}
export function addDays(s: string, n: number) {
  const d = parseDate(s);
  d.setUTCDate(d.getUTCDate() + n);
  return d.toISOString().slice(0, 10);
}
export const dayShort = (s: string) => { const d = parseDate(s); return `${DAYS[d.getUTCDay()]} ${d.getUTCDate()}`; };
export const dayLong = (s: string) => { const d = parseDate(s); return `${DAYS[d.getUTCDay()]} ${d.getUTCDate()} ${MONTHS[d.getUTCMonth()]}`; };
export const weekday1 = (s: string) => DAYS[parseDate(s).getUTCDay()].slice(0, 1);
/** "Sat 14 → Sun 15" for a night keyed by its morning. */
export const nightSpan = (s: string) => `${dayShort(addDays(s, -1))} → ${dayShort(s)}`;
export function nightTitle(s: string, current: string) {
  if (s === current) return "Last night";
  if (s === addDays(current, -1)) return "The night before";
  if (s === addDays(current, 1)) return "Tonight";
  return dayLong(s);
}
/** Normalise typed time to "HH:MM" (accepts 2330, 930, 23.30, 11:30pm). Null if unreadable. */
export function normTime(raw: string): string | null {
  let s = raw.trim().toLowerCase().replace(/\s+/g, "");
  if (!s) return null;
  let pm = false, am = false;
  if (/p(m)?$/.test(s)) { pm = true; s = s.replace(/p(m)?$/, ""); } else if (/a(m)?$/.test(s)) { am = true; s = s.replace(/a(m)?$/, ""); }
  let h: number, m: number;
  const parts = s.split(/[:.]/);
  if (parts.length === 2) { h = Number(parts[0]); m = Number(parts[1]); }
  else if (/^\d{1,4}$/.test(s)) {
    if (s.length <= 2) { h = Number(s); m = 0; } else if (s.length === 3) { h = Number(s[0]); m = Number(s.slice(1)); } else { h = Number(s.slice(0, 2)); m = Number(s.slice(2)); }
  } else return null;
  if (!Number.isInteger(h) || !Number.isInteger(m)) return null;
  if ((pm || am) && h > 12) return null;
  if (pm && h < 12) h += 12;
  if (am && h === 12) h = 0;
  if (h > 23 || m > 59 || h < 0 || m < 0) return null;
  return `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}`;
}
export function shiftTime(t: string, by: number): string {
  const [h, m] = t.split(":").map(Number);
  return clock(h * 60 + m + by);
}
export const toMin = (t: string) => { const [h, m] = t.split(":").map(Number); return h * 60 + m; };
/** Hours and minutes for the big number: [7, "33"]. */
export function hmParts(min: number): [number, string] {
  const m = Math.max(0, Math.round(min));
  return [Math.floor(m / 60), String(m % 60).padStart(2, "0")];
}
/** "Sat 14 – Sun 15 Nov" */
export function nightRange(s: string) {
  const a = parseDate(addDays(s, -1)), b = parseDate(s);
  const sameMonth = a.getUTCMonth() === b.getUTCMonth();
  return `${DAYS[a.getUTCDay()]} ${a.getUTCDate()}${sameMonth ? "" : " " + MONTHS[a.getUTCMonth()]} – ${DAYS[b.getUTCDay()]} ${b.getUTCDate()} ${MONTHS[b.getUTCMonth()]}`;
}
/** "Mo", "Tu" … */
export const wd2 = (s: string) => DAYS[parseDate(s).getUTCDay()].slice(0, 2);
export const dayNum = (s: string) => String(parseDate(s).getUTCDate());
export const plural = (n: number, one: string, many = one + "s") => `${n} ${n === 1 ? one : many}`;
/** 12-hour clock? No app/shared preference exists, so follow the system locale
 *  (Intl hour cycle; DUSK_DEMO `clock=12|24` overrides it for screenshots). */
let clock12: boolean | null = null;
export function setClock12(v: boolean | null) { clock12 = v; }
export function uses12h(): boolean {
  if (clock12 != null) return clock12;
  try {
    const hc = new Intl.DateTimeFormat(undefined, { hour: "numeric" }).resolvedOptions().hourCycle;
    return hc === "h11" || hc === "h12";
  } catch { return false; }
}
/** "HH:MM" → what a time field shows: "23:30" or "11:30 pm" (12h locales). */
export function tdisp(t: string | null | undefined): string {
  if (!t) return "";
  if (!uses12h()) return t;
  const [h, m] = t.split(":").map(Number);
  return `${h % 12 || 12}:${String(m).padStart(2, "0")} ${h < 12 ? "am" : "pm"}`;
}
