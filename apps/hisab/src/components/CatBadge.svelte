<script lang="ts">
  // Round tinted badge with a category (or account) icon.
  import Ic from "./Ic.svelte";
  let { icon, color, size = 36 }: { icon: string; color: string; size?: number } = $props();
</script>

<span class="flex shrink-0 items-center justify-center rounded-full" style="width:{size}px;height:{size}px;background:color-mix(in srgb, {color} 16%, transparent);color:{color}">
  <Ic name={icon} size={Math.round(size * 0.52)} stroke={1.9} />
</span>
