<script lang="ts">
  // Settings (iOS grouped sheet): appearance, defaults for new tasks, desktop shortcut.
  import { SEVERITY, fmtMin } from "../lib/time";
  import { app, defaultReminder, defaultSeverity, savePrefs, saveSettings } from "../lib/state.svelte";
  import Seg from "./Seg.svelte";
  let { desktop = false }: { desktop?: boolean } = $props();
  let recording = $state(false);
  let shortcutError = $state("");
  const set = (k: "theme" | "reminder" | "severity", v: unknown) => savePrefs({ ...app.prefs, [k]: v });
  const REMINDERS: (number | null)[] = [null, 0, 5, 10, 15, 30, 60, 120];
  const remName = (m: number | null) => (m == null ? "None" : m === 0 ? "At start" : `${fmtMin(m)} before`);

  // Android back closes the sheet.
  $effect(() => {
    if (!app.settingsOpen) return;
    const prev = app.closeGuard;
    app.closeGuard = () => ((app.settingsOpen = false), true);
    return () => (app.closeGuard = prev);
  });

  function record(e: KeyboardEvent) {
    if (!recording || !app.settings) return;
    e.preventDefault();
    if (e.key === "Escape") return void (recording = false);
    if (["Shift", "Control", "Alt", "Meta"].includes(e.key)) return;
    const mods = [e.metaKey && "Super", e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift"].filter(Boolean);
    if (!mods.length) return void (shortcutError = "Use at least one modifier key (Super, Ctrl or Alt).");
    const key = e.code.startsWith("Key") ? e.code.slice(3) : e.code.startsWith("Digit") ? e.code.slice(5) : e.code;
    const accel = [...mods, key].join("+");
    const clash = Object.entries(app.settings.shortcuts).find(([a, k]) => a !== "add_task" && k.toLowerCase() === accel.toLowerCase());
    if (clash) return void (shortcutError = `${accel} is already used by ${clash[0].replace("_", " ")}.`);
    recording = false;
    shortcutError = "";
    saveSettings({ ...app.settings, shortcuts: { ...app.settings.shortcuts, add_task: accel } });
  }
</script>

<svelte:window onkeydown={record} />
{#if app.settingsOpen}
  <div class="fixed inset-0 z-50 flex {desktop ? 'items-center justify-center' : 'items-end'}">
    <button type="button" aria-label="Close settings" class="fade absolute inset-0 cursor-default bg-scrim" onclick={() => (app.settingsOpen = false)}></button>
    <div role="dialog" aria-modal="true" aria-label="Settings" class="rise relative flex max-h-[92dvh] w-full flex-col overflow-hidden bg-bg {desktop ? 'max-w-[440px] rounded-[14px]' : 'rounded-t-[12px]'}">
      <div class="grid grid-cols-[1fr_auto_1fr] items-center px-4 py-3">
        <span></span><span class="text-[17px] font-semibold">Settings</span>
        <button type="button" class="press justify-self-end text-[17px] font-semibold text-accent" onclick={() => (app.settingsOpen = false)}>Done</button>
      </div>
      <div class="flex flex-col gap-[22px] overflow-y-auto *:shrink-0 px-4 pt-2 pb-[max(28px,env(safe-area-inset-bottom))]">
        <div class="flex flex-col gap-2">
          <span class="px-4 text-[13px] text-muted uppercase">Appearance</span>
          <Seg label="Appearance" items={[{ id: "system", name: "System" }, { id: "light", name: "Light" }, { id: "dark", name: "Dark" }]}
            value={app.prefs.theme ?? "system"} onchange={(v) => set("theme", v)} />
        </div>

        <div class="flex flex-col gap-2">
          <span class="px-4 text-[13px] text-muted uppercase">Default reminder</span>
          <div class="overflow-hidden rounded-xl bg-card">
            {#each REMINDERS as m, i}
              <button type="button" role="menuitemradio" aria-checked={defaultReminder() === m} onclick={() => set("reminder", m)} class="hl flex w-full items-center pl-4 text-left text-[17px]">
                <span class="flex flex-1 items-center py-[11px] pr-4 {i === REMINDERS.length - 1 ? '' : 'border-b border-sep'}"><span class="flex-1">{remName(m)}</span>
                  {#if defaultReminder() === m}<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="var(--accent)" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg>{/if}</span>
              </button>
            {/each}
          </div>
          <span class="px-4 text-[13px] text-muted">Used for new tasks that have a start time.</span>
        </div>

        <div class="flex flex-col gap-2">
          <span class="px-4 text-[13px] text-muted uppercase">Default severity</span>
          <Seg label="Default severity" small items={[1, 2, 3, 4, 5].map((n) => ({ id: n, name: SEVERITY[n] }))} value={defaultSeverity()} onchange={(v) => set("severity", v)} />
        </div>

        {#if desktop && app.settings}
          <div class="flex flex-col gap-2">
            <span class="px-4 text-[13px] text-muted uppercase">Quick capture</span>
            <div class="overflow-hidden rounded-xl bg-card">
              <button type="button" class="hl flex w-full items-center pl-4 text-left text-[17px]" onclick={() => ((recording = true), (shortcutError = ""))}>
                <span class="flex flex-1 items-center border-b border-sep py-[11px] pr-4"><span class="flex-1">Shortcut</span>
                  <span class={recording ? "text-accent" : "text-muted"}>{recording ? "Press keys…" : app.settings.shortcuts.add_task || "Off"}</span></span></button>
              <button type="button" class="hl w-full py-[11px] pl-4 text-left text-[17px] text-danger" onclick={() => app.settings && saveSettings({ ...app.settings, shortcuts: { ...app.settings.shortcuts, add_task: "" } })}>Turn Off Shortcut</button>
            </div>
            <span class="px-4 text-[13px] {shortcutError ? 'text-danger' : 'text-muted'}" role={shortcutError ? "alert" : undefined}>{shortcutError || "Opens a small New Task window from anywhere."}</span>
          </div>
        {/if}
        <p class="m-0 text-center text-[13px] text-muted">Agenda 0.3 · your data stays on this device and is encrypted when it syncs.</p>
      </div>
    </div>
  </div>
{/if}
