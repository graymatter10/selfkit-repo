// Money and date formatting.
const cache = new Map<string, Intl.NumberFormat>();
// Under a bare C/POSIX locale WebKit formats without digit grouping; use en-US then.
export const LOCALE = /^[a-z]{2,3}(-[A-Za-z]{2,4})?$/.test(navigator.language) && !/posix/i.test(navigator.language) ? navigator.language : "en-US";

function nf(cur: string, digits: number) {
  const k = cur + digits;
  let f = cache.get(k);
  if (!f) {
    try {
      f = new Intl.NumberFormat(LOCALE, { style: "currency", currency: cur, currencyDisplay: "narrowSymbol", minimumFractionDigits: digits, maximumFractionDigits: digits });
    } catch {
      f = new Intl.NumberFormat(LOCALE, { minimumFractionDigits: digits, maximumFractionDigits: digits });
    }
    cache.set(k, f);
  }
  return f;
}

/** "$1,234.50"; whole amounts drop the cents ("$1,234") unless `cents`. */
export function money(n: number, cur: string, cents = false) {
  const digits = cents || Math.round(n * 100) % 100 !== 0 ? 2 : 0;
  // Some locales put a space between a leading symbol and the number ("$ 12"); keep it tight.
  return nf(cur, digits).format(n).replace(/^(-?[^\d\s-]{1,3})[\s\u00a0\u202f](?=\d)/, "$1");
}

/** Compact for charts: 1.2k, 3.4M. */
export function short(n: number) {
  const a = Math.abs(n);
  if (a >= 1e12) return (n / 1e12).toFixed(a >= 1e13 ? 0 : 1) + "T";
  if (a >= 1e9) return (n / 1e9).toFixed(a >= 1e10 ? 0 : 1) + "B";
  if (a >= 1e6) return (n / 1e6).toFixed(a >= 1e7 ? 0 : 1) + "M";
  if (a >= 1e3) return (n / 1e3).toFixed(a >= 1e4 ? 0 : 1) + "k";
  return Math.round(n).toString();
}

export function symbol(cur: string) {
  const p = nf(cur, 0).formatToParts(0).find((x) => x.type === "currency");
  return p?.value ?? cur;
}

export const pad = (n: number) => String(n).padStart(2, "0");
export const iso = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
export const todayIso = () => iso(new Date());
export const nowHm = () => {
  const d = new Date();
  return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
};
export const monthIso = (d = new Date()) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}`;
export const parse = (s: string) => {
  const [y, m, d] = s.split("-").map(Number);
  return new Date(y, (m || 1) - 1, d || 1);
};
export function addDays(s: string, n: number) {
  const d = parse(s);
  d.setDate(d.getDate() + n);
  return iso(d);
}
export function addMonths(month: string, n: number) {
  const d = parse(month + "-01");
  d.setMonth(d.getMonth() + n);
  return monthIso(d);
}
export function monthName(month: string, withYear = false) {
  const d = parse(month + "-01");
  const sameYear = d.getFullYear() === new Date().getFullYear();
  return d.toLocaleDateString(LOCALE, withYear || !sameYear ? { month: "long", year: "numeric" } : { month: "long" });
}
export function dayLabel(s: string) {
  const t = todayIso();
  if (s === t) return "Today";
  if (s === addDays(t, -1)) return "Yesterday";
  if (s === addDays(t, 1)) return "Tomorrow";
  const d = parse(s);
  const sameYear = d.getFullYear() === new Date().getFullYear();
  return d.toLocaleDateString(LOCALE, sameYear ? { weekday: "short", day: "numeric", month: "short" } : { day: "numeric", month: "short", year: "numeric" });
}
export function shortDate(s: string) {
  if (!s) return "";
  const d = parse(s);
  const sameYear = d.getFullYear() === new Date().getFullYear();
  return d.toLocaleDateString(LOCALE, sameYear ? { day: "numeric", month: "short" } : { day: "numeric", month: "short", year: "numeric" });
}
export function daysBetween(a: string, b: string) {
  return Math.round((parse(b).getTime() - parse(a).getTime()) / 86400000);
}

export const CURRENCIES = ["USD", "EUR", "GBP", "BDT", "INR", "PKR", "NPR", "LKR", "AED", "SAR", "QAR", "KWD", "MYR", "SGD", "JPY", "CNY", "KRW", "THB", "IDR", "PHP", "TRY", "CAD", "AUD", "NZD", "CHF", "SEK", "NOK", "DKK", "ZAR", "NGN", "KES", "EGP", "BRL", "MXN"];

/** "#tags" in a note become tags (lower-case, added to `tags` without duplicates; letters
 *  include combining marks, so Bangla "#অফিস" stays whole — L12); the
 *  note keeps the other words. Same rule on the add screen and in the entry editor (C33). */
export function splitNoteTags(note: string, tags: string[]): { note: string; tags: string[] } {
  const out = [...tags];
  const rest = note.replace(/(^|\s)#([\p{L}\p{M}\p{N}_-]+)/gu, (_, sp, t) => {
    const tag = t.toLowerCase();
    if (!out.includes(tag)) out.push(tag);
    return sp;
  }).replace(/[^\S\n]+/g, " ").trim(); // keep line breaks of the editor's multi-line note
  return { note: rest, tags: out };
}
