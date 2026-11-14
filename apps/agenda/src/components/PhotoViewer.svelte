<script lang="ts">
  // Full-screen photo (tap or Back/Escape closes) with "Save to gallery"
  // (Android: Pictures/Selfkit/Agenda; desktop: ~/Pictures/Selfkit/Agenda).
  import { api } from "../lib/api";
  import { SavePhotoButton, saveToGallery } from "../lib/photo-source";
  import { photoUrl } from "../lib/photos";
  import { app } from "../lib/state.svelte";
  import Photo from "./Photo.svelte";
  $effect(() => {
    if (!app.photo) return;
    const prev = app.closeGuard;
    app.closeGuard = () => ((app.photo = null), true);
    return () => (app.closeGuard = prev);
  });
  const save = (id: string) =>
    saveToGallery("Agenda", async () => ({ dataUrl: await photoUrl(id) }), () => api.photoExport(id));
</script>

<svelte:window onkeydown={(e) => app.photo && e.key === "Escape" && (e.stopPropagation(), (app.photo = null))} />
{#if app.photo}
  {@const id = app.photo}
  <div class="fade fixed inset-0 z-[80] bg-black" role="dialog" aria-label="Photo">
    <button type="button" aria-label="Close photo" class="flex h-full w-full items-center justify-center p-2 pt-[max(8px,env(safe-area-inset-top))] pb-[max(76px,calc(env(safe-area-inset-bottom)+70px))]" onclick={() => (app.photo = null)}>
      <Photo {id} fit="contain" />
    </button>
    <SavePhotoButton onsave={() => save(id)} />
  </div>
{/if}
