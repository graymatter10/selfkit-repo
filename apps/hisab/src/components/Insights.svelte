<script lang="ts">
  // Overview + trends, only on request: Month (daily line, categories,
  // budgets), Year (bars, categories, heatmap), Lifetime (years, net worth).
  import Bars from "../charts/Bars.svelte";
  import Cumulative from "../charts/Cumulative.svelte";
  import Donut from "../charts/Donut.svelte";
  import Heatmap from "../charts/Heatmap.svelte";
  import Trend from "../charts/Trend.svelte";
  import { api, type Overview, type Trends } from "../lib/api";
  import { addMonths, money, monthIso, monthName, shortDate } from "../lib/fmt";
  import { app, main, openHistory } from "../lib/state.svelte";
  import CatBadge from "./CatBadge.svelte";
  import Ic from "./Ic.svelte";
  import Seg from "./Seg.svelte";

  let { desktop = false }: { desktop?: boolean } = $props();
  let view = $state<"month" | "year" | "lifetime">("month");
  let month = $state(monthIso());
  let tr = $state<Trends | null>(null);
  let ov = $state<Overview | null>(null);
  const year = $derived(Number(month.slice(0, 4)));

  // Replies can arrive out of order (async commands): keep only the newest (C7).
  let seq = 0;
  $effect(() => {
    void app.rev;
    const m = month;
    const n = ++seq;
    api.trends(m).then((t) => n === seq && (tr = t));
    api.snapshot(m).then((s) => n === seq && (ov = s.overview));
  });

  function step(n: number) {
    month = view === "year" ? addMonths(month, 12 * n) : addMonths(month, n);
    if (month > monthIso()) month = monthIso();
  }
  const isNow = $derived(view === "year" ? year >= new Date().getFullYear() : month >= monthIso());
  const MS = ["J", "F", "M", "A", "M", "J", "J", "A", "S", "O", "N", "D"];
  const yearSums = $derived(tr ? tr.months.reduce((a, m) => ({ spent: a.spent + m.spent, income: a.income + m.income }), { spent: 0, income: 0 }) : { spent: 0, income: 0 });
  const vsPrev = $derived(ov && ov.prev_spent > 0 ? Math.round(((ov.spent - ov.prev_spent) / ov.prev_spent) * 100) : null);
  const budgets = $derived((app.snap?.categories ?? []).filter((c) => c.kind === "expense" && !c.archived && c.budget > 0));
  const cur = $derived(main());
  const card = "card px-4 pt-3.5 pb-4";
  const h2 = "m-0 mb-2 flex items-baseline justify-between gap-2 text-[14px] font-semibold";
</script>

