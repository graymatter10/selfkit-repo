<script lang="ts">
  import { ICONS } from "../icons";
  let { name, size = 20, stroke = 1.75, class: cls = "" }: { name: string; size?: number; stroke?: number; class?: string } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width={stroke}
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  class={cls}>{@html ICONS[name] ?? ""}</svg>
