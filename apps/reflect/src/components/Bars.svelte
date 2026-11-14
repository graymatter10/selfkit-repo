<script lang="ts">
  // Small day bars for reviews: the highest day in the accent, an optional
  // dashed goal line. Tap a bar to read its value.
  let { series, labels, goal = null, goalLabel = "", fmt, max: fixedMax = 0 }: {
    series: (number | null)[]; labels: string[]; goal?: number | null; goalLabel?: string; fmt: (v: number) => string; max?: number;
  } = $props();
  const max = $derived(fixedMax || Math.max(goal ?? 0, ...series.map((v) => v ?? 0)) * 1.08 || 1);
  const hi = $derived(series.reduce((b, v, i) => ((v ?? -1) > (series[b] ?? -1) ? i : b), 0));
  let sel = $state<number | null>(null);
  const H = 66;
</script>

<div class="relative mt-3.5 flex h-[84px] items-end gap-2">
  {#each series as v, i}
    <button type="button" class="flex h-full flex-1 flex-col items-center justify-end gap-1.5" aria-label="{labels[i]}: {v == null ? 'no data' : fmt(v)}" onclick={() => (sel = sel === i ? null : i)}>
      <i class="block w-full max-w-[26px] rounded-[5px_5px_3px_3px] {i === hi && v ? 'bg-accent' : 'bg-bar2'} {sel === i ? 'outline-2 outline-offset-1 outline-ink2' : ''}" style="height:{v == null ? 0 : Math.max(3, (v / max) * H)}px"></i>
      <span class="text-[12px] leading-none text-muted">{labels[i]}</span>
    </button>
  {/each}
  {#if goal}
    <div class="pointer-events-none absolute inset-x-0 border-t-[1.5px] border-dashed border-muted opacity-55" style="bottom:{18 + (goal / max) * H}px"><em class="absolute -top-[17px] right-0 bg-card pl-1 text-[12px] text-muted not-italic">{goalLabel}</em></div>
  {/if}
</div>
{#if sel != null}<p class="num mt-2 text-[13px] text-ink2">{labels[sel]} · <b class="font-semibold">{series[sel] == null ? "no data" : fmt(series[sel]!)}</b></p>{/if}
