<script lang="ts">
  import Icon from "./Icon.svelte";
  let {
    icon,
    label,
    filled = false,
    size = 44,
    onclick,
  }: { icon: string; label: string; filled?: boolean; size?: number; onclick?: (e: MouseEvent) => void } = $props();
</script>

<button
  type="button"
  aria-label={label}
  title={label}
  class="flex shrink-0 cursor-pointer items-center justify-center rounded-sm transition-colors {filled
    ? 'bg-key border border-[var(--sk-key-border)]'
    : 'bg-transparent hover:bg-key'}"
  style="width:{size}px;height:{size}px"
  {onclick}>
  <Icon name={icon} size={size >= 44 ? 21 : 18} />
</button>
