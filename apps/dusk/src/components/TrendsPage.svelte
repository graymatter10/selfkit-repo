<script lang="ts">
  // Trends: 2 weeks / Month / 3 months of nights, the four stats, and bed/wake times.
  import { dur, hmParts } from "../lib/fmt";
  import { app, openTab, show } from "../lib/state.svelte";
  import NightBars from "./NightBars.svelte";
  import RangeChart from "./RangeChart.svelte";
  import Seg from "./Seg.svelte";
  import StatsGrid from "./StatsGrid.svelte";

  let { desktop }: { desktop: boolean } = $props();
  let span = $state<14 | 30 | 90>(30);
  const dash = $derived(app.dash!);
  const ns = $derived(dash.nights.slice(-span));
  const st = $derived(span === 14 ? dash.s14 : span === 30 ? dash.s30 : dash.s90);
  const parts = $derived(st.avg_min != null ? hmParts(st.avg_min) : null);
  const pick = (n: string) => (show(n), openTab("night"));
</script>

<div class={desktop ? "max-w-[1000px]" : ""}>
  {#if desktop}<h1 class="m-0 mb-3.5 text-[26px] font-bold tracking-[-0.03em]">Trends</h1>{/if}
  <div class="mt-1 mb-4 {desktop ? 'w-[360px]' : ''}">
    <Seg label="Period" items={[{ id: 14, name: "2 weeks" }, { id: 30, name: "Month" }, { id: 90, name: "3 months" }]} value={span} onchange={(v) => (span = v)} />
  </div>
  <div class="card p-4">
    <div class="mb-3.5 flex items-start">
      <div class="flex flex-1 flex-col gap-1">
        <span class="text-[13px] text-muted">Average asleep</span>
        {#if parts}<span class="num text-[28px] leading-none font-[650] tracking-[-0.045em]">{parts[0]}<span class="mr-[0.26em] ml-[0.02em] text-[0.5em] font-semibold tracking-normal text-ink2">h</span>{parts[1]}<span class="ml-[0.02em] text-[0.5em] font-semibold tracking-normal text-ink2">m</span></span>
        {:else}<span class="text-[22px] font-[650]">No data yet</span>{/if}
      </div>
      <div class="legend mt-0.5"><span><i style="background:var(--accent2)"></i>Night</span><span><i style="background:var(--faint)"></i>Nap</span></div>
    </div>
    <NightBars nights={ns} goal={dash.goals.goal_min} height={desktop ? 170 : 130} labels={span === 14 ? "day" : "sparse"} onpick={pick} />
  </div>
  <div class="mt-3"><StatsGrid {dash} cols={desktop ? 4 : 2} /></div>
  <div class="card mt-3 p-4">
    <div class="mb-3.5 flex items-center"><h2 class="m-0 flex-1 text-[15px] font-[650]">Bed and wake times</h2><div class="legend"><span><i class="dash"></i>Bedtime goal {dash.goals.bedtime}</span></div></div>
    <RangeChart nights={dash.nights.slice(-(desktop ? Math.min(span, 30) : Math.min(span, 14)))} bedtimeMin={Number(dash.goals.bedtime.slice(0, 2)) * 60 + Number(dash.goals.bedtime.slice(3))} height={desktop ? 190 : 180} />
  </div>
  {#if span === 90 && dash.weekly.length}
    <div class="card mt-3 p-4">
      <h2 class="m-0 mb-2 text-[15px] font-[650]">Weekly averages</h2>
      {#each dash.weekly.slice(-13).reverse() as w}
        <div class="flex h-9 items-center gap-3 text-[14px]"><span class="w-28 text-muted">Week of {w.week.slice(5).replace("-", "/")}</span>
          <div class="h-2 flex-1 overflow-hidden rounded-full bg-fill2"><i class="block h-full rounded-full bg-accent2" style="width:{Math.min(100, (w.avg_min / 600) * 100)}%"></i></div>
          <span class="num w-16 text-right font-semibold">{dur(w.avg_min)}</span></div>
      {/each}
    </div>
  {/if}
</div>
