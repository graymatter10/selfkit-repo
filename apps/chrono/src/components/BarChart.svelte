<script lang="ts">
  // Single-series bars (screen time per bucket) with a dashed average line.
  // Tap a bar to select it; the selected bar is the solid accent.
  import { dur } from "../lib/fmt";
  let { values, labels, selected, onselect, avg, phone, future = 0, showLabel }: {
    values: number[]; labels: string[]; selected: number; onselect: (i: number) => void; avg: number; phone: boolean;
    future?: number; showLabel: (i: number) => boolean;
  } = $props();
  let W = $state(0);
  const H = $derived(phone ? 150 : 180);
  const G = 40;
  const n = $derived(values.length + future);
  const bw = $derived((W - G) / Math.max(1, n));
  const maxH = $derived(Math.max(1, ...values, avg) / 3600000);
  const step = $derived(maxH > 16 ? Math.ceil(maxH / 2 / 4) * 4 : maxH > 8 ? 4 : maxH > 4 ? 2 : 1);
  const top = $derived(Math.ceil(maxH / step) * step);
  const ys = $derived([step, top].filter((v, i, a) => a.indexOf(v) === i && v > 0));
  const y = (ms: number) => H - (ms / 3600000 / top) * H;
</script>

<div bind:clientWidth={W}>
  {#if W > 0}
    <svg width={W} height={H + 24} viewBox="0 0 {W} {H + 24}" style="display:block;overflow:visible">
      {#each ys as v}
        <line x1="0" x2={W - G} y1={y(v * 3600000)} y2={y(v * 3600000)} stroke="var(--line2)" />
        <text x={W} y={y(v * 3600000) + 4} font-size="12" fill="var(--muted)" text-anchor="end">{v} h</text>
      {/each}
      <line x1="0" x2={W - G} y1={H} y2={H} stroke="var(--line)" />
      {#each values as v, i}
        {@const h = Math.max(v > 0 ? 2 : 0, H - y(v))}
        {@const w = Math.max(1, bw - (bw > 8 ? 3 : 1))}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <g role="button" tabindex="-1" aria-label="{labels[i]}: {dur(v)}" style="cursor:pointer" onclick={() => onselect(i)}>
          <rect x={i * bw} y="0" width={bw} height={H} fill="transparent" />
          <rect x={i * bw + (bw - w) / 2} y={H - h} width={w} height={h} rx={Math.min(3, w / 2)} fill={i === selected ? "var(--accent)" : "var(--accent-bar)"} />
        </g>
      {/each}
      {#each Array(future) as _, k}
        <rect x={(values.length + k) * bw + 1.5} y={H - 3} width={Math.max(1, bw - 3)} height="3" rx="1.5" fill="var(--track)" />
      {/each}
      {#if avg > 0}
        <line x1="0" x2={W - G} y1={y(avg)} y2={y(avg)} stroke="var(--ink2)" stroke-width="1.2" stroke-dasharray="4 4" pointer-events="none" />
        <text x={Math.min(W - G - 120, Math.max(4, values.length * bw + 8 > W - G - 120 ? 4 : values.length * bw + 8))} y={y(avg) - 7} font-size="12" fill="var(--ink2)" pointer-events="none">average {dur(avg)}</text>
      {/if}
      {#each labels as l, i}
        {#if showLabel(i)}
          <text x={i * bw + bw / 2} y={H + 18} font-size="12" fill={i === selected ? "var(--ink)" : "var(--muted)"} font-weight={i === selected ? 600 : 400} text-anchor="middle">{l}</text>
        {/if}
      {/each}
    </svg>
  {/if}
</div>
