<script lang="ts" generics="T extends string | number">
  // Segmented control.
  let { items, value, onchange, label }: { items: { id: T; name: string }[]; value: T; onchange: (v: T) => void; label: string } = $props();
</script>

<div role="radiogroup" aria-label={label} class="flex rounded-[12px] bg-fill p-[3px]">
  {#each items as it}
    <button type="button" role="radio" aria-checked={value === it.id} onclick={() => onchange(it.id)}
      class="h-[38px] flex-1 truncate rounded-[9px] px-2.5 text-[14px] transition-colors duration-150 {value === it.id ? 'bg-seg-on font-semibold text-ink shadow-[0_1px_3px_rgba(0,0,0,0.12)]' : 'font-medium text-ink2'}">{it.name}</button>
  {/each}
</div>
