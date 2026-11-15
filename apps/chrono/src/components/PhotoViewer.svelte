<script lang="ts">
  // Full-size photo (tap, Back or Escape closes) with "Save to gallery"
  // (Android: Pictures/Selfkit/Chrono; desktop: ~/Pictures/Selfkit/Chrono).
  import { api } from "../lib/api";
  import { SavePhotoButton, saveToGallery } from "../lib/photo-source";
  import { photoUrl } from "../lib/photos";
  import { app, applyTheme } from "../lib/state.svelte";
  import Photo from "./Photo.svelte";
  // Android: light status-bar icons on the black viewer (the clock was
  // invisible); the app theme's colours come back when it closes.
  $effect(() => {
    if (!app.photo) return;
    (window as unknown as { ChronoBars?: { setDark(d: boolean): void } }).ChronoBars?.setDark(true);
    return () => applyTheme();
  });
  const save = (id: string) =>
    saveToGallery("Chrono", async () => ({ dataUrl: await photoUrl(id) }), () => api.photoExport(id));
</script>

<svelte:window onkeydown={(e) => app.photo && e.key === "Escape" && (e.stopPropagation(), (app.photo = null))} />
{#if app.photo}
  {@const id = app.photo}
  <div class="fade" role="dialog" aria-label="Photo" style="position:fixed;inset:0;z-index:90;background:#000">
    <button type="button" aria-label="Close photo" style="width:100%;height:100%;display:flex;align-items:center;justify-content:center;padding:8px 8px max(76px,calc(env(safe-area-inset-bottom) + 70px))" onclick={() => (app.photo = null)}>
      <Photo {id} fit="contain" />
    </button>
    <SavePhotoButton onsave={() => save(id)} />
  </div>
{/if}