{#snippet stat(label: string, v: string, cls = "")}
  <div class="min-w-0 flex-1 {desktop ? 'px-4' : 'px-3'} py-3.5"><div class="text-[12.5px] font-medium text-muted">{label}</div><div class="num mt-0.5 overflow-hidden leading-tight text-ellipsis whitespace-nowrap {desktop ? (v.length > 18 ? "text-[14px]" : v.length > 14 ? "text-[17px]" : "text-[21px]") : v.length > 17 ? "text-[9.5px]" : v.length > 15 ? "text-[10.5px]" : v.length > 13 ? "text-[11.5px]" : v.length > 10 ? "text-[13.5px]" : v.length > 7 ? "text-[15px]" : "text-[18px]"} font-semibold tracking-[-0.4px] {cls}">{v}</div></div>
{/snippet}

<div class="flex flex-col gap-3">
  <div class="flex flex-col gap-3 {desktop ? 'sm:flex-row sm:items-center' : ''}">
    <div class={desktop ? "w-[300px]" : ""}><Seg label="Range" items={[{ id: "month", name: "Month" }, { id: "year", name: "Year" }, { id: "lifetime", name: "Lifetime" }]} value={view} onchange={(v) => (view = v)} /></div>
    {#if view !== "lifetime"}
      <div class="flex items-center justify-between {desktop ? 'ml-auto gap-2' : 'px-1'}">
        <button type="button" aria-label="Previous" class="press flex h-9 w-9 items-center justify-center rounded-full text-ink2" onclick={() => step(-1)}><Ic name="back" size={20} /></button>
        <b class="text-[17px] {desktop ? 'w-44 text-center' : ''}">{view === "year" ? year : monthName(month, true)}</b>
        <button type="button" aria-label="Next" disabled={isNow} class="press flex h-9 w-9 items-center justify-center rounded-full text-ink2 disabled:opacity-25" onclick={() => step(1)}><Ic name="chevron" size={20} /></button>
      </div>
    {/if}
  </div>

  {#if tr && ov}
    {#if tr.missing_rates.length}<p class="m-0 rounded-[12px] bg-danger-soft px-3 py-2 text-[13px] text-ink2">No exchange rate for {tr.missing_rates.join(", ")}; counted 1:1. Add it in Settings → Currencies.</p>{/if}
    {#if view === "month"}
      <div class="card flex">
        {@render stat("Spent", money(ov.spent, cur))}
        {@render stat(ov.remaining !== null ? (ov.remaining < 0 ? "Over budget" : "Remaining") : "Income", money(ov.remaining !== null ? Math.abs(ov.remaining) : ov.income, cur), ov.remaining !== null && ov.remaining < 0 ? "text-danger" : "")}
        {@render stat("Balance", (ov.balance > 0 ? "+" : "") + money(ov.balance, cur), ov.balance > 0 ? "text-accent-ink" : ov.balance < 0 ? "text-danger" : "")}
        {#if desktop}{@render stat("vs last month", vsPrev === null ? "—" : `${vsPrev > 0 ? "+" : ""}${vsPrev}%`)}{/if}
      </div>
      <div class="grid gap-3 {desktop ? 'grid-cols-[minmax(0,1.4fr)_minmax(0,1fr)]' : 'grid-cols-[minmax(0,1fr)]'}">
        <div class={card}>
          <h3 class={h2}>Daily spending<span class="text-[12px] font-normal text-muted">vs {monthName(addMonths(month, -1))} ┄</span></h3>
          <Cumulative daily={tr.daily} prev={tr.daily_prev} budget={ov.budget} today={ov.days_elapsed} {cur} />
          <p class="m-0 text-[12.5px] text-muted">{ov.days_elapsed ? `${money(ov.spent / Math.max(1, ov.days_elapsed), cur)} a day on average` : ""}{vsPrev !== null && !desktop ? ` · ${vsPrev > 0 ? "+" : ""}${vsPrev}% vs last month` : ""}</p>
        </div>
        <div class={card}><h3 class={h2}>By category</h3><Donut data={tr.by_category_month} {cur} onpick={(id) => openHistory({ category: id, period: month === monthIso() ? "month" : "all" })} /></div>
      </div>
      {#if budgets.length && month === monthIso()}
        <div class={card}>
          <h3 class={h2}>Budgets</h3>
          <div class="flex flex-col gap-3">
            {#each budgets as c (c.id)}
              {@const over = c.spent > c.budget}
              <div class="flex items-center gap-3">
                <CatBadge icon={c.icon} color={c.color} size={30} />
                <div class="min-w-0 flex-1">
                  <div class="flex justify-between text-[14px]"><span class="truncate">{c.name}</span><span class="num {over ? 'font-medium text-danger' : 'text-muted'}">{money(c.spent, cur)} / {money(c.budget, cur)}</span></div>
                  <div class="mt-1.5 h-[5px] overflow-hidden rounded-full bg-fill"><div class="h-full rounded-full {over ? 'bg-danger' : 'bg-accent'}" style="width:{Math.min(100, (c.spent / c.budget) * 100)}%"></div></div>
                </div>
              </div>
            {/each}
          </div>
        </div>
      {/if}
      {#if tr.income_by_category_month.length}<div class={card}><h3 class={h2}>Income</h3><Donut data={tr.income_by_category_month} {cur} label="Income" /></div>{/if}
    {:else if view === "year"}
      <div class="card flex">
        {@render stat("Spent", money(yearSums.spent, cur))}
        {@render stat("Income", money(yearSums.income, cur))}
        {@render stat("Saved", money(yearSums.income - yearSums.spent, cur), yearSums.income - yearSums.spent >= 0 ? "text-accent-ink" : "text-danger")}
      </div>
      <div class="grid gap-3 {desktop ? 'grid-cols-[minmax(0,1.4fr)_minmax(0,1fr)]' : 'grid-cols-[minmax(0,1fr)]'}">
        <div class={card}><h3 class={h2}>Months<span class="text-[12px] font-normal text-muted">spent · income (grey)</span></h3>
          <Bars items={tr.months.map((m, i) => ({ label: MS[i], key: m.month, spent: m.spent, income: m.income }))} active={month} /></div>
        <div class={card}><h3 class={h2}>By category · {year}</h3><Donut data={tr.by_category_year} {cur} /></div>
      </div>
      <div class={card}><h3 class={h2}>Spending calendar · {year}<span class="text-[12px] font-normal text-muted">darker = more</span></h3><Heatmap {year} data={tr.heatmap} {cur} /></div>
    {:else}
      <div class="card flex">
        {@render stat("Spent", money(tr.lifetime.spent, cur))}
        {@render stat("Income", money(tr.lifetime.income, cur))}
        {@render stat("Net worth", money(ov.net_worth, cur), ov.net_worth < 0 ? "text-danger" : "")}
      </div>
      <p class="m-0 px-1 text-[13px] text-muted">{tr.lifetime.count} {tr.lifetime.count === 1 ? "entry" : "entries"}{tr.lifetime.first ? ` since ${shortDate(tr.lifetime.first)}` : ""}</p>
      <div class="grid gap-3 {desktop ? 'grid-cols-[minmax(0,1fr)_minmax(0,1fr)]' : 'grid-cols-[minmax(0,1fr)]'}">
        <div class={card}><h3 class={h2}>Years</h3><Bars items={tr.years.map((y) => ({ label: y.year, key: y.year, spent: y.spent, income: y.income }))} /></div>
        <div class={card}><h3 class={h2}>Net worth over time</h3><Trend values={tr.networth.map((n) => n.value)} labels={tr.networth.map((n) => monthName(n.month, true))} /></div>
      </div>
    {/if}
  {/if}
</div>
