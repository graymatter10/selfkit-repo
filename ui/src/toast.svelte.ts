// One undo toast at a time ("Done · Call the landlord   Undo").
type T = { id: number; text: string; undo?: () => void | Promise<void> };
let seq = 0;
let timer: ReturnType<typeof setTimeout> | undefined;
export const toast = $state<{ current: T | null }>({ current: null });

export function showToast(text: string, undo?: T["undo"], ms = 5000) {
  clearTimeout(timer);
  toast.current = { id: ++seq, text, undo };
  timer = setTimeout(() => (toast.current = null), ms);
}

export async function runUndo() {
  const u = toast.current?.undo;
  toast.current = null;
  // A failed undo used to vanish as an unhandled rejection: the toast closed and
  // nothing changed, with no message (C40).
  try {
    await u?.();
  } catch (e) {
    showToast(`Couldn't undo: ${String(e).replace(/^Error:\s*/, "")}`);
  }
}
