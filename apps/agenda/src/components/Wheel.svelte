<script lang="ts">
  // iOS-style scroll wheel with snapping. `items` are [value, label] pairs.
  let { items, value, onchange, label }: { items: [number, string][]; value: number; onchange: (v: number) => void; label: string } = $props();
  const H = 38;
  let el: HTMLDivElement | undefined = $state();
  let t: ReturnType<typeof setTimeout>;
  let programmatic = false;

  $effect(() => {
    const i = items.findIndex(([v]) => v === value);
    if (el && i >= 0 && Math.round(el.scrollTop / H) !== i) {
      programmatic = true;
      el.scrollTo({ top: i * H });
      setTimeout(() => (programmatic = false), 60);
    }
  });

  function onscroll() {
    if (programmatic) return;
    clearTimeout(t);
    t = setTimeout(() => {
      if (!el) return;
      const i = Math.max(0, Math.min(items.length - 1, Math.round(el.scrollTop / H)));
      if (items[i][0] !== value) onchange(items[i][0]);
    }, 90);
  }
  function key(e: KeyboardEvent) {
    const i = items.findIndex(([v]) => v === value);
    if (e.key === "ArrowDown" && i < items.length - 1) (e.preventDefault(), onchange(items[i + 1][0]));
    if (e.key === "ArrowUp" && i > 0) (e.preventDefault(), onchange(items[i - 1][0]));
  }
</script>

<div class="relative h-[190px] w-[76px]">
  <div class="pointer-events-none absolute inset-x-0 top-[76px] h-[38px] rounded-lg bg-fill"></div>
  <div bind:this={el} {onscroll} role="listbox" tabindex="0" aria-label={label} onkeydown={key}
    class="relative h-full snap-y snap-mandatory overflow-y-scroll py-[76px] [mask-image:linear-gradient(transparent,#000_30%,#000_70%,transparent)]">
    {#each items as [v, l]}
      <button type="button" role="option" aria-selected={v === value} onclick={() => onchange(v)}
        class="flex h-[38px] w-full snap-center items-center justify-center text-[22px] tabular-nums {v === value ? 'font-semibold text-ink' : 'text-muted'}">{l}</button>
    {/each}
  </div>
</div>
