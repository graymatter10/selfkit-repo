<script lang="ts">
  // Single-series bar chart (accent colour, thin rounded bars, recessive axis).
  // Hover or tap a bar to read its value in the caption line (no legend: the
  // section title names the series).
  let { values, labels, tips, unit = "" }: { values: number[]; labels: string[]; tips: string[]; unit?: string } = $props();
  let sel = $state<number | null>(null);
  const max = $derived(Math.max(1, ...values));
  const W = 320, H = 120, PAD = 18;
  const bw = $derived((W - (values.length - 1) * 2) / values.length);
  const total = $derived(values.reduce((a, b) => a + b, 0));
</script>

<div class="flex flex-col gap-1">
  <p class="m-0 h-5 text-[13px] text-muted tabular-nums">{sel != null ? tips[sel] : `Total ${total}${unit}`}</p>
  <svg viewBox="0 0 {W} {H + PAD}" class="w-full" role="img" aria-label={tips.join("; ")} onpointerleave={() => (sel = null)}>
    <line x1="0" x2={W} y1={H + 0.5} y2={H + 0.5} stroke="var(--sep)" />
    {#each values as v, i}
      {@const h = v ? Math.max(3, (v / max) * (H - 6)) : 0}
      {@const x = i * (bw + 2)}
      <g role="presentation" onpointerenter={() => (sel = i)} onpointerdown={() => (sel = i)}>
        <rect x={x} y="0" width={bw} height={H} fill="transparent" />
        {#if h}<path d="M{x},{H} v{-(h - 4)} q0,-4 4,-4 h{bw - 8} q4,0 4,4 v{h - 4} z" fill="var(--accent)" opacity={sel == null || sel === i ? 1 : 0.45} />{/if}
      </g>
      {#if labels[i]}<text x={x + bw / 2} y={H + 13} text-anchor="middle" font-size="10" fill="var(--muted)">{labels[i]}</text>{/if}
    {/each}
  </svg>
</div>
