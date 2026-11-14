// Thin wrapper over Tauri IPC so components never import Tauri directly.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// On Android the webview can call commands before the Rust side has finished
// opening its database ("state not managed"). Wait and retry instead of
// failing — this caused "adding does nothing" on first launch.
const NOT_READY = /state not managed/i;
const RETRY_MS = 100;
const RETRY_MAX = 100; // ~10 s

export async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  for (let i = 0; ; i++) {
    try {
      return await invoke<T>(cmd, args);
    } catch (e) {
      if (i >= RETRY_MAX || !NOT_READY.test(String(e))) throw e;
      await new Promise((r) => setTimeout(r, RETRY_MS));
    }
  }
}

export function on<T>(event: string, fn: (payload: T) => void): Promise<UnlistenFn> {
  return listen<T>(event, (e) => fn(e.payload));
}
