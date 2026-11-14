<script lang="ts">
  // 24-hour strip: when any screen was in use (merged across devices).
  let { strip, from, to }: { strip: [number, number][]; from: number; to: number } = $props();
  let w = $state(0);
  const span = $derived(to - from);
  const x = (t: number) => ((t - from) / span) * w;
  const startH = $derived(new Date(from).getHours());
  const now = $derived(Date.now());
</script>

<div bind:clientWidth={w} style="height:44px">
  {#if w > 0}
    <svg width={w} height="44" viewBox="0 0 {w} 44" style="display:block" role="img" aria-label="Screen use through the day">
      <rect x="0" y="4" width={w} height="20" rx="6" fill="var(--track)" />
      {#each strip as [a, b]}
        <rect x={x(Math.max(a, from))} y="4" width={Math.max(2.5, x(Math.min(b, to)) - x(Math.max(a, from)))} height="20" rx="3" fill="var(--accent)" />
      {/each}
      {#if now > from && now < to}<line x1={x(now)} x2={x(now)} y1="1" y2="27" stroke="var(--ink)" stroke-width="1.5" />{/if}
      {#each [0, 6, 12, 18, 24] as h}
        <text x={h === 0 ? 0 : h === 24 ? w : (h / 24) * w} y="41" font-size="12" fill="var(--muted)" text-anchor={h === 0 ? "start" : h === 24 ? "end" : "middle"}>{String((startH + h) % 24).padStart(2, "0")}:00</text>
      {/each}
    </svg>
  {/if}
</div>
