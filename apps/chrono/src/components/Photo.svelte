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

{#if src}<img {src} alt="" draggable="false" style="width:100%;height:100%;display:block;object-fit:{fit}" />
{:else if failed}<span class="muted" style="display:grid;place-items:center;width:100%;height:100%;font-size:13px">Missing</span>{/if}
