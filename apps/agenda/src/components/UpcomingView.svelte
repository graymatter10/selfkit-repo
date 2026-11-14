<script lang="ts">
  // Upcoming: overdue first, then the next 14 days day by day (repeating tasks
  // show on each day they repeat), then a short "Later" count.
  import type { Task } from "../lib/api";
  import { app, tagged } from "../lib/state.svelte";
  import { addDays, dayLabel, hm, occurrences, parseWhen, shortDay, startOfDay, status, sortTasks } from "../lib/time";
  import TaskRow from "./TaskRow.svelte";

  let { desktop = false, onopen }: { desktop?: boolean; onopen: (t: Task) => void } = $props();
  const DAYS = 14;

  const data = $derived.by(() => {
    const today = startOfDay(app.now);
    const end = addDays(today, DAYS);
    const active = tagged().filter((t) => !t.done_at);
    const overdue = sortTasks(active.filter((t) => status(t, app.now).kind === "overdue"), "start");
    const days = Array.from({ length: DAYS }, (_, i) => ({ day: addDays(today, i), items: [] as { t: Task; first: boolean }[] }));
    let later = 0;
    for (const t of active) {
      const isOver = overdue.includes(t);
      for (const o of occurrences(t, today, end)) {
        if (isOver && o.first) continue;
        const i = Math.round((o.day.getTime() - today.getTime()) / 86_400_000);
        if (i >= 0 && i < DAYS) days[i].items.push({ t, first: o.first });
      }
      const s = parseWhen(t.start);
      if (s && s.date >= end) later++;
    }
    for (const d of days) d.items.sort((a, b) => (parseWhen(a.t.start)?.date.getHours() ?? -1) * 60 + (parseWhen(a.t.start)?.date.getMinutes() ?? 0) - ((parseWhen(b.t.start)?.date.getHours() ?? -1) * 60 + (parseWhen(b.t.start)?.date.getMinutes() ?? 0)));
    return { overdue, days, later };
  });

  function label(d: Date) {
    const l = dayLabel(d, app.now);
    return l === "Today" || l === "Tomorrow" ? `${l} · ${shortDay(d)}` : shortDay(d);
  }
  /** Row detail for a repeat on a later day. */
  function repeatWhen(t: Task) {
    const s = parseWhen(t.start);
    return s && !s.allDay ? `${hm(s.date)} · repeats` : "All day · repeats";
  }
</script>

<div class="flex flex-col gap-6">
  {#if data.overdue.length}
    <section aria-label="Overdue">
      <h2 class="m-0 px-4 pb-1.5 text-[13px] font-normal tracking-wide text-danger uppercase">Overdue</h2>
      <div class="overflow-hidden rounded-xl bg-card">
        {#each data.overdue as t, i (t.id)}<TaskRow task={t} last={i === data.overdue.length - 1} {desktop} {onopen} />{/each}
      </div>
    </section>
  {/if}
  {#each data.days as d, di (d.day.getTime())}
    {#if d.items.length || di < 2}
      <section aria-label={label(d.day)}>
        <h2 class="m-0 px-4 pb-1.5 text-[13px] font-normal tracking-wide text-muted uppercase">{label(d.day)}</h2>
        {#if d.items.length}
          <div class="overflow-hidden rounded-xl bg-card">
            {#each d.items as it, i (it.t.id + (it.first ? "" : d.day.getTime()))}
              <TaskRow task={it.t} last={i === d.items.length - 1} {desktop} {onopen} when={it.first ? undefined : repeatWhen(it.t)} />
            {/each}
          </div>
        {:else}
          <p class="m-0 rounded-xl bg-card px-4 py-3 text-[15px] text-muted">Nothing planned</p>
        {/if}
      </section>
    {/if}
  {/each}
  {#if data.later}<p class="m-0 px-4 text-[15px] text-muted">{data.later} more after {shortDay(addDays(startOfDay(app.now), DAYS - 1))}</p>{/if}
</div>
