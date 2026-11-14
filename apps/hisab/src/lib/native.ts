// Android-only bridge (MainActivity.kt → window.HisabNative). Everything is
// optional: on desktop these return false / [].
interface Native {
  setDark(d: boolean): void;
  smsPermission(): boolean;
  requestSms(): void;
  readSms(sinceMs: number): string;
  takeQueued(): string;
  setSmsEnabled(on: boolean): void;
  takeLaunchAction(): string;
  minimize(): void;
}
const n = () => (window as unknown as { HisabNative?: Native }).HisabNative;

export const isAndroid = () => !!n();
export const smsAllowed = () => n()?.smsPermission() ?? false;
export const requestSms = () => n()?.requestSms();
export const setSmsEnabled = (on: boolean) => n()?.setSmsEnabled(on);
export interface RawSms { sender: string; body: string; at: number }
export function readSms(sinceMs: number): RawSms[] {
  try {
    return JSON.parse(n()?.readSms(sinceMs) ?? "[]");
  } catch {
    return [];
  }
}
export function takeQueued(): RawSms[] {
  try {
    return JSON.parse(n()?.takeQueued() ?? "[]");
  } catch {
    return [];
  }
}
/** "add" when launched from the home-screen widget (once). */
export const takeLaunchAction = () => n()?.takeLaunchAction?.() ?? "";
/** Send the app to the background (Back on the add screen). */
export const minimize = () => n()?.minimize?.();
