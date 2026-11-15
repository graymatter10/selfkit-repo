// Shared Sync UI ↔ Rust (selfkit-appsync feature "tauri", commands in src/tauri_cmds.rs).
// Every app registers the same command names, so this file works in all of them.
import { call } from "../ipc";

export type Owner = "app" | "companion" | "agent";
export type TransportMode = "auto" | "lan_only" | "server_only";
export type Transport = "lan" | "server" | "agent";

export interface SyncSettings {
  owner: Owner; transport: TransportMode; auto_sync: boolean; background_minutes: number;
  lan_enabled: boolean; lan_peers: string[]; lan_discovery_port: number; lan_discovery_ms: number; device_name: string | null;
}
export interface SeenPeer { device_id: string; name: string; addr: string; last_seen: number; last_sync: number | null }
export interface PassReport { pulled: number; pushed: number; conflicts: number; undecryptable: number; photos_up: number; photos_down: number; photos_waiting: number }
export interface SyncOutcome {
  ok: boolean; ran: boolean; transport: Transport | null; peers: string[]; report: PassReport;
  error: string | null; detail: string | null; note: string | null; started_at: number; finished_at: number;
}
export interface SyncStatus {
  app: string; device_id: string; device_name: string; owner: Owner; running: boolean; unlocked: boolean; test_key: boolean;
  encryption_set_up: boolean; server_configured: boolean; photos_configured: boolean;
  last_attempt_at: number | null; last_ok_at: number | null; last_transport: Transport | null; last_peers: string[];
  last_error: string | null; last_note: string | null; pending: number; conflicts: number; peers: SeenPeer[];
  headline: string; next_auto_at: number | null; platform: "android" | "desktop"; settings: SyncSettings;
}
export interface ServerView {
  configured: boolean; pg_url: string; has_password: boolean; s3_endpoint: string; s3_bucket: string; s3_region: string;
  s3_access_key: string; has_s3_secret: boolean;
}
export interface ServerForm {
  pg_url: string; pg_password: string; s3_endpoint: string; s3_bucket: string; s3_region: string; s3_access_key: string; s3_secret: string;
}
export interface Version { device_id: string; updated_at: number; tz_offset: number; deleted: boolean; data: Record<string, unknown> }
export interface DiffLine { tag: "Same" | "Local" | "Remote"; text: string }
export interface FieldView { key: string; a: unknown; b: unknown; lines: DiffLine[] | null }
export interface ConflictView {
  store: string; id: string; collection: string; source: "server" | "peer"; a: Version; b: Version;
  fields: FieldView[]; same: string[]; delete_vs_edit: boolean;
}
export type Choice =
  | { choice: "keep_a" }
  | { choice: "keep_b" }
  | { choice: "merge"; fields: Record<string, "a" | "b">; lines: Record<string, number[]> };

export const syncApi = {
  status: () => call<SyncStatus>("sync_status"),
  now: (trigger: "manual" | "open" = "manual") => call<SyncOutcome>("sync_now", { trigger }),
  saveSettings: (settings: SyncSettings) => call<SyncSettings>("sync_settings_set", { settings }),
  server: () => call<ServerView>("sync_server_get"),
  testServer: (form: ServerForm) => call<void>("sync_server_test", { form }),
  saveServer: (form: ServerForm) => call<ServerView>("sync_server_set", { form }),
  setupEncryption: (passphrase: string) => call<string>("sync_encryption_setup", { passphrase }),
  unlock: (secret: string, recovery: boolean) => call<void>("sync_unlock", { secret, recovery }),
  changePassphrase: (old: string, next: string) => call<void>("sync_change_passphrase", { old, new: next }),
  conflicts: () => call<ConflictView[]>("sync_conflicts"),
  resolve: (store: string, id: string, choice: Choice) => call<void>("sync_resolve", { store, id, choice }),
  log: () => call<SyncOutcome[]>("sync_log"),
};

/** Android JS bridge from android-plugins/selfkit-sync (SelfkitSyncBridge); absent on desktop. */
interface Bridge { needsBatteryTip(): boolean; openBatterySettings(): void; schedule(): void; runNow(): void }
export const bridge = (): Bridge | null => ((globalThis as unknown as { SelfkitSync?: Bridge }).SelfkitSync ?? null);

export function ago(now: number, t: number | null | undefined): string {
  if (!t) return "never";
  const s = Math.max(0, Math.round((now - t) / 1000));
  if (s < 45) return "just now";
  if (s < 5400) return `${Math.max(1, Math.round(s / 60))} min ago`;
  if (s < 86400) return `${Math.round(s / 3600)} h ago`;
  if (s < 172800) return "yesterday";
  return `${Math.floor(s / 86400)} days ago`;
}

/** "note_before" -> "Note before" */
export const label = (k: string) => (k.charAt(0).toUpperCase() + k.slice(1)).replace(/_/g, " ");

export function show(v: unknown): string {
  if (v === undefined || v === null || v === "") return "—";
  if (typeof v === "boolean") return v ? "Yes" : "No";
  if (Array.isArray(v)) return v.length ? v.map((x) => (typeof x === "object" ? (x && ((x as Record<string, unknown>).title ?? (x as Record<string, unknown>).name ?? JSON.stringify(x))) : String(x))).join(", ") : "—";
  if (typeof v === "object") return JSON.stringify(v);
  return String(v);
}
