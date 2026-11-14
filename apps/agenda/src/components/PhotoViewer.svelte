<script lang="ts">
  // Full-screen photo (tap or Back/Escape closes).
  import { app } from "../lib/state.svelte";
  import Photo from "./Photo.svelte";
  $effect(() => {
    if (!app.photo) return;
    const prev = app.closeGuard;
    app.closeGuard = () => ((app.photo = null), true);
    return () => (app.closeGuard = prev);
  });
</script>

<svelte:window onkeydown={(e) => app.photo && e.key === "Escape" && (e.stopPropagation(), (app.photo = null))} />
{#if app.photo}
  <button type="button" aria-label="Close photo" class="fade fixed inset-0 z-[80] flex items-center justify-center bg-black p-2 pt-[max(8px,env(safe-area-inset-top))]" onclick={() => (app.photo = null)}>
    <Photo id={app.photo} fit="contain" />
  </button>
{/if}
