// Shared App lock UI ↔ Rust (selfkit-appsync feature "tauri", commands in src/lock_cmds.rs;
// rules in selfkit-core/src/applock.rs). Docs: docs/components/app-lock.md.
import { call } from "../ipc";

export type Kind = "pin" | "passphrase";

export interface LockView {
  enabled: boolean; locked: boolean; kind: Kind; pin_len: number; timeout_secs: number;
  biometric: boolean; hide_in_switcher: boolean; failed: number; retry_in_ms: number;
  platform: "android" | "desktop"; can_reset: boolean; test_build: boolean;
  /** Full path of this app's lock file (desktop "Forgot PIN?" hint). */
  lock_file: string;
  /** Taken under Rust's lock mutex: a higher seq is a later state (C97). */
  seq: number;
}
export type Attempt =
  | { result: "ok" }
  | { result: "wrong"; failed: number; retry_in_ms: number }
  | { result: "wait"; retry_in_ms: number }
  | { result: "not_set" };
export type AttemptView = Attempt & { lock: LockView };

export const lockApi = {
  status: () => call<LockView>("lock_status"),
  unlock: (secret: string) => call<AttemptView>("lock_unlock", { secret }),
  unlockBiometric: () => call<LockView>("lock_unlock_biometric"),
  testUnlock: () => call<LockView>("lock_test_unlock"),
  enable: (kind: Kind, secret: string) => call<LockView>("lock_enable", { kind, secret }),
  change: (current: string, kind: Kind, secret: string) => call<AttemptView>("lock_change", { current, kind, secret }),
  disable: (current: string) => call<AttemptView>("lock_disable", { current }),
  setOptions: (timeout_secs: number, biometric: boolean, hide_in_switcher: boolean) =>
    call<LockView>("lock_set_options", { timeoutSecs: timeout_secs, biometric, hideInSwitcher: hide_in_switcher }),
  away: (ms: number) => call<LockView>("lock_away", { ms: Math.round(ms) }),
  now: () => call<LockView>("lock_now"),
  reset: (secret: string, recovery: boolean) => call<LockView>("lock_reset", { secret, recovery }),
};

/** Android bridge (SelfkitLock.kt in each app); absent on desktop. */
export interface LockBridge {
  takeAway(): number;
  biometricStatus(): "fingerprint" | "credential" | "none" | "unavailable";
  authenticate(title: string, subtitle: string): void;
  setSecure(on: boolean): void;
}
export const lockBridge = (): LockBridge | null => ((globalThis as unknown as { SelfkitLock?: LockBridge }).SelfkitLock ?? null);

/** Ask for fingerprint / face (falls back to the phone's screen lock). */
export function biometricPrompt(title: string, subtitle = ""): Promise<"ok" | "cancel" | string> {
  const b = lockBridge();
  if (!b) return Promise.resolve("error:unavailable");
  return new Promise((resolve) => {
    const fn = (e: Event) => { window.removeEventListener("selfkit-lock-auth", fn); resolve(String((e as CustomEvent).detail)); };
    window.addEventListener("selfkit-lock-auth", fn);
    b.authenticate(title, subtitle);
  });
}

/** Shared state so the settings section sees what the gate sees. */
export const lockState = $state<{ view: LockView | null }>({ view: null });

export const TIMEOUT_LABELS: [number, string][] = [[0, "Immediately"], [60, "1 min"], [300, "5 min"], [900, "15 min"]];

export function waitText(ms: number): string {
  const s = Math.ceil(ms / 1000);
  if (s < 60) return `${s} s`;
  const m = Math.floor(s / 60), r = s % 60;
  return r ? `${m}:${String(r).padStart(2, "0")}` : `${m} min`;
}

export const word = (k: Kind) => (k === "pin" ? "PIN" : "passphrase");

/** Message after a wrong try. */
export function wrongText(a: Attempt, k: Kind): string {
  if (a.result === "wait") return `Too many tries. Try again in ${waitText(a.retry_in_ms)}.`;
  if (a.result !== "wrong") return "";
  if (a.retry_in_ms > 0) return `Wrong ${word(k)}. Try again in ${waitText(a.retry_in_ms)}.`;
  const left = 4 - a.failed;
  return left > 0 && left <= 2 ? `Wrong ${word(k)}. ${left} more ${left === 1 ? "try" : "tries"} before a short wait.` : `Wrong ${word(k)}.`;
}
