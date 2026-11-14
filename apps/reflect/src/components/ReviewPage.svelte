<script lang="ts">
  import type { AppKey, Sect } from "../lib/api";
  import { dur, money } from "../lib/fmt";
  import { app, openTab, stepReview } from "../lib/state.svelte";
  import AppList from "./AppList.svelte";
  import Bars from "./Bars.svelte";
  import Calendar from "./Calendar.svelte";
  import Icon from "./Icon.svelte";
  import Seg from "./Seg.svelte";
  import Src from "./Src.svelte";
  let { desktop, kind }: { desktop: boolean; kind: "week" | "month" } = $props();
  const r = $derived(app.review[kind]);
  const sym = $derived(app.overview?.sym ?? "$");
  type Card = { key: string; title: string; app: AppKey; s: Sect; fmt: (v: number) => string; unit: string; calFmt: (v: number) => string };
  const cards = $derived(
    r
      ? ([
          r.screen && { key: "screen", title: "Screen time", app: "chrono", s: r.screen, fmt: (v: number) => dur(v * 60), unit: "on screens", calFmt: (v: number) => dur(v * 60) },
          r.spent && { key: "spent", title: "Spent", app: "hisab", s: r.spent, fmt: (v: number) => money(sym, v), unit: "spent", calFmt: (v: number) => money(sym, v) },
          r.tasks && { key: "tasks", title: "Tasks done", app: "agenda", s: r.tasks, fmt: (v: number) => String(v), unit: "tasks done", calFmt: (v: number) => String(v) },
          r.sleep && { key: "sleep", title: "Sleep", app: "dusk", s: r.sleep, fmt: (v: number) => dur(v * 60), unit: "asleep", calFmt: (v: number) => dur(v * 60) },
        ].filter(Boolean) as Card[])
      : [],
  );
  let metric = $state("screen");
  const cal = $derived(cards.find((c) => c.key === metric) ?? cards[0]);
  const ICON: Record<string, string> = { screen: "phone", spent: "coin", tasks: "check", sleep: "moon" };
</script>

