<script lang="ts" generics="T extends string">
  // Segmented control.
  let { items, value, onchange, label, small = false }: { items: { id: T; name: string }[]; value: T; onchange: (v: T) => void; label: string; small?: boolean } = $props();
</script>

<div role="radiogroup" aria-label={label} class="flex rounded-[11px] bg-fill p-[3px]">
  {#each items as it}
    <button type="button" role="radio" aria-checked={value === it.id} onclick={() => onchange(it.id)}
      class="flex-1 truncate rounded-[8px] px-2 transition-colors duration-150 {small ? 'h-7 text-[13px]' : 'h-8 text-[14px]'} {value === it.id ? 'bg-card font-semibold text-ink shadow-[0_1px_3px_rgba(0,0,0,0.12)]' : 'font-medium text-muted'}">{it.name}</button>
  {/each}
</div>
