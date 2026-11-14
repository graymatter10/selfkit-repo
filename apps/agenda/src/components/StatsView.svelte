<script lang="ts">
  // Stats (own screen, never on the home screen): completed per day and per
  // week, on-time rate, focus minutes. Data: `completions` + `focus` collections.
  import { api, type Completion, type FocusLog } from "../lib/api";
  import { app } from "../lib/state.svelte";
  import { addDays, shortDay, startOfDay } from "../lib/time";
  import Bars from "./Bars.svelte";

  let hist = $state<Completion[]>([]);
  let focus = $state<FocusLog[]>([]);
  $effect(() => {
    void app.tasks; // reload when tasks change
    api.history().then((h) => (hist = h));
    api.focusSessions().then((f) => (focus = f));
  });

  const DOW1 = ["S", "M", "T", "W", "T", "F", "S"];
  const today = $derived(startOfDay(app.now));
  const dayIdx = (ms: number, n: number) => Math.floor((startOfDay(new Date(ms)).getTime() - addDays(today, -(n - 1)).getTime()) / 86_400_000 + 0.5);
  /** Monday of the week containing d. */
  const monday = (d: Date) => addDays(startOfDay(d), -((d.getDay() + 6) % 7));

  const perDay = $derived.by(() => {
    const v = Array(14).fill(0);
    for (const c of hist) { const i = dayIdx(c.done_at, 14); if (i >= 0 && i < 14) v[i]++; }
    return v;
  });
  const perWeek = $derived.by(() => {
    const v = Array(12).fill(0);
    const m0 = addDays(monday(app.now), -77);
    for (const c of hist) { const i = Math.floor((monday(new Date(c.done_at)).getTime() - m0.getTime()) / (7 * 86_400_000) + 0.5); if (i >= 0 && i < 12) v[i]++; }
    return v;
  });
  const focusPerDay = $derived.by(() => {
    const v = Array(14).fill(0);
    for (const f of focus) if (f.kind === "focus") { const i = dayIdx(f.start, 14); if (i >= 0 && i < 14) v[i] += f.minutes; }
    return v;
  });
  function rate(from: number) {
    const dated = hist.filter((c) => c.on_time != null && c.done_at >= from);
    return dated.length ? { pct: Math.round((dated.filter((c) => c.on_time).length / dated.length) * 100), n: dated.length } : null;
  }
  const rate30 = $derived(rate(addDays(today, -29).getTime()));
  const rateAll = $derived(rate(0));
  const weekStart = $derived(monday(app.now).getTime());
  const doneWeek = $derived(hist.filter((c) => c.done_at >= weekStart).length);
  const focusWeek = $derived(focus.filter((f) => f.kind === "focus" && f.start >= weekStart).reduce((a, f) => a + f.minutes, 0));
  const fmtH = (m: number) => (m >= 60 ? `${Math.floor(m / 60)} h ${m % 60 ? `${m % 60} min` : ""}`.trim() : `${m} min`);

  const dayLabels = $derived(perDay.map((_, i) => DOW1[addDays(today, i - 13).getDay()]));
  const dayTips = (unit: (n: number) => string, v: number[]) => v.map((n, i) => `${shortDay(addDays(today, i - 13))} · ${unit(n)}`);
  const weekLabels = $derived(perWeek.map((_, i) => (i % 3 === 2 || i === 11 ? `${addDays(monday(app.now), (i - 11) * 7).getDate()}/${addDays(monday(app.now), (i - 11) * 7).getMonth() + 1}` : "")));
  const weekTips = $derived(perWeek.map((n, i) => `Week of ${shortDay(addDays(monday(app.now), (i - 11) * 7))} · ${n} done`));
</script>

{#snippet tile(label: string, value: string, sub: string)}
  <div class="flex flex-col gap-0.5 rounded-xl bg-card px-4 py-3">
    <span class="text-[13px] text-muted">{label}</span>
    <span class="text-[26px] leading-tight font-semibold tracking-[-0.4px] tabular-nums">{value}</span>
    <span class="text-[13px] text-muted">{sub}</span>
  </div>
{/snippet}

<div class="flex flex-col gap-6">
  <div class="grid grid-cols-2 gap-3 md:grid-cols-4">
    {@render tile("Done this week", String(doneWeek), `${hist.length} all time`)}
    {@render tile("On time · 30 days", rate30 ? `${rate30.pct}%` : "—", rate30 ? `of ${rate30.n} dated tasks` : "No dated tasks yet")}
    {@render tile("Focus this week", fmtH(focusWeek), `${focus.filter((f) => f.kind === "focus" && f.completed).length} full sessions in all`)}
    {@render tile("On time · all", rateAll ? `${rateAll.pct}%` : "—", rateAll ? `of ${rateAll.n} dated tasks` : "")}
  </div>
  <div class="grid gap-6 md:grid-cols-2">
  <section aria-label="Completed per day" class="flex flex-col gap-1.5">
    <h2 class="m-0 px-4 text-[13px] font-normal tracking-wide text-muted uppercase">Completed per day · 14 days</h2>
    <div class="rounded-xl bg-card px-4 py-3"><Bars values={perDay} labels={dayLabels} tips={dayTips((n) => `${n} done`, perDay)} /></div>
  </section>
  <section aria-label="Completed per week" class="flex flex-col gap-1.5">
    <h2 class="m-0 px-4 text-[13px] font-normal tracking-wide text-muted uppercase">Completed per week · 12 weeks</h2>
    <div class="rounded-xl bg-card px-4 py-3"><Bars values={perWeek} labels={weekLabels} tips={weekTips} /></div>
  </section>
  <section aria-label="Focus minutes per day" class="flex flex-col gap-1.5">
    <h2 class="m-0 px-4 text-[13px] font-normal tracking-wide text-muted uppercase">Focus minutes per day · 14 days</h2>
    <div class="rounded-xl bg-card px-4 py-3"><Bars values={focusPerDay} labels={dayLabels} tips={dayTips((n) => `${n} min focus`, focusPerDay)} unit=" min" /></div>
  </section>
  </div>
</div>
