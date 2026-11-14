<script lang="ts">
  // Cumulative daily spending this month vs last month, with budget line.
  import { money, short } from "../lib/fmt";
  let { daily, prev, budget = 0, today = 0, cur, h = 130 }: { daily: number[]; prev: number[]; budget?: number; today?: number; cur: string; h?: number } = $props();
  const W = 340;
  const cum = (a: number[]) => a.reduce<number[]>((o, v) => (o.push((o[o.length - 1] ?? 0) + v), o), []);
  const c = $derived(cum(daily));
  const p = $derived(cum(prev));
  const shown = $derived(today > 0 ? c.slice(0, today) : c);
  const max = $derived(Math.max(1, budget, ...c, ...p) * 1.08);
  const n = $derived(Math.max(daily.length, 2));
  const x = (i: number) => (i / (n - 1)) * W;
  const y = (v: number) => h - (v / max) * h;
  const path = (a: number[]) => a.map((v, i) => `${i ? "L" : "M"}${x(i).toFixed(1)},${y(v).toFixed(1)}`).join("");
</script>

<svg width="100%" viewBox="0 -8 {W} {h + 26}" style="max-height:{h * 1.5 + 40}px" role="img" aria-label="Spending through the month: {money(shown[shown.length - 1] ?? 0, cur)} so far">
  {#if budget > 0}<line x1="0" x2={W} y1={y(budget)} y2={y(budget)} stroke="var(--faint)" stroke-dasharray="3 4" /><text x={W} y={y(budget) - 4} text-anchor="end" font-size="10" fill="var(--muted)">budget {short(budget)}</text>{/if}
  {#if p.length}<path d={path(p.slice(0, n))} stroke="var(--faint)" stroke-width="2" fill="none" />{/if}
  {#if shown.length}
    <path d="{path(shown)}L{x(shown.length - 1)},{h}L0,{h}Z" fill="var(--accent-soft)" />
    <path d={path(shown)} stroke="var(--accent)" stroke-width="2.4" fill="none" stroke-linejoin="round" />
    <circle cx={x(shown.length - 1)} cy={y(shown[shown.length - 1])} r="4" fill="var(--accent)" />
  {/if}
  {#each [1, 8, 15, 22, daily.length] as d}<text x={x(d - 1)} y={h + 16} font-size="10" fill="var(--muted)" text-anchor={d === 1 ? "start" : d === daily.length ? "end" : "middle"}>{d}</text>{/each}
</svg>
