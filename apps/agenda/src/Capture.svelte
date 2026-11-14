<script lang="ts">
  // Quick-capture popup (global shortcut): title, severity, start, reminder. Monochrome.
  import { api } from "./lib/api";
  import { app, createTask, defaultReminder, defaultSeverity } from "./lib/state.svelte";
  import { SEVERITY, addDays, fmtMin, fmtWhen, startOfDay } from "./lib/time";
  import Seg from "./components/Seg.svelte";

  // Only this popup window is transparent (a global <style> here would leak into the main window).
  document.documentElement.style.background = document.body.style.background = "transparent";
  api.prefs().then((p) => {
    app.prefs = p ?? {};
    if (p?.theme && p.theme !== "system") document.documentElement.dataset.theme = p.theme;
    severity = defaultSeverity();
    reminder = defaultReminder();
  });
  let shortcut = $state("");
  api.settings().then((s) => (shortcut = s.shortcuts.add_task ?? ""));

  let title = $state("");
  let severity = $state(3);
  let day = $state<"none" | "today" | "tomorrow">("none");
  let time = $state("09:00");
  let reminder = $state<number | null>(30);
  let error = $state("");
  let el: HTMLInputElement | undefined = $state();
  $effect(() => void el?.focus());

  const close = () => api.closeCapture();
  const cycle = [null, 0, 5, 10, 15, 30, 60, 120];

  let saving = false;
  async function save(e: SubmitEvent) {
    e.preventDefault();
    if (saving) return; // held Enter / double click used to add the task several times
    const t = (el?.value ?? title).trim();
    if (!t) return void (error = "Give the task a title.");
    let start: string | undefined;
    if (day !== "none") {
      const [h, m] = (time || "09:00").split(":").map(Number);
      const d = addDays(startOfDay(new Date()), day === "tomorrow" ? 1 : 0);
      d.setHours(h, m);
      start = fmtWhen(d, false);
    }
    saving = true;
    try {
      await createTask({ title: t, severity, start, reminder_min: start ? (reminder ?? undefined) : undefined });
      close();
    } catch (err) {
      error = String(err);
      saving = false;
    }
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && close()} onblur={close} />
<form onsubmit={save} aria-label="New task" class="flex h-screen flex-col gap-3.5 overflow-hidden rounded-[14px] bg-bg px-4 py-3.5 ring-1 ring-sep">
  <div class="grid grid-cols-[1fr_auto_1fr] items-center">
    <button type="button" class="press justify-self-start text-[15px] text-accent" onclick={close}>Cancel</button>
    <span class="text-[15px] font-semibold">New Task</span>
    <button type="submit" class="press justify-self-end text-[15px] font-semibold {title.trim() ? 'text-accent' : 'text-faint'}">Add</button>
  </div>
  <label class="sr" for="cap">Title</label>
  <input id="cap" bind:this={el} bind:value={title} placeholder="Title" autocomplete="off" class="h-11 rounded-[10px] bg-card px-3.5 text-[17px] outline-none" />
  <Seg label="Severity" small items={[1, 2, 3, 4, 5].map((n) => ({ id: n, name: SEVERITY[n] }))} value={severity} onchange={(v) => (severity = v)} />
  <Seg label="Start" small items={[{ id: "none", name: "No date" }, { id: "today", name: "Today" }, { id: "tomorrow", name: "Tomorrow" }]} value={day} onchange={(v) => (day = v as typeof day)} />
  {#if day !== "none"}
    <div class="flex items-center gap-2 text-[15px]">
      <label class="flex items-center gap-2 text-muted">At <input type="time" bind:value={time} class="h-8 rounded-lg bg-card px-2 text-ink" /></label>
      <button type="button" class="press ml-auto h-8 rounded-lg bg-card px-3 text-ink2" onclick={() => (reminder = cycle[(cycle.indexOf(reminder) + 1) % cycle.length])}>
        {reminder == null ? "No reminder" : reminder === 0 ? "Remind at start" : `Remind ${fmtMin(reminder)} before`}</button>
    </div>
  {/if}
  {#if error}<p role="alert" class="m-0 text-[13px] text-danger">{error}</p>{/if}
  <span class="mt-auto text-[12px] text-muted">Enter adds · Esc closes{shortcut ? ` · ${shortcut}` : ""}</span>
</form>
