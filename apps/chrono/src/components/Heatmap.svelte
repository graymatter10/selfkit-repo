<script lang="ts">
  // Hour × weekday: average screen time per hour (4 steps of teal).
  // Tap a cell to see its value.
  import { dur } from "../lib/fmt";
  let { data, phone, selected = null, onselect }: { data: number[][]; phone: boolean; selected?: [number, number] | null; onselect: (c: [number, number] | null) => void } = $props();
  const DAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
  let W = $state(0);
  const from = $derived(phone && data.every((r) => r.slice(0, 6).every((v) => v < 60000)) ? 6 : 0);
  const cols = $derived(24 - from);
  const lw = 40, gap = 3;
  const cell = $derived(Math.min(24, (W - lw - gap * cols) / cols));
  const rowH = $derived(Math.max(15, Math.min(cell, 22)));
  const max = $derived(Math.max(1, ...data.flat()));
  function level(v: number, m: number) {
    if (v < 60000) return 0;
    const r = v / m;
    return r > 0.66 ? 3 : r > 0.33 ? 2 : 1;
  }
  const labels = $derived([from, 6, 12, 18].filter((v, i, a) => a.indexOf(v) === i && v >= from));
</script>

<div bind:clientWidth={W}>
  {#if W > 0}
    <svg width={W} height={7 * (rowH + gap) + 20} viewBox="0 0 {W} {7 * (rowH + gap) + 20}" style="display:block">
      {#each DAYS as d, r}
        <text x="0" y={r * (rowH + gap) + rowH * 0.72} font-size="12" fill="var(--muted)">{d}</text>
        {#each Array(cols) as _, c}
          {@const h = c + from}
          {@const v = data[r]?.[h] ?? 0}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <rect role="button" tabindex="-1" aria-label="{d} {h}:00 {dur(v)}" style="cursor:pointer" x={lw + c * (cell + gap)} y={r * (rowH + gap)} width={cell} height={rowH} rx="3"
            fill="var(--h{level(v, max)})" stroke={selected && selected[0] === r && selected[1] === h ? "var(--ink)" : "none"} stroke-width="2"
            onclick={() => onselect(selected && selected[0] === r && selected[1] === h ? null : [r, h])} />
        {/each}
      {/each}
      {#each labels as h}
        <text x={lw + (h - from) * (cell + gap)} y={7 * (rowH + gap) + 15} font-size="12" fill="var(--muted)">{String(h).padStart(2, "0")}:00</text>
      {/each}
    </svg>
  {/if}
</div>
