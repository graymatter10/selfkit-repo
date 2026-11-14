<script lang="ts">
  // Stored photo by id (loaded from Rust as bytes → blob URL).
  import { photoUrl } from "../lib/photo";
  let { id, class: cls = "" }: { id: string; class?: string } = $props();
  let url = $state("");
  let failed = $state(false);
  $effect(() => {
    failed = false;
    photoUrl(id).then((u) => (url = u)).catch(() => (failed = true));
  });
</script>

{#if url}<img src={url} alt="Attached" class={cls} />{:else}<div class="{cls} flex items-center justify-center bg-fill text-[11px] text-muted">{failed ? "Not on this device" : ""}</div>{/if}
