<script lang="ts" generics="T extends string">
  let { items, value, onchange, label, style = "" }: { items: { id: T; name: string }[]; value: T; onchange: (v: T) => void; label: string; style?: string } = $props();
</script>

<div class="seg" role="radiogroup" aria-label={label} {style}>
  {#each items as it}
    <button type="button" role="radio" aria-checked={value === it.id} class:on={value === it.id} onclick={() => onchange(it.id)}>{it.name}</button>
  {/each}
</div>
