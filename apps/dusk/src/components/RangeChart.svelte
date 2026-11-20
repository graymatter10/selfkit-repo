<script lang="ts">
  // Bed → wake per night on a clock axis (21:00 top → 11:00 bottom, wider when needed), with the
  // bedtime goal as a dashed line. Shows consistency at a glance.
  import type { Night } from "../lib/api";
  import { dayLong, hm, tdisp } from "../lib/fmt";
  let { nights, bedtimeMin, height = 180 }: { nights: Night[]; bedtimeMin: number; height?: number } = $props();
  const rel = (ms: number, tz: number) => {
    const d = new Date(ms + tz * 60000);
    const m = d.getUTCHours() * 60 + d.getUTCMinutes();
    return m >= 18 * 60 ? m - 1440 : m;
  };
  // The axis grows to fit late wakes / early bedtimes (21:00–11:00 by default, at most
  // 18:00–18:00); they were clipped at the edge before (L4-22).
  const rels = $derived(nights.flatMap((x) => [x.bed, x.wake].filter((v): v is number => v != null).map((v) => rel(v, x.tz_offset))));
  const top = $derived(Math.max(-360, Math.min(-180, ...rels.map((r) => Math.floor(r / 180) * 180))));
  const bot = $derived(Math.min(1080, Math.max(660, ...rels.map((r) => Math.ceil(r / 60) * 60))));
  const y = (m: number) => ((Math.min(Math.max(m, top), bot) - top) / (bot - top)) * height;
  const n = $derived(nights.length);
  const bw = $derived(100 / Math.max(1, n));
  const goal = $derived(bedtimeMin >= 18 * 60 ? bedtimeMin - 1440 : bedtimeMin);
  // Labels follow the 12h/24h rule ("9 pm" in 12h; L4-11).
  // Every 3 h inside the axis; labels follow the 12h/24h rule ("9 pm" in 12h; L4-11).
  const TICKS = $derived.by(() => {
    const out: [number, string][] = [];
    for (let m = Math.ceil(top / 180) * 180; m < bot; m += 180) out.push([m, tdisp(`${String((m / 60 + 24) % 24).padStart(2, "0")}:00`).replace(":00 ", " ")]);
    return out;
  });
</script>

<div class="flex gap-2.5">
  <div class="num relative w-9 shrink-0 text-[11px] text-muted" style="height:{height}px" aria-hidden="true">
    {#each TICKS as [m, l]}<span class="absolute right-0" style="top:{y(m) - 7}px">{l}</span>{/each}
  </div>
  <svg viewBox="0 0 100 {height}" preserveAspectRatio="none" class="block flex-1 overflow-visible" style="height:{height}px" role="img" aria-label="Bed and wake times, {n} nights">
    {#each TICKS as [m]}<line x1="0" x2="100" y1={y(m)} y2={y(m)} stroke="var(--sep)" stroke-width="1" vector-effect="non-scaling-stroke" />{/each}
    {#each nights as x, i (x.night)}
      {#if x.bed != null && x.wake != null}
        {@const a = y(rel(x.bed, x.tz_offset))}
        {@const b = y(rel(x.wake, x.tz_offset))}
        <rect x={i * bw + bw * 0.3} y={a} width={bw * 0.4} height={Math.max(1, b - a)} rx="1" fill={i === n - 1 ? "var(--accent)" : "var(--accent2)"}>
          <title>{dayLong(x.night)}: {hm(x.bed, x.tz_offset)} – {hm(x.wake, x.tz_offset)}</title>
        </rect>
      {/if}
    {/each}
    <line x1="0" x2="100" y1={y(goal)} y2={y(goal)} stroke="var(--ink2)" stroke-width="1.2" stroke-dasharray="4 3" vector-effect="non-scaling-stroke" opacity="0.6" />
  </svg>
</div>
