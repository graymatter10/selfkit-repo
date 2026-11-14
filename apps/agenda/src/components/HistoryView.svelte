<script lang="ts">
  // Completed-task history (every completion, also of repeating tasks) and the
  // focus-session log, grouped by day, newest first.
  import { api, type Completion, type FocusLog, type Task } from "../lib/api";
  import { app } from "../lib/state.svelte";
  import { dayLabel, hm, shortDay } from "../lib/time";
  import Seg from "./Seg.svelte";

  let { onopen }: { onopen: (t: Task) => void } = $props();
  let tab = $state<"tasks" | "focus">("tasks");
  let hist = $state<Completion[]>([]);
  let focus = $state<FocusLog[]>([]);
  let shown = $state(60);
  $effect(() => {
    void app.tasks;
    void app.focus;
    api.history().then((h) => (hist = h));
    api.focusSessions().then((f) => (focus = f));
  });

  function byDay<T>(items: T[], at: (x: T) => number) {
    const out: { label: string; items: T[] }[] = [];
    for (const x of items) {
      const d = new Date(at(x));
      const l = dayLabel(d, app.now);
      const label = l === "Today" || l === "Yesterday" ? `${l} · ${shortDay(d)}` : `${shortDay(d)} ${d.getFullYear() !== app.now.getFullYear() ? d.getFullYear() : ""}`.trim();
      if (out.at(-1)?.label !== label) out.push({ label, items: [] });
      out.at(-1)!.items.push(x);
    }
    return out;
  }
  const filtered = $derived(app.tag ? hist.filter((c) => c.tags?.includes(app.tag!)) : hist);
  const days = $derived(byDay(filtered.slice(0, shown), (c) => c.done_at));
  const fdays = $derived(byDay(focus.slice(0, shown), (f) => f.start));
  const more = $derived((tab === "tasks" ? filtered.length : focus.length) > shown);
  const openTask = (id?: string) => {
    const t = id && app.tasks.find((x) => x.id === id);
    if (t) onopen(t);
  };
</script>

<div class="flex flex-col gap-5">
  <Seg label="History" items={[{ id: "tasks", name: "Completed" }, { id: "focus", name: "Focus Sessions" }]} value={tab} onchange={(v) => ((tab = v), (shown = 60))} />
  {#if tab === "tasks"}
    {#each days as d (d.label)}
      <section aria-label={d.label}>
        <h2 class="m-0 flex justify-between px-4 pb-1.5 text-[13px] font-normal tracking-wide text-muted uppercase"><span>{d.label}</span><span>{d.items.length} done</span></h2>
        <div class="overflow-hidden rounded-xl bg-card">
          {#each d.items as c, i (c.id)}
            <button type="button" class="hl flex w-full items-center gap-3 pl-4 text-left" onclick={() => openTask(c.task)}>
              <span class="flex h-[22px] w-[22px] shrink-0 items-center justify-center rounded-full bg-accent text-on-accent"><svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg></span>
              <span class="flex min-w-0 flex-1 flex-col gap-0.5 py-[10px] pr-4 {i === d.items.length - 1 ? '' : 'border-b border-sep'}">
                <span class="truncate text-[17px]">{c.title}</span>
                <span class="text-[14px] text-muted">{hm(new Date(c.done_at))}{c.on_time === true ? " · on time" : c.on_time === false ? " · late" : ""}{c.tags?.length ? " · " + c.tags.map((g) => "#" + g).join(" ") : ""}</span>
              </span>
            </button>
          {/each}
        </div>
      </section>
    {:else}
      <p class="pt-16 text-center text-[17px] text-muted">Completed tasks will show up here.</p>
    {/each}
  {:else}
    {#each fdays as d (d.label)}
      <section aria-label={d.label}>
        <h2 class="m-0 flex justify-between px-4 pb-1.5 text-[13px] font-normal tracking-wide text-muted uppercase"><span>{d.label}</span>
          <span>{d.items.filter((f) => f.kind === "focus").reduce((a, f) => a + f.minutes, 0)} min focus</span></h2>
        <div class="overflow-hidden rounded-xl bg-card">
          {#each d.items as f, i (f.id)}
            <button type="button" class="hl flex w-full items-center pl-4 text-left" onclick={() => openTask(f.task)}>
              <span class="flex min-w-0 flex-1 items-center gap-3 py-[11px] pr-4 {i === d.items.length - 1 ? '' : 'border-b border-sep'}">
                <span class="w-12 shrink-0 text-[15px] text-muted tabular-nums">{hm(new Date(f.start))}</span>
                <span class="min-w-0 flex-1 truncate text-[17px] {f.kind === 'break' ? 'text-muted' : ''}">{f.kind === "break" ? "Break" : f.title}</span>
                <span class="shrink-0 text-[15px] text-muted tabular-nums">{f.minutes} min{f.completed ? "" : " · stopped"}</span>
              </span>
            </button>
          {/each}
        </div>
      </section>
    {:else}
      <p class="pt-16 text-center text-[17px] text-muted">Start Focus on a task to log a session.</p>
    {/each}
  {/if}
  {#if more}<button type="button" class="press self-center py-2 text-[15px] text-accent" onclick={() => (shown += 120)}>Show More</button>{/if}
</div>