{#snippet nav()}
  <div class="flex items-center gap-0.5">
    <button type="button" class="iconbtn press" aria-label="Earlier" disabled={!r?.can_prev} onclick={() => stepReview(kind, -1)}><Icon name="left" stroke={2.1} /></button>
    <span class="num min-w-[110px] px-1 text-center text-[15px] font-semibold">{r ? (desktop ? r.label : r.short_label) : ""}</span>
    <button type="button" class="iconbtn press" aria-label="Later" disabled={!r?.can_next} onclick={() => stepReview(kind, 1)}><Icon name="right" stroke={2.1} /></button>
  </div>
{/snippet}
{#snippet head()}
  {#if r}<div class="date">{r.range_label}</div><h1 class="headline {desktop ? '!text-[24px]' : ''}">{r.headline}</h1>{#if r.sub}<p class="subline">{r.sub}</p>{/if}{/if}
{/snippet}
{#snippet card(c: Card)}
  <div class="card p-4">
    <div class="ctitle">{c.title}<span class="ml-auto"><Src apps={[c.app]} /></span></div>
    <div class="num mt-2.5 text-[26px] leading-[1.1] font-[680] tracking-[-0.03em]">{c.s.value}{#if c.s.unit}<small class="ml-[3px] text-[14.5px] font-medium tracking-[-0.01em] text-muted">{c.s.unit}</small>{/if}</div>
    {#if c.s.sub}<div class="mt-[5px] text-[13px] text-muted">{c.s.sub}</div>{/if}
    <Bars series={c.s.series} labels={r!.day_labels} goal={c.s.goal} goalLabel={c.s.goal_label} fmt={c.fmt} />
  </div>
{/snippet}
{#snippet worth()}
  {#if r?.worth.length}
    <div class="card p-4"><div class="ctitle">Worth knowing</div>
      <div class="mt-2 flex flex-col">
        {#each r.worth as w, i}<div class="flex gap-3 py-3 text-[14px] leading-[1.45] text-ink2 {i ? 'border-t border-sep' : 'pt-1'}"><span class="mt-px shrink-0 text-muted"><Icon name={w.icon} size={18} /></span><span>{w.text}</span></div>{/each}
      </div></div>
  {/if}
{/snippet}
{#snippet kept()}
  {#if r?.kept.length}
    <div class="card p-4"><div class="ctitle">Streaks this {kind}</div>
      <div class="mt-1">
        {#each r.kept as k}
          <div class="flex min-h-11 items-center gap-3 text-[14px]"><span class="min-w-0 flex-1 truncate">{k.name}</span>
            {#if kind === "week"}<span class="flex gap-1">{#each k.pattern as p}<i class="h-3 w-3 rounded-[3px] {p === 'k' ? 'bg-accent' : p === 't' ? 'shadow-[inset_0_0_0_1.5px_var(--accent)]' : 'bg-fill'}"></i>{/each}</span>
            {:else}<span class="h-2 w-[30%] overflow-hidden rounded-[4px] bg-fill"><i class="block h-full rounded-[4px] bg-accent" style="width:{k.of ? (k.kept / k.of) * 100 : 0}%"></i></span>{/if}
            <span class="num w-16 text-right text-[13px] text-muted">{k.kept} of {k.of}</span></div>
        {/each}
      </div></div>
  {/if}
{/snippet}
{#snippet monthRows()}
  <div class="card">
    {#each cards as c}
      <div class="lrow"><span class="ic"><Icon name={ICON[c.key]} size={18} /></span><div class="lbl"><b>{c.title}</b><small>{c.s.sub}</small></div>
        <div class="flex flex-col items-end gap-1"><span class="num text-[15.5px] font-[650] whitespace-nowrap">{c.s.value}{c.s.unit ? ` ${c.s.unit}` : ""}</span><Src apps={[c.app]} /></div></div>
    {/each}
  </div>
{/snippet}
{#snippet calCard()}
  {#if cal}
    <div class="card p-4">
      <div class="ctitle">Each day</div>
      <div class="mt-3 flex flex-wrap gap-1.5" role="radiogroup" aria-label="Measure">
        {#each cards as c}<button type="button" role="radio" aria-checked={cal.key === c.key} class="press h-9 rounded-full px-[13px] text-[13.5px] font-[550] {cal.key === c.key ? 'bg-ink text-bg' : 'bg-fill text-ink2'}" onclick={() => (metric = c.key)}>{c.title === "Tasks done" ? "Tasks" : c.title}</button>{/each}
      </div>
      {#key cal.key + r!.label}<Calendar r={r!} metric={cal.s.series} fmt={cal.calFmt} unit={cal.unit} />{/key}
    </div>
  {/if}
{/snippet}

{#if !r}<p class="pt-16 text-center text-muted">Loading…</p>
{:else if desktop}
  <div class="max-w-[1020px]">
    <div class="mb-5 flex items-end gap-4"><h1 class="dtitle flex-1">{kind === "week" ? "Weekly review" : "Monthly review"}</h1>{@render nav()}</div>
    <div class="-mt-1 mb-[18px]">{@render head()}</div>
    {#if r.empty}<p class="small">Nothing recorded in this {kind}.</p>
    {:else if kind === "week"}
      <div class="grid gap-3.5" style="grid-template-columns:repeat({Math.max(2, cards.length)},1fr)">{#each cards as c}{@render card(c)}{/each}</div>
      <div class="mt-3.5 grid grid-cols-[1.2fr_1fr] items-start gap-3.5">{@render worth()}{@render kept()}</div>
    {:else}
      <div class="grid grid-cols-[1fr_1.1fr] items-start gap-3.5">
        <div class="flex max-w-[470px] flex-col gap-3">{@render calCard()}{@render kept()}</div>
        <div class="flex flex-col gap-3">{@render monthRows()}{@render worth()}
          {#if r.top_apps.length}<div class="card p-4"><div class="ctitle">Most used, a day on average<span class="ml-auto"><Src apps={["chrono"]} /></span></div><AppList rows={r.top_apps} /></div>{/if}</div>
      </div>
    {/if}
  </div>
{:else}
  <div class="flex items-center gap-2">
    <Seg label="Period" items={[{ id: "week", name: "Week" }, { id: "month", name: "Month" }]} value={kind} onchange={(v) => openTab(v as "week" | "month")} />
    <span class="ml-auto">{@render nav()}</span>
  </div>
  <div class="px-1 pt-4 pb-4">{@render head()}</div>
  {#if r.empty}<p class="small px-1">Nothing recorded in this {kind}.</p>
  {:else if kind === "week"}
    <div class="flex flex-col gap-3">{#each cards as c}{@render card(c)}{/each}{@render worth()}{@render kept()}</div>
  {:else}
    <div class="flex flex-col gap-3">{@render monthRows()}{@render calCard()}{@render worth()}
      {#if r.top_apps.length}<div class="card p-4"><div class="ctitle">Most used, a day on average<span class="ml-auto"><Src apps={["chrono"]} /></span></div><AppList rows={r.top_apps} /></div>{/if}
      {@render kept()}</div>
  {/if}
{/if}
