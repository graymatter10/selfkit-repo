<script lang="ts">
  // Round (square in Paper & Mono) completion control. Priority tints the ring.
  import Icon from "./Icon.svelte";
  let {
    checked = false,
    priority = 0,
    label,
    size = 22,
    onclick,
  }: { checked?: boolean; priority?: number; label: string; size?: number; onclick?: (e: MouseEvent) => void } = $props();
  const tone = $derived(
    checked ? "bg-accent border-accent text-on-accent" : priority === 1 ? "border-danger bg-danger-soft" : priority === 2 ? "border-warn bg-warn-soft" : "border-ring",
  );
</script>

<button
  type="button"
  class="-mx-2.5 flex h-11 w-11 shrink-0 cursor-pointer items-center justify-center bg-transparent"
  aria-label={label}
  aria-pressed={checked}
  {onclick}>
  <span
    class="flex items-center justify-center rounded-check border-2 transition-transform duration-150 active:scale-90 {tone}"
    style="width:{size}px;height:{size}px">
    {#if checked}<Icon name="check" size={size * 0.6} stroke={3} />{/if}
  </span>
</button>
