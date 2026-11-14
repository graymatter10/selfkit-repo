<script lang="ts">
  // Calendar: Monday-first month grid (tasks by start day; repeats included,
  // done tasks greyed), and the selected day's tasks below with "Add on this day".
  import { Icon } from "@selfkit/ui";
  import type { Task } from "../lib/api";
  import { app, tagged } from "../lib/state.svelte";
  import { MON_LONG, addDays, dayLabel, hm, isoDate, occurrences, parseWhen, sameDay, shortDay, startOfDay, sevMark } from "../lib/time";
  import TaskRow from "./TaskRow.svelte";

  let { desktop = false, onopen }: { desktop?: boolean; onopen: (t: Task) => void } = $props();
  let month = $state(new Date(app.now.getFullYear(), app.now.getMonth(), 1));
  let sel = $state(startOfDay(app.now));

  const grid = $derived.by(() => {
    const lead = (month.getDay() + 6) % 7;
    const first = addDays(month, -lead);
    const n = new Date(month.getFullYear(), month.getMonth() + 1, 0).getDate();
    const weeks = Math.ceil((lead + n) / 7);
    const cells = Array.from({ length: weeks * 7 }, (_, i) => ({ day: addDays(first, i), items: [] as { t: Task; first: boolean }[] }));
    const end = addDays(first, weeks * 7);
    for (const t of tagged()) {
      for (const o of occurrences(t, first, end)) {
        const i = Math.round((o.day.getTime() - first.getTime()) / 86_400_000);
        if (i >= 0 && i < cells.length) cells[i].items.push({ t, first: o.first });
      }
    }
    const mins = (t: Task) => { const s = parseWhen(t.start); return s && !s.allDay ? s.date.getHours() * 60 + s.date.getMinutes() : -1; };
    for (const c of cells) c.items.sort((a, b) => Number(!!a.t.done_at) - Number(!!b.t.done_at) || mins(a.t) - mins(b.t));
    return cells;
  });
  const selItems = $derived(grid.find((c) => sameDay(c.day, sel))?.items ?? itemsFor(sel));
  function itemsFor(d: Date) {
    return tagged().flatMap((t) => occurrences(t, d, addDays(d, 1)).map((o) => ({ t, first: o.first })));
  }
  function go(k: number) {
    month = new Date(month.getFullYear(), month.getMonth() + k, 1);
  }
  function today() {
    month = new Date(app.now.getFullYear(), app.now.getMonth(), 1);
    sel = startOfDay(app.now);
  }
  function pick(d: Date) {
    sel = d;
    if (d.getMonth() !== month.getMonth()) month = new Date(d.getFullYear(), d.getMonth(), 1);
  }
  function rowWhen(t: Task) {
    const s = parseWhen(t.start);
    return s && !s.allDay ? `${hm(s.date)} · repeats` : "All day · repeats";
  }
  // Swipe left/right on the grid changes month (phone).
  let x0 = 0;
</script>

<div class="flex flex-col gap-4">
  <div class="overflow-hidden rounded-xl bg-card pb-2">
    <div class="flex items-center justify-between py-1 pr-1 pl-4">
      <span class="text-[17px] font-semibold">{MON_LONG[month.getMonth()]} {month.getFullYear()}</span>
      <div class="flex items-center text-accent">
        <button type="button" class="press px-2 text-[15px]" onclick={today}>Today</button>
        <button type="button" aria-label="Previous month" class="press flex h-11 w-11 items-center justify-center" onclick={() => go(-1)}><Icon name="back" size={22} stroke={2.2} /></button>
        <button type="button" aria-label="Next month" class="press flex h-11 w-11 items-center justify-center" onclick={() => go(1)}><Icon name="chevron" size={22} stroke={2.2} /></button>
      </div>
    </div>
    <div role="group" aria-label="{MON_LONG[month.getMonth()]} {month.getFullYear()}" class="grid grid-cols-7 px-1.5"
      ontouchstart={(e) => (x0 = e.touches[0].clientX)} ontouchend={(e) => { const dx = e.changedTouches[0].clientX - x0; if (Math.abs(dx) > 60) go(dx < 0 ? 1 : -1); }}>
      {#each ["MON", "TUE", "WED", "THU", "FRI", "SAT", "SUN"] as w}<span class="pb-1 text-center text-[11px] font-semibold text-muted">{w}</span>{/each}
      {#each grid as c (c.day.getTime())}
        {@const out = c.day.getMonth() !== month.getMonth()}
        {@const isSel = sameDay(c.day, sel)}
        {@const isToday = sameDay(c.day, app.now)}
        {@const open = c.items.filter((x) => !x.t.done_at).length}
        <button type="button" aria-pressed={isSel} aria-label="{shortDay(c.day)}, {c.items.length} tasks" onclick={() => pick(c.day)}
          class="flex flex-col items-center gap-0.5 rounded-lg py-1 {desktop ? 'min-h-[84px] items-stretch px-1' : 'min-h-[52px]'} {out ? 'opacity-40' : ''} {desktop && isSel ? 'bg-fill2' : ''}">
          <span class="flex h-8 w-8 shrink-0 items-center justify-center self-center rounded-full text-[17px] tabular-nums {isSel ? (isToday ? 'bg-accent font-semibold text-on-accent' : 'bg-ink font-semibold text-bg') : isToday ? 'font-semibold text-accent' : ''}">{c.day.getDate()}</span>
          {#if desktop}
            {@const openItems = c.items.filter((x) => !x.t.done_at)}
            {#each openItems.slice(0, 2) as it}
              <span class="truncate rounded bg-accent-soft px-1 text-left text-[12px] leading-[18px] text-ink2">{sevMark(it.t.severity)}{sevMark(it.t.severity) ? " " : ""}{it.t.title}</span>
            {/each}
            {#if openItems.length > 2}<span class="px-1 text-left text-[11px] text-muted">+{openItems.length - 2} more</span>{/if}
            {#if c.items.length > openItems.length}<span class="px-1 text-left text-[11px] text-muted">{c.items.length - openItems.length} done</span>{/if}
          {:else if c.items.length}
            <span class="flex h-1.5 gap-[3px]" aria-hidden="true">
              {#each Array(Math.min(open, 3)) as _}<span class="h-1.5 w-1.5 rounded-full bg-accent"></span>{/each}
              {#if !open}<span class="h-1.5 w-1.5 rounded-full bg-faint"></span>{/if}
            </span>
          {/if}
        </button>
      {/each}
    </div>
  </div>

  <section aria-label="Tasks on {shortDay(sel)}">
    <div class="flex items-center justify-between px-4 pb-1.5">
      <h2 class="m-0 text-[13px] font-normal tracking-wide text-muted uppercase">{dayLabel(sel, app.now) === shortDay(sel) ? shortDay(sel) : `${dayLabel(sel, app.now)} · ${shortDay(sel)}`}</h2>
      <button type="button" class="press flex items-center gap-1 text-[15px] text-accent" onclick={() => (app.panel = { kind: "new", start: isoDate(sel) })}><Icon name="plus" size={16} stroke={2.4} />Add</button>
    </div>
    {#if selItems.length}
      <div class="overflow-hidden rounded-xl bg-card">
        {#each selItems as it, i (it.t.id)}<TaskRow task={it.t} last={i === selItems.length - 1} {desktop} {onopen} when={it.first ? undefined : rowWhen(it.t)} />{/each}
      </div>
    {:else}
      <p class="m-0 rounded-xl bg-card px-4 py-3 text-[15px] text-muted">Nothing on this day</p>
    {/if}
  </section>
</div>
