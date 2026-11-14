<script lang="ts">
  // 1–5 sleep quality. One tap rates (undo in the toast). Desktop: keys 1–5.
  import { QUALITY } from "../lib/fmt";
  let { value, onrate }: { value: number | null; onrate: (q: number) => void } = $props();
</script>

<div role="radiogroup" aria-label="Sleep quality" class="flex gap-2">
  {#each [1, 2, 3, 4, 5] as q}
    <button type="button" role="radio" aria-checked={value === q} aria-label="{q} {QUALITY[q]}" onclick={() => onrate(q)}
      class="press flex h-16 flex-1 flex-col items-center justify-center gap-[3px] rounded-[14px] transition-colors duration-150 {value === q ? 'bg-accent text-on-accent' : 'bg-fill text-ink2'}">
      <span class="num text-[20px] leading-none font-semibold">{q}</span>
      <span class="text-[11.5px] font-medium {value === q ? 'opacity-85' : 'text-muted'}">{QUALITY[q]}</span>
    </button>
  {/each}
</div>
