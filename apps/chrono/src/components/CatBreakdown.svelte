<script lang="ts">
  // "Where the time went": stacked bar + labelled rows + Focus sentence.
  import { CATS, catColor, catName, dur } from "../lib/fmt";
  let { cats, focus, compact = false, focusNote = "of your time was Work or Study" }: { cats: Record<string, number>; focus: number | null; compact?: boolean; focusNote?: string } = $props();
  const list = $derived(
    [...CATS.filter((c) => (cats[c] ?? 0) > 0), ...Object.keys(cats).filter((c) => !(CATS as readonly string[]).includes(c) && cats[c] > 0)]
      .map((c) => [c, cats[c]] as [string, number])
      .sort((a, b) => (a[0] === "other" ? 1 : b[0] === "other" ? -1 : b[1] - a[1])),
  );
  const tot = $derived(list.reduce((a, b) => a + b[1], 0));
</script>

{#if tot > 0}
  <div style="display:flex;gap:3px;height:12px;border-radius:6px;overflow:hidden;margin:0 0 {compact ? 14 : 6}px">
    {#each list as [c, v]}<i style="flex:{v};background:{catColor(c)}"></i>{/each}
  </div>
  {#if compact}
    <div class="legend" style="gap:10px 20px;font-size:14px">
      {#each list as [c, v]}<span><i class="sw" style="background:{catColor(c)}"></i>{catName(c)} <b class="num" style="font-weight:600;color:var(--ink)">{dur(v)}</b></span>{/each}
    </div>
  {:else}
    {#each list as [c, v]}
      <div class="row" style="min-height:40px"><i class="sw" style="background:{catColor(c)}"></i><span style="flex:1;font-size:14.5px">{catName(c)}</span><span class="muted num" style="font-size:13px;width:44px;text-align:right">{Math.round((v / tot) * 100)}%</span><span class="val" style="width:62px;text-align:right">{dur(v)}</span></div>
    {/each}
  {/if}
  {#if focus != null}
    <div class="sel" style="margin-top:{compact ? 16 : 10}px"><span class="num" style="font-weight:600;font-size:15px;white-space:nowrap">Focus {focus}%</span><span class="muted" style="font-size:13px">{focusNote}</span></div>
  {/if}
{:else}
  <p class="hint" style="margin:0">Nothing recorded yet.</p>
{/if}
