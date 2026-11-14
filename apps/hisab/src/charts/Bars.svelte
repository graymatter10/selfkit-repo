<script lang="ts">
  // Spent (and income, faint) bars, e.g. per month or per year.
  import { short } from "../lib/fmt";
  let { items, h = 120, active = "" }: { items: { label: string; key: string; spent: number; income: number }[]; h?: number; active?: string } = $props();
  const W = 340;
  const max = $derived(Math.max(1, ...items.map((i) => Math.max(i.spent, i.income))) * 1.1);
  const bw = $derived(W / Math.max(items.length, 1));
</script>

<svg width="100%" viewBox="0 -14 {W} {h + 30}" style="max-height:{h * 1.6 + 40}px" role="img" aria-label="Spending and income bars">
  {#each items as it, i}
    {@const x = i * bw}
    <rect x={x + bw * 0.16} y={h - (it.income / max) * h} width={bw * 0.32} height={(it.income / max) * h} rx="2" fill="var(--faint)" opacity="0.6" />
    <rect x={x + bw * 0.5} y={h - (it.spent / max) * h} width={bw * 0.32} height={(it.spent / max) * h} rx="2" fill={it.key === active ? "var(--accent)" : "var(--accent)"} opacity={!active || it.key === active ? 1 : 0.45} />
    {#if it.spent > 0 && items.length <= 12}<text x={x + bw * 0.66} y={h - (it.spent / max) * h - 3} font-size="8.5" text-anchor="middle" fill="var(--muted)">{short(it.spent)}</text>{/if}
    <text x={x + bw / 2} y={h + 14} font-size="10" text-anchor="middle" fill="var(--muted)">{it.label}</text>
  {/each}
</svg>
