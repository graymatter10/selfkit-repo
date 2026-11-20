<script lang="ts">
  // Date + time picker sheet (iOS style): calendar, quick days, all-day switch,
  // hour/minute wheels. Cancel / Done in the header.
  import { Icon } from "@selfkit/ui";
  import Wheel from "./Wheel.svelte";
  import { MON_LONG, addDays, fmtWhen, parseWhen, shortDay, startOfDay } from "../lib/time";
  import { app } from "../lib/state.svelte";

  let { which, value, base, onset, onclose }: { which: "start" | "stop"; value?: string; base?: string; onset: (v: string) => void; onclose: () => void } = $props();

  const now = app.now;
  const cur = parseWhen(value);
  const b = parseWhen(base);
  const fallback = which === "stop" && b ? new Date(b.date.getTime() + 3_600_000) : new Date(now.getFullYear(), now.getMonth(), now.getDate(), now.getHours() + 1, 0);
  const t0 = cur && !cur.allDay ? cur.date : fallback;
  let day = $state(startOfDay(cur?.date ?? t0)); // t0 may roll past midnight (23:50 → tomorrow 00:00)
  let hour = $state(t0.getHours());
  let minute = $state(t0.getMinutes());
  let allDay = $state(!!cur?.allDay);
  let month = $state(new Date(day.getFullYear(), day.getMonth(), 1));

  const hours: [number, string][] = Array.from({ length: 24 }, (_, i) => [i, String(i).padStart(2, "0")]);
  const minutes = $derived.by(() => {
    const m: [number, string][] = Array.from({ length: 12 }, (_, i) => [i * 5, String(i * 5).padStart(2, "0")]);
    if (minute % 5) m.push([minute, String(minute).padStart(2, "0")]), m.sort((a, c) => a[0] - c[0]);
    return m;
  });
  const cells = $derived.by(() => {
    const lead = (month.getDay() + 6) % 7; // Monday first
    const n = new Date(month.getFullYear(), month.getMonth() + 1, 0).getDate();
    return [...Array(lead).fill(null), ...Array.from({ length: n }, (_, i) => new Date(month.getFullYear(), month.getMonth(), i + 1))];
  });
  const same = (a: Date, c: Date) => a.toDateString() === c.toDateString();
  const nextMon = addDays(startOfDay(now), (8 - now.getDay()) % 7 || 7);
  const quick = [
    { n: "Today", d: startOfDay(now) },
    { n: "Tomorrow", d: addDays(startOfDay(now), 1) },
    { n: "Next Mon", d: nextMon },
  ];
  function pick(d: Date) {
    day = d;
    month = new Date(d.getFullYear(), d.getMonth(), 1);
  }
  function done() {
    if (allDay && which === "start") return onset(fmtWhen(day, true));
    let d = new Date(day.getFullYear(), day.getMonth(), day.getDate(), hour, minute);
    if (which === "stop" && b && !b.allDay && d <= b.date && same(day, b.date)) d = addDays(d, 1);
    onset(fmtWhen(d, false));
  }
</script>

<div class="fixed inset-0 z-[60] flex items-end justify-center sm:items-center">
  <button type="button" aria-label="Cancel" class="fade absolute inset-0 bg-scrim" onclick={onclose}></button>
  <div role="dialog" aria-modal="true" aria-label={which === "start" ? "Starts" : "Ends"}
    class="rise relative flex max-h-[94dvh] w-full max-w-[420px] flex-col overflow-y-auto rounded-t-[14px] bg-card pb-[max(12px,env(safe-area-inset-bottom))] sm:rounded-[14px]">
    <div class="sticky top-0 z-10 grid grid-cols-[1fr_auto_1fr] items-center bg-card px-4 py-2.5">
      <button type="button" class="press justify-self-start py-2 text-[17px] text-accent" onclick={onclose}>Cancel</button>
      <span class="text-[17px] font-semibold">{which === "start" ? "Starts" : "Ends"}</span>
      <button type="button" class="press justify-self-end py-2 text-[17px] font-semibold text-accent" onclick={done}>Done</button>
    </div>
    <div class="flex gap-2 px-4 pb-1">
      {#each quick as q}
        <button type="button" aria-pressed={same(day, q.d)} onclick={() => pick(q.d)}
          class="press h-8 rounded-full px-3.5 text-[15px] {same(day, q.d) ? 'bg-accent-soft font-semibold text-accent' : 'bg-fill text-ink2'}">{q.n}</button>
      {/each}
    </div>
    <div class="flex items-center justify-between pt-3 pr-2 pl-5">
      <span class="text-[17px] font-semibold">{MON_LONG[month.getMonth()]} {month.getFullYear()}</span>
      <div class="flex text-accent">
        <button type="button" aria-label="Previous month" class="press flex h-11 w-11 items-center justify-center" onclick={() => (month = new Date(month.getFullYear(), month.getMonth() - 1, 1))}><Icon name="back" size={22} stroke={2.2} /></button>
        <button type="button" aria-label="Next month" class="press flex h-11 w-11 items-center justify-center" onclick={() => (month = new Date(month.getFullYear(), month.getMonth() + 1, 1))}><Icon name="chevron" size={22} stroke={2.2} /></button>
      </div>
    </div>
    <div class="grid grid-cols-7 px-3 text-center">
      {#each ["MON", "TUE", "WED", "THU", "FRI", "SAT", "SUN"] as w}<span class="pb-1 text-[11px] font-semibold text-muted">{w}</span>{/each}
      {#each cells as c}
        {#if c}
          <button type="button" aria-label={shortDay(c)} aria-pressed={same(c, day)} onclick={() => pick(c)} class="flex h-11 items-center justify-center">
            <span class="flex h-10 w-10 items-center justify-center rounded-full text-[19px] tabular-nums {same(c, day) ? 'bg-accent font-semibold text-on-accent' : same(c, now) ? 'font-semibold text-accent' : c < startOfDay(now) ? 'text-muted' : ''}">{c.getDate()}</span>
          </button>
        {:else}<span></span>{/if}
      {/each}
    </div>
    <div class="mx-4 mt-2 border-t border-sep"></div>
    {#if which === "start"}
      <label class="flex min-h-12 items-center justify-between px-5 text-[17px]">All day
        <input type="checkbox" bind:checked={allDay} class="peer sr" />
        <span aria-hidden="true" class="relative h-[31px] w-[51px] rounded-full transition-colors {allDay ? 'bg-accent' : 'bg-fill'}"><span class="absolute top-[2px] h-[27px] w-[27px] rounded-full bg-white shadow transition-transform {allDay ? 'translate-x-[22px]' : 'translate-x-[2px]'}"></span></span>
      </label>
    {/if}
    {#if !allDay || which === "stop"}
      <div class="flex items-center justify-center gap-1 py-2">
        <Wheel items={hours} value={hour} onchange={(v) => (hour = v)} label="Hour" />
        <span class="pb-1 text-[22px] font-semibold">:</span>
        <Wheel items={minutes} value={minute} onchange={(v) => (minute = v)} label="Minutes" />
      </div>
    {/if}
  </div>
</div>
