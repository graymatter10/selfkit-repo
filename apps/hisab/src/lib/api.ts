// Typed wrappers over the Rust commands (src-tauri/src/lib.rs).
import { call } from "@selfkit/ui";
import { invoke } from "@tauri-apps/api/core";

export type Kind = "expense" | "income" | "transfer";
export interface Split { category: string; amount: number }
export interface Tx {
  id: string; kind: Kind; amount: number; currency: string; date: string; time: string; account: string;
  to_account?: string; to_amount?: number; category: string; splits: Split[]; tags: string[]; note: string;
  photos: string[]; source: string; amount_main?: number; recurring?: string; created_at?: number;
}
export interface Account { id: string; name: string; kind: string; currency: string; opening: number; archived: boolean; order: number; balance: number; balance_main: number; explicit_currency?: boolean; entry_currency: string }
export interface Category { id: string; name: string; kind: "expense" | "income"; icon: string; color: string; budget: number; archived: boolean; order: number; usage: number; spent: number }
export interface Repayment { id: string; date: string; amount: number; note: string }
export interface Loan {
  id: string; direction: "lent" | "borrowed"; person: string; title: string; description: string; amount: number; currency: string;
  given: string; due: string; account: string; photos: string[]; repayments: Repayment[]; remind: boolean; remind_days: number; remind_time: string;
  repaid: number; remaining: number; remaining_main: number; status: "ongoing" | "due_soon" | "overdue" | "finished"; days_left: number | null;
}
export interface Contribution { id: string; date: string; amount: number; note: string }
export interface Goal { id: string; name: string; target: number; currency: string; deadline: string; contributions: Contribution[]; archived: boolean; saved: number }
export interface Recurring {
  id: string; name: string; freq: "daily" | "weekly" | "monthly" | "yearly"; interval: number; start: string; next: string; end: string;
  active: boolean; subscription: boolean; template: Partial<Tx>; monthly: number;
}
export interface CatAmount { id: string; name: string; color: string; icon: string; archived: boolean; amount: number }
export interface Overview {
  month: string; main: string; spent: number; income: number; balance: number; budget: number; remaining: number | null;
  prev_spent: number; days_in_month: number; days_elapsed: number; net_worth: number; by_category: CatAmount[];
}
export interface Rates { base: string; rates: Record<string, number>; manual: Record<string, number>; fetched_at: number }
export interface Snapshot {
  main: string; today: string; accounts: Account[]; categories: Category[]; loans: Loan[]; goals: Goal[]; recurring: Recurring[];
  overview: Overview; recent: Tx[]; rates: Rates; tags: string[]; missing_rates: string[];
  stale_accounts: MainChange["used"];
}
export interface Trends {
  month: string; main: string; daily: number[]; daily_prev: number[]; by_category_month: CatAmount[]; income_by_category_month: CatAmount[];
  by_category_year: CatAmount[]; months: { month: string; spent: number; income: number }[]; years: { year: string; spent: number; income: number }[];
  heatmap: { date: string; amount: number }[]; networth: { month: string; value: number }[];
  lifetime: { spent: number; income: number; count: number; first: string | null }; missing_rates: string[];
}
export interface Filter { q?: string; kind?: string; category?: string; account?: string; tag?: string; from?: string; to?: string; min?: number; max?: number; limit?: number }
// CSV import (src-tauri/src/csvimport.rs). Column indexes; null = not in the file.
export interface CsvMapping {
  date: number | null; time: number | null; amount: number | null; debit: number | null; credit: number | null; description: number | null;
  category: number | null; account: number | null; currency: number | null; kind: number | null; tags: number | null; to_account: number | null; to_amount: number | null;
  date_format: string; hisab: boolean; positive_is_expense: boolean;
}
export interface CsvRow {
  line: number; date: string; time: string; kind: string; amount: number; currency: string; note: string; category: string; category_name: string;
  account: string; to_account: string; to_amount: number; tags: string[]; duplicate: boolean; error: string;
}
export interface CsvPreview {
  headers: string[]; sample: string[][]; delimiter: string; count: number; mapping: CsvMapping;
  preview: { rows: CsvRow[]; ok: number; duplicates: number; errors: number };
}
export interface Prefs { theme?: "system" | "light" | "dark"; rank?: "manual" | "used"; account?: string; notify?: boolean }
export interface MainChange { old: string; new: string; switched: string[]; used: { id: string; name: string; currency: string; entries: number }[] }
export interface Settings { day_start_min: number; main_currency: string; theme: unknown; shortcuts: Record<string, string> }

export type Coll = "accounts" | "categories" | "transactions" | "recurring" | "loans" | "goals";

export const api = {
  boot: () => call<{ main: string; today: string; prefs: Prefs; generated: number; draft: object | null }>("boot"),
  startupNotice: () => call<string | null>("startup_notice"),
  draftSet: (draft: object) => call<void>("draft_set", { draft }),
  snapshot: (month: string) => call<Snapshot>("snapshot", { month }),
  transactions: (filter: Filter) => call<Tx[]>("transactions", { filter }),
  trends: (month: string) => call<Trends>("trends", { month }),
  /** `draft`: add-screen draft to store in the same call (cleared after a Save). */
  save: <T>(collection: Coll, data: object, id?: string, draft?: object) => call<T>("save", { collection, id: id ?? null, data, draft: draft ?? null }),
  get: <T>(id: string) => call<T | null>("get", { id }),
  remove: (id: string) => call<void>("remove", { id }),
  restore: (id: string) => call<void>("restore", { id }),
  generateRecurring: () => call<number>("generate_recurring"),
  csvPreview: (text: string, mapping: CsvMapping | null, account: string) => call<CsvPreview>("csv_preview", { text, mapping, account }),
  csvImport: (rows: CsvRow[]) => call<string[]>("csv_import", { rows }),
  csvImportUndo: (ids: string[]) => call<number>("csv_import_undo", { ids }),
  ratesSave: (base: string, rates: Record<string, number>) => call<Rates>("rates_save", { base, rates }),
  ratesManual: (currency: string, rate: number | null) => call<Rates>("rates_manual", { currency, rate }),
  exportFile: (kind: "csv" | "pdf", from: string, to: string) => call<string>("export_file", { kind, from, to }),
  photoSave: (bytes: Uint8Array) => invoke<string>("photo_save", bytes),
  photoGet: (id: string) => invoke<ArrayBuffer>("photo_get", { id }),
  photoExport: (id: string) => invoke<string>("photo_export", { id }),
  prefsSet: (prefs: Prefs) => call<void>("prefs_set", { prefs }),
  settingsGet: () => call<Settings>("settings_get"),
  settingsSet: (settings: Settings) => call<void>("settings_set", { settings }),
  /** Change the global main currency; returns used accounts still in the old one. */
  mainCurrencySet: (code: string) => call<MainChange>("main_currency_set", { code }),
  accountRelabel: (id: string, currency: string) => call<number>("account_relabel", { id, currency }),
  accountKeepCurrency: (id: string) => call<void>("account_keep_currency", { id }),
  closeCapture: () => call<void>("close_capture"),
  reschedule: () => call<void>("reschedule"),
  devHooks: () => call<string | null>("dev_hooks"),
};
