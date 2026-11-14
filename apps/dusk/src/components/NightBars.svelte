<script lang="ts">
  // Nights as bars (asleep + nap on top), dashed goal line, the last (or
  // selected) night highlighted. Tap/click a bar to open that night.
  import type { Night } from "../lib/api";
  import { dayNum, dur, dayLong, wd2 } from "../lib/fmt";
  let { nights, goal, height = 96, labels = "weekday", dots = false, highlight, onpick }:
    { nights: Night[]; goal: number; height?: number; labels?: "weekday" | "day" | "sparse"; dots?: boolean; highlight?: string; onpick?: (n: string) => void } = $props();

  const n = $derived(nights.length);
  const max = $derived(Math.max(600, goal + 60, ...nights.map((x) => x.asleep_min + x.nap_min + 20)));
  const y = (m: number) => height - (m / max) * height;
  const bw = $derived(100 / Math.max(1, n));
  const hi = $derived(highlight ?? nights[n - 1]?.night);
  const hasNotes = (x: Night) => !!(x.log?.note_before || x.log?.note_after);
  function label(x: Night, i: number) {
    if (labels === "weekday") return wd2(x.night);
    if (labels === "day") return dayNum(x.night);
    const d = Number(dayNum(x.night));
    return i === n - 1 || d % 7 === 1 ? String(d) : "";
  }
  const rx = $derived(n > 30 ? 0.3 : n > 14 ? 0.6 : 1.2);
</script>

<div class="relative">
  <svg viewBox="0 0 100 {height}" preserveAspectRatio="none" class="block w-full" style="height:{height}px" role="img"
    aria-label="Sleep per night, {n} nights, goal {dur(goal)}">
    {#each nights as x, i (x.night)}
      {@const x0 = i * bw + bw * 0.2}
      {@const w = bw * 0.6}
      <g class={onpick ? "cursor-pointer" : ""} role={onpick ? "button" : undefined} onclick={() => onpick?.(x.night)} onkeydown={() => {}}>
        <title>{dayLong(x.night)}: {x.asleep_min ? dur(x.asleep_min) : x.source === "allnighter" ? "no sleep" : "no data"}{x.nap_min ? ` + ${dur(x.nap_min)} nap` : ""}</title>
        <rect x={i * bw} y="0" width={bw} height={height} fill="transparent" />
        {#if x.asleep_min > 0}
          <rect x={x0} y={y(x.asleep_min)} width={w} height={height - y(x.asleep_min)} rx={rx} fill={x.night === hi ? "var(--accent)" : "var(--accent2)"} />
        {/if}
        {#if x.nap_min > 0}
          <rect x={x0} y={y(x.asleep_min + x.nap_min)} width={w} height={Math.max(0.5, (x.nap_min / max) * height - 1.2)} rx={rx} fill="var(--faint)" />
        {/if}
      </g>
    {/each}
    <line x1="0" x2="100" y1={y(goal)} y2={y(goal)} stroke="var(--ink2)" stroke-width="1.2" stroke-dasharray="4 3" vector-effect="non-scaling-stroke" opacity="0.6" />
  </svg>
  <span class="num absolute right-0 bg-card pl-1 text-[11px] font-semibold text-muted" style="top:{y(goal) - 17}px">{dur(goal).replace(" 00m", "")} goal</span>
</div>
<div class="num mt-2 flex text-[11px] text-muted" aria-hidden="true">
  {#each nights as x, i (x.night)}<span class="flex-1 text-center {x.night === hi ? 'font-[650] text-ink' : ''}">{label(x, i)}</span>{/each}
</div>
{#if dots}
  <div class="mt-[3px] flex h-2" aria-hidden="true">
    {#each nights as x (x.night)}<span class="flex flex-1 justify-center">{#if hasNotes(x)}<i class="h-1 w-1 rounded-full bg-muted"></i>{/if}</span>{/each}
  </div>
{/if}
