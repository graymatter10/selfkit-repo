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
  await u?.();
}
