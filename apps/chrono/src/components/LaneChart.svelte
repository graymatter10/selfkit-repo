<script lang="ts">
  // Combined timeline: one lane per device / browser, blocks coloured by
  // category. Tap a block to select it (details are shown by the parent).
  import type { Lane } from "../lib/api";
  import { catColor } from "../lib/fmt";
  let { lanes, from, to, phone, selected = null, onselect }: {
    lanes: Lane[]; from: number; to: number; phone: boolean;
    selected?: { lane: number; i: number } | null; onselect: (s: { lane: number; i: number } | null) => void;
  } = $props();
  let W = $state(0);
  const LH = $derived(phone ? 50 : 58), BH = $derived(phone ? 20 : 22);
  // visible window: from 06:00 (or the first block, if earlier) to the end of the day
  const view = $derived.by(() => {
    const first = Math.min(...lanes.flatMap((l) => l.segs.map((s) => s[0])), to);
    const six = from + 6 * 3600000;
    const a = Math.min(six, Math.floor((first - from) / 3600000) * 3600000 + from);
    return [Math.max(from, a), to] as const;
  });
  const x = (t: number) => ((t - view[0]) / (view[1] - view[0])) * W;
  const ticks = $derived.by(() => {
    const hours = (view[1] - view[0]) / 3600000;
    const step = phone ? (hours > 12 ? 6 : 3) : hours > 12 ? 3 : 2;
    const out: number[] = [];
    for (let h = 0; h <= hours + 0.01; h += step) out.push(view[0] + h * 3600000);
    return out;
  });
  const H = $derived(lanes.length * LH);
  const lbl = (t: number) => { const d = new Date(t); return t >= to ? "24:00" : `${String(d.getHours()).padStart(2, "0")}:00`; };
</script>

<div bind:clientWidth={W}>
  {#if W > 0}
    <svg width={W} height={H + 24} viewBox="0 0 {W} {H + 24}" style="display:block;overflow:visible">
      {#each ticks as t, i}
        <line x1={x(t)} y1="0" x2={x(t)} y2={H} stroke="var(--line)" stroke-dasharray="2 3" />
        <text x={x(t)} y={H + 18} font-size="12" fill="var(--muted)" text-anchor={i === 0 ? "start" : i === ticks.length - 1 ? "end" : "middle"}>{lbl(t)}</text>
      {/each}
      {#each lanes as lane, li}
        {@const y = li * LH + 22}
        <text x="0" y={y - 7} font-size="13" fill="var(--ink2)" font-weight="550">{lane.name}</text>
        <rect x="0" {y} width={W} height={BH} rx="5" fill="var(--track)" />
        {#each lane.segs as [a, b, name, cat], i}
          {@const xa = x(Math.max(a, view[0]))}
          {@const wd = Math.max(3, x(b) - xa - 1.5)}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <g role="button" tabindex="-1" aria-label={name} style="cursor:pointer" onclick={() => onselect(selected?.lane === li && selected.i === i ? null : { lane: li, i })}>
            <rect x={xa - 4} y={y - 8} width={wd + 8} height={BH + 16} fill="transparent" />
            <rect x={xa} {y} width={wd} height={BH} rx="3" fill={catColor(cat)} />
          </g>
          {#if selected?.lane === li && selected.i === i}
            <rect x={xa - 2.5} y={y - 2.5} width={wd + 5} height={BH + 5} rx="5" fill="none" stroke="var(--ink)" stroke-width="2" pointer-events="none" />
          {/if}
        {/each}
      {/each}
    </svg>
  {/if}
</div>
