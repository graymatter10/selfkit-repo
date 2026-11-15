// Android-only bridge (MainActivity.kt → window.HisabNative). Everything is
// optional: on desktop these return false / [].
interface Native {
  setDark(d: boolean): void;
  takeLaunchAction(): string;
  minimize(): void;
}
const n = () => (window as unknown as { HisabNative?: Native }).HisabNative;

export const isAndroid = () => !!n();
/** "add" when launched from the home-screen widget (once). */
export const takeLaunchAction = () => n()?.takeLaunchAction?.() ?? "";
/** Send the app to the background (Back on the add screen). */
export const minimize = () => n()?.minimize?.();
