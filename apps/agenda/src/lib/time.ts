// Local date/time helpers, task status ("Overdue by 1d 22h", "Starts in 25 min")
// and grouping. All in the device's local time.
import type { Task } from "./api";

export const SEVERITY = ["", "Low", "Minor", "Medium", "High", "Critical"] as const;
/** Quiet priority marks shown before a title (iOS-style); only High/Critical get one. */
export const sevMark = (n: number) => (n >= 5 ? "!!!" : n === 4 ? "!!" : "");
const DOW = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const DOW_LONG = ["SUNDAY", "MONDAY", "TUESDAY", "WEDNESDAY", "THURSDAY", "FRIDAY", "SATURDAY"];
const MON = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
export const MON_LONG = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

export interface When { date: Date; allDay: boolean }

export function parseWhen(s?: string | null): When | null {
  if (!s) return null;
  const m = /^(\d{4})-(\d{2})-(\d{2})(?:T(\d{2}):(\d{2}))?$/.exec(s);
  if (!m) return null;
  const d = new Date(+m[1], +m[2] - 1, +m[3], m[4] ? +m[4] : 0, m[5] ? +m[5] : 0);
  return { date: d, allDay: !m[4] };
}

const p2 = (n: number) => String(n).padStart(2, "0");
export const isoDate = (d: Date) => `${d.getFullYear()}-${p2(d.getMonth() + 1)}-${p2(d.getDate())}`;
export const hm = (d: Date) => `${p2(d.getHours())}:${p2(d.getMinutes())}`;
export const fmtWhen = (d: Date, allDay: boolean) => (allDay ? isoDate(d) : `${isoDate(d)}T${hm(d)}`);
export const startOfDay = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate());
export const addDays = (d: Date, n: number) => new Date(d.getFullYear(), d.getMonth(), d.getDate() + n, d.getHours(), d.getMinutes());
export const dayDiff = (a: Date, b: Date) => Math.round((startOfDay(a).getTime() - startOfDay(b).getTime()) / 86_400_000);

export function eyebrow(now: Date) {
  return `${DOW_LONG[now.getDay()]}, ${now.getDate()} ${MON_LONG[now.getMonth()].toUpperCase()}`;
}
export function shortDay(d: Date) {
  return `${DOW[d.getDay()]}, ${d.getDate()} ${MON[d.getMonth()]}`;
}
export function dayLabel(d: Date, now: Date) {
  const n = dayDiff(d, now);
  if (n === 0) return "Today";
  if (n === 1) return "Tomorrow";
  if (n === -1) return "Yesterday";
  return shortDay(d);
}

/** "Today · 10:30 → 11:00", "Fri, 13 Nov · all day". */
export function whenLabel(t: Task, now: Date): string {
  const s = parseWhen(t.start);
  if (!s) return "";
  const e = parseWhen(t.stop);
  let out = dayLabel(s.date, now);
  if (s.allDay) out += " · all day";
  else out += ` · ${hm(s.date)}`;
  if (e && !s.allDay) out += dayDiff(e.date, s.date) === 0 ? ` → ${hm(e.date)}` : ` → ${shortDay(e.date)} ${hm(e.date)}`;
  return out;
}

export function fmtDur(ms: number): string {
  const min = Math.max(1, Math.round(ms / 60_000));
  if (min < 60) return `${min} min`;
  const h = Math.floor(min / 60), m = min % 60;
  if (h < 24) return m ? `${h} h ${m} min` : `${h} h`;
  const d = Math.floor(h / 24), hh = h % 24;
  return hh ? `${d}d ${hh}h` : `${d}d`;
}

export function fmtMin(m: number): string {
  if (m < 60) return `${m} min`;
  if (m % 60 === 0) return `${m / 60} h`;
  return `${Math.floor(m / 60)} h ${m % 60} min`;
}

/** When the task is due to be finished. */
export function endOf(t: Task): Date | null {
  const e = parseWhen(t.stop);
  if (e) return e.date;
  const s = parseWhen(t.start);
  if (!s) return null;
  return s.allDay ? addDays(startOfDay(s.date), 1) : s.date;
}

export type StatusKind = "overdue" | "soon" | "progress" | "done" | "none";
export function status(t: Task, now: Date): { kind: StatusKind; text: string } {
  if (t.done_at) {
    const d = new Date(t.done_at);
    return { kind: "done", text: dayDiff(d, now) === 0 ? `Done today at ${hm(d)}` : `Done ${dayLabel(d, now)}` };
  }
  const s = parseWhen(t.start);
  const end = endOf(t);
  if (!s || !end) return { kind: "none", text: "" };
  if (end.getTime() < now.getTime()) return { kind: "overdue", text: `Overdue by ${fmtDur(now.getTime() - end.getTime())}` };
  if (!s.allDay && s.date.getTime() <= now.getTime() && parseWhen(t.stop)) return { kind: "progress", text: `In progress · ends ${hm(end)}` };
  if (!s.allDay && s.date.getTime() > now.getTime() && s.date.getTime() - now.getTime() <= 24 * 3_600_000)
    return { kind: "soon", text: `Starts in ${fmtDur(s.date.getTime() - now.getTime())}` };
  return { kind: "none", text: "" };
}

export type GroupKey = "overdue" | "today" | "tomorrow" | "later" | "nodate" | "done";
export const GROUP_LABEL: Record<GroupKey, string> = {
  overdue: "Overdue", today: "Today", tomorrow: "Tomorrow", later: "Later", nodate: "No date", done: "Done",
};
export function groupOf(t: Task, now: Date): GroupKey {
  if (t.done_at) return "done";
  const s = parseWhen(t.start);
  if (!s) return "nodate";
  if (status(t, now).kind === "overdue") return "overdue";
  const n = dayDiff(s.date, now);
  if (n <= 0) return "today";
  if (n === 1) return "tomorrow";
  return "later";
}

export type Sort = "start" | "severity" | "newest";
export function sortTasks(list: Task[], sort: Sort): Task[] {
  const startMs = (t: Task) => parseWhen(t.start)?.date.getTime() ?? Infinity;
  return [...list].sort((a, b) => {
    if (sort === "severity") return b.severity - a.severity || startMs(a) - startMs(b);
    if (sort === "newest") return (b.created_at ?? 0) - (a.created_at ?? 0);
    return startMs(a) - startMs(b) || b.severity - a.severity;
  });
}

/** "You'll get a reminder at 10:00 today." */
export function reminderHint(start: string | undefined, mins: number | null | undefined, now: Date): string {
  const s = parseWhen(start);
  if (!s) return "Set a start time to get a reminder";
  if (mins == null) return "No reminder";
  if (mins === 0 && !s.allDay) return `You'll be reminded when it starts.`;
  const base = s.allDay ? new Date(s.date.getFullYear(), s.date.getMonth(), s.date.getDate(), 9) : s.date;
  const at = new Date(base.getTime() - mins * 60_000);
  if (at.getTime() <= now.getTime()) return "That reminder time has already passed";
  const day = dayLabel(at, now);
  return `You'll get a reminder at ${hm(at)} ${day === "Today" || day === "Tomorrow" ? day.toLowerCase() : `on ${day}`}.`;
}
