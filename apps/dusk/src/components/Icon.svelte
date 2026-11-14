<script lang="ts">
  import { ICONS } from "../lib/icons";
  let { name, size = 22, stroke = 1.9 }: { name: string; size?: number; stroke?: number } = $props();
</script>

<svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width={stroke} stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">{@html ICONS[name] ?? ""}</svg>
