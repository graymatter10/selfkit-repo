<script lang="ts">
  // Full-size photo (tap, Back or Escape closes).
  import { app } from "../lib/state.svelte";
  import Photo from "./Photo.svelte";
</script>

<svelte:window onkeydown={(e) => app.photo && e.key === "Escape" && (e.stopPropagation(), (app.photo = null))} />
{#if app.photo}
  <button type="button" aria-label="Close photo" class="fade" style="position:fixed;inset:0;z-index:90;display:flex;align-items:center;justify-content:center;background:#000;padding:8px" onclick={() => (app.photo = null)}>
    <Photo id={app.photo} fit="contain" />
  </button>
{/if}
