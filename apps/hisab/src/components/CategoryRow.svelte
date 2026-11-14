<script lang="ts">
  // Sliding category row: ranked categories, "+ New" at the end, and a fade
  // on the edge that has more to show.
  import type { Category } from "../lib/api";
  import CatBadge from "./CatBadge.svelte";
  import Ic from "./Ic.svelte";
  let { cats, value, onpick, onnew, splitIds = [] }: { cats: Category[]; value: string; onpick: (id: string) => void; onnew: () => void; splitIds?: string[] } = $props();
  let el: HTMLDivElement | undefined = $state();
  let fade = $state("fade-r");
  function edges() {
    if (!el) return;
    const l = el.scrollLeft > 4, r = el.scrollLeft + el.clientWidth < el.scrollWidth - 4;
    fade = l && r ? "fade-lr" : l ? "fade-l" : r ? "fade-r" : "";
  }
  $effect(() => {
    void cats.length;
    queueMicrotask(edges);
  });
  // Keep the picked category in view.
  $effect(() => {
    const b = el?.querySelector<HTMLElement>(`[data-id="${value}"]`);
    b?.scrollIntoView({ inline: "nearest", block: "nearest" });
  });
</script>

<div bind:this={el} onscroll={edges} class="flex gap-1 overflow-x-auto {fade}" role="radiogroup" aria-label="Category">
  {#each cats as c (c.id)}
    {@const on = value === c.id || splitIds.includes(c.id)}
    <button type="button" role="radio" aria-checked={on} data-id={c.id} onclick={() => onpick(c.id)}
      class="press flex w-[68px] shrink-0 flex-col items-center gap-1 rounded-[14px] pt-1.5 pb-1 {on ? 'bg-accent-soft' : ''}">
      <CatBadge icon={c.icon} color={c.color} size={36} />
      <span class="w-full truncate px-0.5 text-center text-[12px] {on ? 'font-semibold text-ink' : 'text-ink2'}">{c.name}</span>
    </button>
  {/each}
  <button type="button" onclick={onnew} class="press flex w-[68px] shrink-0 flex-col items-center gap-1 pt-1.5 pb-1">
    <span class="flex h-9 w-9 items-center justify-center rounded-full border border-dashed border-faint text-muted"><Ic name="plus" size={18} stroke={2} /></span>
    <span class="text-[12px] text-muted">New</span>
  </button>
</div>
