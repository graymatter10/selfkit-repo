<script lang="ts">
  // Donut chart with a legend; hand-made SVG.
  import type { CatAmount } from "../lib/api";
  import { money, short } from "../lib/fmt";
  let { data, cur, label = "Spent", onpick }: { data: CatAmount[]; cur: string; label?: string; onpick?: (id: string) => void } = $props();
  const total = $derived(data.reduce((a, d) => a + d.amount, 0));
  // Top 6, the rest as "Other".
  const items = $derived.by(() => {
    if (data.length <= 7) return data;
    const top = data.slice(0, 6);
    const rest = data.slice(6).reduce((a, d) => a + d.amount, 0);
    return [...top, { id: "", name: "Others", color: "#8a8a8e", icon: "dot", archived: false, amount: rest }];
  });
  const R = 62, C = 80;
  const arcs = $derived.by(() => {
    let a0 = -Math.PI / 2;
    const gap = items.length > 1 ? 0.035 : 0;
    return items.map((d) => {
      const span = total ? (d.amount / total) * Math.PI * 2 : 0;
      const a1 = a0 + Math.max(0, span - gap);
      const p = (a: number) => `${(C + R * Math.cos(a)).toFixed(2)},${(C + R * Math.sin(a)).toFixed(2)}`;
      const path = span >= Math.PI * 2 - 0.001 ? `M${C},${C - R} a${R},${R} 0 1 1 0,${2 * R} a${R},${R} 0 1 1 0,${-2 * R}` : `M${p(a0)} A${R},${R} 0 ${a1 - a0 > Math.PI ? 1 : 0} 1 ${p(a1)}`;
      a0 += span;
      return { d, path };
    });
  });
</script>

{#if total > 0}
  <div class="flex items-center gap-4">
    <svg width="150" height="150" viewBox="0 0 160 160" class="shrink-0" role="img" aria-label="{label} by category">
      {#each arcs as a}<path d={a.path} stroke={a.d.color} stroke-width="22" fill="none" />{/each}
      <text x="80" y="74" text-anchor="middle" font-size="12" fill="var(--muted)">{label}</text>
      <text x="80" y="95" text-anchor="middle" font-size="19" font-weight="650" fill="var(--ink)">{short(total)}</text>
    </svg>
    <div class="flex min-w-0 flex-1 flex-col gap-1.5 text-[13px]">
      {#each items as d}
        <button type="button" class="press flex items-center gap-2 text-left" onclick={() => d.id && onpick?.(d.id)}>
          <span class="h-2.5 w-2.5 shrink-0 rounded-[3px]" style="background:{d.color}"></span>
          <span class="min-w-0 flex-1 truncate {d.archived ? 'text-muted' : ''}">{d.name}</span>
          <span class="num font-semibold">{money(d.amount, cur)}</span>
        </button>
      {/each}
    </div>
  </div>
{:else}<p class="m-0 py-6 text-center text-[14px] text-muted">Nothing spent in this period.</p>{/if}
