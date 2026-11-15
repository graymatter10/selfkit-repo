<script lang="ts">
  // Full-screen receipt/proof photo with "Save to gallery" (Android:
  // Pictures/Selfkit/Hisab; desktop: ~/Pictures/Selfkit/Hisab).
  import { api } from "../lib/api";
  import { SavePhotoButton, saveToGallery } from "../lib/photo-source";
  import { closeSheet } from "../lib/state.svelte";
  import Ic from "./Ic.svelte";
  import Photo from "./Photo.svelte";
  let { id }: { id: string } = $props();
  const mime = (id: string) => (id.endsWith(".webp") ? "image/webp" : id.endsWith(".png") ? "image/png" : "image/jpeg");
  const save = () => saveToGallery("Hisab", async () => ({ bytes: await api.photoGet(id), mime: mime(id) }), () => api.photoExport(id));
</script>

<div class="fade fixed inset-0 z-[60] flex items-center justify-center bg-black pb-[max(76px,calc(env(safe-area-inset-bottom)+70px))]" role="dialog" aria-label="Photo">
  <button type="button" aria-label="Close photo" class="absolute inset-0" onclick={closeSheet}></button>
  <Photo {id} class="pointer-events-none relative max-h-full max-w-full object-contain" />
  <button type="button" aria-label="Close" class="absolute top-[max(16px,env(safe-area-inset-top))] right-4 flex h-10 w-10 items-center justify-center rounded-full bg-white/15 text-white" onclick={closeSheet}><Ic name="close" size={20} stroke={2.2} /></button>
  <SavePhotoButton onsave={save} />
</div>
