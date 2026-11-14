<script lang="ts" generics="T">
  // Virtualized list with fixed row height, scrolled by an outer container
  // (the screen's content area), so months of rows stay cheap.
  import type { Snippet } from "svelte";
  let { items, rowH, row, scroller }: { items: T[]; rowH: number; row: Snippet<[T, number]>; scroller: HTMLElement | null } = $props();
  let el: HTMLDivElement | undefined = $state();
  let top = $state(0), viewH = $state(800);
  function measure() {
    if (!el || !scroller) return;
    const a = el.getBoundingClientRect(), b = scroller.getBoundingClientRect();
    top = b.top - a.top;
    viewH = b.height;
  }
  $effect(() => {
    if (!scroller) return;
    measure();
    const f = () => measure();
    scroller.addEventListener("scroll", f, { passive: true });
    window.addEventListener("resize", f);
    return () => { scroller.removeEventListener("scroll", f); window.removeEventListener("resize", f); };
  });
  $effect(() => { void items.length; queueMicrotask(measure); });
  const first = $derived(Math.max(0, Math.floor(top / rowH) - 8));
  const last = $derived(Math.min(items.length, Math.ceil((top + viewH) / rowH) + 8));
</script>

<div bind:this={el} style="position:relative;height:{items.length * rowH}px">
  {#each items.slice(first, last) as it, k (first + k)}
    <div style="position:absolute;left:0;right:0;top:{(first + k) * rowH}px;height:{rowH}px">{@render row(it, first + k)}</div>
  {/each}
</div>
