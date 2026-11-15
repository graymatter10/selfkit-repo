<script lang="ts">
  // One stored photo (loaded through Rust as a data: URL, cached).
  import { photoUrl } from "../lib/photos";
  let { id, fit = "cover" }: { id: string; fit?: "cover" | "contain" } = $props();
  let src = $state("");
  let failed = $state(false);
  $effect(() => {
    const want = id;
    photoUrl(want).then((u) => want === id && (src = u)).catch(() => (failed = true));
  });
</script>

{#if src}<img {src} alt="" draggable="false" class="block h-full w-full {fit === 'cover' ? 'object-cover' : 'object-contain'}" />
{:else if failed}<span class="flex h-full w-full items-center justify-center text-[13px] text-muted">Missing</span>{/if}
