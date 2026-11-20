<script lang="ts">
  // Settings (iOS grouped sheet): appearance, defaults for new tasks, desktop shortcut.
  import { SEVERITY, fmtMin } from "../lib/time";
  import { app, breakMin, defaultReminder, defaultSeverity, focusMin, refresh, savePrefs, saveSettings } from "../lib/state.svelte";
  import { api } from "../lib/api";
  import Seg from "./Seg.svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import { syncApi } from "../../../../ui/src/sync/api";
  import LockSettings from "../../../../ui/src/lock/LockSettings.svelte"; // App lock section
  let { desktop = false }: { desktop?: boolean } = $props();
  let version = $state("");
  getVersion().then((v) => (version = v)).catch(() => {}); // real version in the footer (L11)
  let syncLine = $state("");
  $effect(() => { if (app.settingsOpen && !app.syncOpen) syncApi.status().then((st) => (syncLine = st.conflicts ? `${st.conflicts} to review` : st.headline.split(" · ")[0])).catch(() => (syncLine = "")); });
  let recording = $state(false);
  let shortcutError = $state("");
  const set = (k: "theme" | "reminder" | "severity" | "focus_min" | "break_min", v: unknown) => savePrefs({ ...app.prefs, [k]: v });

  // .ics export / import
  let icsMsg = $state("");
  let icsErr = $state(false);
  let includeDone = $state(true);
  let importEl: HTMLInputElement | undefined = $state();
  type Files = { saveDownload(name: string, mime: string, text: string): string };
  async function exportIcs() {
    icsErr = false;
    try {
      const text = await api.icsExport(includeDone);
      const d = new Date();
      const name = `agenda-${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}.ics`;
      const files = (window as unknown as { AgendaFiles?: Files }).AgendaFiles;
      const path = files ? files.saveDownload(name, "text/calendar", text) : await api.saveExport(name, text);
      if (!path) throw new Error("Could not save the file.");
      const n = (text.match(/BEGIN:VTODO/g) ?? []).length;
      icsMsg = `Exported ${n} task${n === 1 ? "" : "s"} to ${path}`;
    } catch (e) {
      icsErr = true;
      icsMsg = String(e).replace(/^Error:\s*/, "");
    }
  }
  async function importIcs(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const f = input.files?.[0];
    input.value = "";
    if (!f) return;
    icsErr = false;
    try {
      const r = await api.icsImport(await f.text());
      icsMsg = `Imported ${f.name}: ${r.added} new, ${r.updated} updated, ${r.unchanged} unchanged${r.kept ? `, ${r.kept} kept (newer here)` : ""}${r.skipped ? `, ${r.skipped} skipped` : ""}.`;
      await refresh();
    } catch (err) {
      icsErr = true;
      icsMsg = String(err).replace(/^Error:\s*/, "").replace(/^invalid format:\s*/i, "");
    }
  }
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
        <div class="overflow-hidden rounded-xl bg-card">
          <button type="button" class="hl flex w-full items-center pl-4 text-left text-[17px]" onclick={() => (app.syncOpen = true)}>
            <span class="flex flex-1 items-center gap-3 py-[11px] pr-4"><span class="flex-1">Sync</span><span class="truncate text-[15px] text-muted">{syncLine}</span>
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="var(--faint)" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9 6l6 6-6 6" /></svg></span></button>
        </div>

        <LockSettings phone={!desktop} appName="Agenda" />

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
          <span class="px-4 text-[13px] text-muted">Used for new tasks that have a start time.{#if /Android/i.test(navigator.userAgent)} Some phones (e.g. ColorOS) line alarms up, so a reminder can come up to about 2 minutes late.{/if}</span>
        </div>

        <div class="flex flex-col gap-2">
          <span class="px-4 text-[13px] text-muted uppercase">Default severity</span>
          <Seg label="Default severity" small items={[1, 2, 3, 4, 5].map((n) => ({ id: n, name: SEVERITY[n] }))} value={defaultSeverity()} onchange={(v) => set("severity", v)} />
        </div>

        <div class="flex flex-col gap-2">
          <span class="px-4 text-[13px] text-muted uppercase">Focus timer</span>
          <div class="flex flex-col gap-3 rounded-xl bg-card px-4 py-3">
            <div class="flex items-center gap-3"><span class="w-14 text-[15px] text-muted">Focus</span>
              <div class="flex-1"><Seg label="Focus length" small items={[15, 25, 45, 50].map((n) => ({ id: n, name: `${n} min` }))} value={focusMin()} onchange={(v) => set("focus_min", v)} /></div></div>
            <div class="flex items-center gap-3"><span class="w-14 text-[15px] text-muted">Break</span>
              <div class="flex-1"><Seg label="Break length" small items={[5, 10, 15].map((n) => ({ id: n, name: `${n} min` }))} value={breakMin()} onchange={(v) => set("break_min", v)} /></div></div>
          </div>
          <span class="px-4 text-[13px] text-muted">Start Focus from a task. You get a notification when it ends.</span>
        </div>

        <div class="flex flex-col gap-2">
          <span class="px-4 text-[13px] text-muted uppercase">Calendar file (.ics)</span>
          <div class="overflow-hidden rounded-xl bg-card">
            <label class="flex min-h-11 items-center justify-between border-b border-sep pr-4 pl-4 text-[17px]">Include completed
              <input type="checkbox" bind:checked={includeDone} class="sr" />
              <span aria-hidden="true" class="relative h-[31px] w-[51px] rounded-full transition-colors {includeDone ? 'bg-accent' : 'bg-fill'}"><span class="absolute top-[2px] h-[27px] w-[27px] rounded-full bg-white shadow transition-transform {includeDone ? 'translate-x-[22px]' : 'translate-x-[2px]'}"></span></span>
            </label>
            <button type="button" class="hl w-full border-b border-sep py-[11px] pl-4 text-left text-[17px] text-accent" onclick={exportIcs}>Export Tasks…</button>
            <button type="button" class="hl w-full py-[11px] pl-4 text-left text-[17px] text-accent" onclick={() => importEl?.click()}>Import .ics File…</button>
          </div>
          <input bind:this={importEl} type="file" accept="text/calendar,.ics,application/ics" class="sr" tabindex="-1" aria-hidden="true" onchange={importIcs} />
          <span class="px-4 text-[13px] break-words {icsErr ? 'text-danger' : 'text-muted'}" role={icsMsg ? "status" : undefined}>{icsMsg || "Tasks as VTODO for other calendar and task apps. Importing the same file again updates instead of duplicating."}</span>
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
        <p class="m-0 text-center text-[13px] text-muted">Agenda {version} · your data stays on this device and is encrypted when it syncs.</p>
      </div>
    </div>
  </div>
{/if}
