<script lang="ts">
  // Icon from the shared set or Hisab's category set.
  import { ICONS } from "@selfkit/ui";
  import { CAT_ICONS } from "../lib/icons";
  let { name, size = 20, stroke = 1.8, class: cls = "" }: { name: string; size?: number; stroke?: number; class?: string } = $props();
</script>

<svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width={stroke} stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" class={cls}>{@html CAT_ICONS[name] ?? ICONS[name] ?? CAT_ICONS.dot}</svg>
