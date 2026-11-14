<script lang="ts">
  // Focus (Pomodoro) timer: a slim bar while a session runs or has just ended,
  // and a sheet with the big countdown. The end is also a system notification
  // (Android: scheduled alarm; desktop: the Rust timer thread).
  import { Icon } from "@selfkit/ui";
  import { app, breakMin, focusMin, startFocus, stopFocus } from "../lib/state.svelte";

  let { desktop = false, part }: { desktop?: boolean; part: "bar" | "sheet" } = $props();
  let now = $state(Date.now());
  $effect(() => {
    if (!app.focus) return;
    const t = setInterval(() => (now = Date.now()), 250);
    return () => clearInterval(t);
  });
  const left = $derived(app.focus ? Math.max(0, app.focus.end_ms - now) : 0);
  const mmss = $derived(`${String(Math.floor(left / 60000)).padStart(2, "0")}:${String(Math.floor((left % 60000) / 1000)).padStart(2, "0")}`);
  const frac = $derived(app.focus ? 1 - left / (app.focus.end_ms - app.focus.start_ms) : 0);
  const task = $derived(app.tasks.find((t) => t.id === (app.focus?.task ?? app.focusDone?.task)) ?? null);

  $effect(() => {
    if (part !== "sheet" || !app.focusOpen) return;
    const prev = app.closeGuard;
    app.closeGuard = () => ((app.focusOpen = false), true);
    return () => (app.closeGuard = prev);
  });
  const R = 92, C = 2 * Math.PI * R;
  async function next(kind: "focus" | "break") {
    await startFocus(kind === "focus" ? task : task, kind);
  }
</script>

{#if part === "bar" && (app.focus || app.focusDone)}
  <div class="fade flex items-center gap-3 rounded-[14px] bg-card py-1.5 pr-1.5 pl-4 shadow-[0_4px_20px_rgba(0,0,0,0.10)] ring-1 ring-sep">
    {#if app.focus}
      <button type="button" class="flex min-w-0 flex-1 items-center gap-3 text-left" onclick={() => (app.focusOpen = true)} aria-label="Open focus timer">
        <span class="h-2 w-2 shrink-0 rounded-full {app.focus.kind === 'break' ? 'bg-faint' : 'bg-accent'}"></span>
        <span class="text-[17px] font-semibold tabular-nums">{mmss}</span>
        <span class="min-w-0 flex-1 truncate text-[15px] text-muted">{app.focus.kind === "break" ? "Break" : app.focus.title}</span>
      </button>
      <button type="button" class="press h-9 rounded-[10px] bg-fill px-3 text-[15px] font-semibold" onclick={stopFocus}>Stop</button>
    {:else if app.focusDone}
      <span class="min-w-0 flex-1 truncate text-[15px]">{app.focusDone.kind === "break" ? "Break is over" : `Focus done · ${app.focusDone.planned_min} min`}</span>
      <button type="button" class="press h-9 rounded-[10px] bg-accent-soft px-3 text-[15px] font-semibold text-accent" onclick={() => next(app.focusDone?.kind === "break" ? "focus" : "break")}>
        {app.focusDone.kind === "break" ? `Focus ${focusMin()} min` : `Break ${breakMin()} min`}</button>
      <button type="button" aria-label="Dismiss" class="press flex h-9 w-9 items-center justify-center text-muted" onclick={() => (app.focusDone = null)}><Icon name="close" size={16} stroke={2.2} /></button>
    {/if}
  </div>
{/if}

{#if part === "sheet" && app.focusOpen}
  <div class="fixed inset-0 z-[65] flex {desktop ? 'items-center justify-center' : 'items-end'}">
    <button type="button" aria-label="Close focus timer" class="fade absolute inset-0 cursor-default bg-scrim" onclick={() => (app.focusOpen = false)}></button>
    <div role="dialog" aria-modal="true" aria-label="Focus" class="rise relative flex w-full flex-col items-center gap-5 bg-bg px-6 pt-3 pb-[max(28px,env(safe-area-inset-bottom))] {desktop ? 'max-w-[420px] rounded-[14px]' : 'rounded-t-[12px]'}">
      <div class="grid w-full grid-cols-[1fr_auto_1fr] items-center">
        <span></span><span class="text-[17px] font-semibold">{app.focus?.kind === "break" || (!app.focus && app.focusDone?.kind === "break") ? "Break" : "Focus"}</span>
        <button type="button" class="press justify-self-end py-2 text-[17px] font-semibold text-accent" onclick={() => (app.focusOpen = false)}>Done</button>
      </div>
      <div class="relative h-[220px] w-[220px]">
        <svg viewBox="0 0 220 220" class="h-full w-full -rotate-90" aria-hidden="true">
          <circle cx="110" cy="110" r={R} fill="none" stroke="var(--fill)" stroke-width="8" />
          <circle cx="110" cy="110" r={R} fill="none" stroke={app.focus?.kind === "break" ? "var(--muted)" : "var(--accent)"} stroke-width="8" stroke-linecap="round"
            stroke-dasharray={C} stroke-dashoffset={C * (1 - (app.focus ? frac : 1))} class="transition-[stroke-dashoffset] duration-300" />
        </svg>
        <div class="absolute inset-0 flex flex-col items-center justify-center gap-1">
          <span class="text-[48px] font-semibold tracking-[-1px] tabular-nums" role="timer">{app.focus ? mmss : "00:00"}</span>
          <span class="text-[15px] text-muted">{app.focus ? `${app.focus.planned_min} min session` : "Session complete"}</span>
        </div>
      </div>
      <p class="m-0 max-w-full truncate text-center text-[17px] text-ink2">{app.focus ? (app.focus.kind === "break" ? "Rest your eyes." : app.focus.title) : (task?.title ?? "")}</p>
      <div class="flex w-full flex-col gap-2">
        {#if app.focus}
          <button type="button" class="press h-12 w-full rounded-xl bg-card text-[17px] font-semibold text-danger" onclick={stopFocus}>Stop Session</button>
        {:else}
          <button type="button" class="press h-12 w-full rounded-xl bg-accent text-[17px] font-semibold text-on-accent" onclick={() => next("break")}>Take a {breakMin()}-min Break</button>
          <button type="button" class="press h-12 w-full rounded-xl bg-card text-[17px] font-semibold text-accent" onclick={() => next("focus")}>Focus {focusMin()} min{task ? ` on “${task.title.slice(0, 24)}${task.title.length > 24 ? "…" : ""}”` : ""}</button>
        {/if}
      </div>
    </div>
  </div>
{/if}
