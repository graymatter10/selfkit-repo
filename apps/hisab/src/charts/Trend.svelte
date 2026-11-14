<script lang="ts">
  // Simple area line (net worth over time).
  import { short } from "../lib/fmt";
  let { values, labels = [], h = 100 }: { values: number[]; labels?: string[]; h?: number } = $props();
  const W = 340;
  const min = $derived(Math.min(0, ...values));
  const max = $derived(Math.max(1, ...values));
  const x = (i: number) => (values.length < 2 ? W / 2 : (i / (values.length - 1)) * W);
  const y = (v: number) => h - ((v - min) / (max - min || 1)) * h;
  const d = $derived(values.map((v, i) => `${i ? "L" : "M"}${x(i).toFixed(1)},${y(v).toFixed(1)}`).join(""));
</script>

{#if values.length}
  <svg width="100%" viewBox="0 -10 {W} {h + 26}" style="max-height:{h * 1.6 + 40}px" role="img" aria-label="Net worth trend">
    {#if min < 0}<line x1="0" x2={W} y1={y(0)} y2={y(0)} stroke="var(--faint)" stroke-dasharray="3 4" />{/if}
    <path d="{d}L{x(values.length - 1)},{h}L{x(0)},{h}Z" fill="var(--accent-soft)" />
    <path {d} stroke="var(--accent)" stroke-width="2.2" fill="none" stroke-linejoin="round" />
    <circle cx={x(values.length - 1)} cy={y(values[values.length - 1])} r="3.5" fill="var(--accent)" />
    <text x={W} y={y(max) - 2} font-size="9.5" text-anchor="end" fill="var(--muted)">{short(max)}</text>
    {#if labels.length > 1}<text x="0" y={h + 15} font-size="10" fill="var(--muted)">{labels[0]}</text><text x={W} y={h + 15} font-size="10" text-anchor="end" fill="var(--muted)">{labels[labels.length - 1]}</text>{/if}
  </svg>
{/if}
