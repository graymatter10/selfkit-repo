<script lang="ts">
  // Month at a glance: one square per day, shaded by the chosen measure
  // (four steps of the accent). Tap a day to read it.
  import type { Review } from "../lib/api";
  let { r, metric, fmt, unit }: { r: Review; metric: (number | null)[]; fmt: (v: number) => string; unit: string } = $props();
  const vals = $derived(metric.filter((v): v is number => v != null).sort((a, b) => a - b));
  const q = (f: number) => vals[Math.min(vals.length - 1, Math.floor(vals.length * f))] ?? 0;
  const lvl = (v: number | null) => (v == null || !vals.length ? 0 : v <= q(0.25) ? 1 : v <= q(0.5) ? 2 : v <= q(0.75) ? 3 : 4);
  const SH = ["", "9%", "20%", "36%", "62%"];
  let sel = $state<number | null>(null);
  const label = (i: number) => new Date(r.dates[i] + "T12:00").toLocaleDateString("en-GB", { weekday: "short", day: "numeric", month: "short" });
</script>

<div class="mt-3 grid grid-cols-7 gap-[5px]">
  {#each ["M", "T", "W", "T", "F", "S", "S"] as d}<div class="pb-0.5 text-center text-[12px] font-[550] text-muted">{d}</div>{/each}
  {#each Array(r.lead) as _}<div></div>{/each}
  {#each metric as v, i}
    <button type="button" aria-label="{label(i)}: {v == null ? 'no data' : fmt(v)}" onclick={() => (sel = sel === i ? null : i)}
      class="num aspect-square rounded-[8px] pt-[5px] pl-1.5 text-left align-top text-[12px] font-[550] text-ink2 {sel === i ? 'shadow-[inset_0_0_0_2px_var(--ink)]' : ''}"
      style="background:{lvl(v) ? `color-mix(in srgb, var(--accent) ${SH[lvl(v)]}, transparent)` : 'var(--fill2)'};display:flex;align-items:flex-start">{i + 1}</button>
  {/each}
</div>
<div class="mt-3 flex min-h-11 items-center border-t border-sep text-[13.5px]">
  {#if sel != null}<span class="text-ink2">{label(sel)}</span><span class="num ml-auto font-[650]">{metric[sel] == null ? "No data" : fmt(metric[sel]!)}</span>{#if metric[sel] != null}<span class="ml-1.5 text-muted">{unit}</span>{/if}
  {:else}<span class="text-muted">Tap a day to see it.</span>{/if}
</div>
