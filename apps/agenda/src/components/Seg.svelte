<script lang="ts" generics="T extends string | number">
  // iOS-style segmented control.
  let { items, value, onchange, label, small = false }: { items: { id: T; name: string }[]; value: T; onchange: (v: T) => void; label: string; small?: boolean } = $props();
</script>

<div role="radiogroup" aria-label={label} class="flex rounded-[9px] bg-fill p-[2px]">
  {#each items as it}
    <button type="button" role="radio" aria-checked={value === it.id} onclick={() => onchange(it.id)}
      class="flex-1 truncate rounded-[7px] px-1.5 transition-colors duration-150 {small ? 'h-7 text-[13px]' : 'h-8 text-[14px]'} {value === it.id ? 'bg-sel font-semibold shadow-[var(--shadow-sel)]' : 'font-medium text-ink2'}">{it.name}</button>
  {/each}
</div>
