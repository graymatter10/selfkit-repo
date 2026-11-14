// Applies the shared Selfkit theme setting to <html data-theme>, following
// the OS light/dark switch in System mode.
export type Theme = "calm_light" | "soft_dark" | "paper_mono";
export interface ThemeSetting {
  mode: { mode: "system" } | { mode: "fixed"; theme: Theme };
  light: Theme;
  dark: Theme;
}

const mq = typeof window !== "undefined" ? window.matchMedia("(prefers-color-scheme: dark)") : null;
let current: ThemeSetting | null = null;

export function effectiveTheme(t: ThemeSetting, osDark = mq?.matches ?? false): Theme {
  return t.mode.mode === "fixed" ? t.mode.theme : osDark ? t.dark : t.light;
}

function paint() {
  if (current) document.documentElement.dataset.theme = effectiveTheme(current);
}

export function applyTheme(t: ThemeSetting) {
  current = t;
  paint();
}

mq?.addEventListener("change", paint);

export const THEMES: { id: Theme; name: string }[] = [
  { id: "calm_light", name: "A · Calm Light" },
  { id: "soft_dark", name: "B · Soft Dark" },
  { id: "paper_mono", name: "D · Paper & Mono" },
];
