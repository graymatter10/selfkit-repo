<script lang="ts">
  import type { StreakView } from "../lib/api";
  import Src from "./Src.svelte";
  let { s, n = 21, onchange }: { s: StreakView; n?: number; onchange?: () => void } = $props();
  const pat = $derived(s.pattern.slice(-n));
</script>

<div class="card px-4 pt-4 pb-1.5">
  <div class="flex items-center gap-2"><span class="min-w-0 flex-1 text-[15px] leading-tight font-[650] tracking-[-0.012em]">{s.name}</span><Src apps={[s.app]} /></div>
  {#if s.available}
    <div class="num mt-2 flex items-baseline gap-1.5"><b class="text-[30px] leading-none font-[680] tracking-[-0.035em]">{s.current}</b>
      <span class="text-[14px] font-medium text-muted">{s.current === 0 ? "days · start again today" : s.current === 1 ? "day in a row" : "days in a row"}</span></div>
    <div class="mt-3.5 flex gap-1" aria-label="Last {n} days: {s.kept} kept">
      <!-- The last square is today: outlined on every streak, also when it already has a
           value (Asleep by counts last night, so today is never "pending" — P38). -->
      {#each pat as p, i}<i class="aspect-square max-w-[15px] flex-1 rounded-[4px] {p === 'k' ? 'bg-accent' : p === 't' ? 'shadow-[inset_0_0_0_1.5px_var(--accent)]' : p === 'n' ? 'bg-fill2' : 'bg-fill'} {i === pat.length - 1 && p !== 't' ? 'outline outline-[1.5px] outline-offset-1 outline-accent' : ''}"></i>{/each}
    </div>
    <div class="mt-2.5 flex min-h-11 items-center border-t border-sep text-[13px] text-muted">
      <span class="num min-w-0 flex-1 py-1 leading-snug">Best {s.best} {s.best === 1 ? "day" : "days"} · {s.note}</span>
      {#if onchange}<button type="button" class="press h-11 pl-3 text-[13px] font-semibold text-accent" onclick={onchange}>Change</button>{/if}
    </div>
  {:else}
    <p class="mt-2 mb-3 text-[13.5px] text-muted">{s.missing}</p>
  {/if}
</div>
