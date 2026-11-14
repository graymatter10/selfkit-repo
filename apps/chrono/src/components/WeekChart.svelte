<script lang="ts">
  // Weekly report: 7 bars, busiest and quietest highlighted and labelled.
  import { dur } from "../lib/fmt";
  let { values, labels, phone }: { values: number[]; labels: string[]; phone: boolean } = $props();
  let W = $state(0);
  const H = $derived(phone ? 120 : 140);
  const max = $derived(Math.max(1, ...values) * 1.05);
  const act = $derived(values.map((v, i) => [v, i]).filter(([v]) => v > 0));
  const busiest = $derived(act.length ? act.reduce((a, b) => (b[0] > a[0] ? b : a))[1] : -1);
  const quiet = $derived(act.length > 1 ? act.reduce((a, b) => (b[0] < a[0] ? b : a))[1] : -1);
  const bw = $derived(W / 7);
  const w = $derived(Math.min(36, bw * 0.56));
</script>

<div bind:clientWidth={W}>
  {#if W > 0}
    <svg width={W} height={H + 44} viewBox="0 -18 {W} {H + 44}" style="display:block">
      <line x1="0" x2={W} y1={H} y2={H} stroke="var(--line)" />
      {#each values as v, i}
        {@const h = (v / max) * H}
        {@const cx = i * bw + bw / 2}
        {@const hi = i === busiest || i === quiet}
        <rect x={cx - w / 2} y={H - h} width={w} height={h} rx="4" fill={hi ? "var(--accent)" : "var(--accent-bar)"} />
        {#if hi}<text x={cx} y={H - h - 7} font-size="12" fill="var(--ink2)" text-anchor="middle" font-weight="550">{dur(v)}</text>{/if}
        <text x={cx} y={H + 18} font-size="12" fill="var(--muted)" text-anchor="middle">{labels[i]}</text>
      {/each}
    </svg>
  {/if}
</div>
