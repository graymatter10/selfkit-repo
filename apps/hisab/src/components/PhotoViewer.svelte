<script lang="ts">
  import { closeSheet } from "../lib/state.svelte";
  import Ic from "./Ic.svelte";
  import Photo from "./Photo.svelte";
  let { id }: { id: string } = $props();
</script>

<div class="fade fixed inset-0 z-[60] flex items-center justify-center bg-black/90" role="dialog" aria-label="Photo">
  <button type="button" aria-label="Close photo" class="absolute inset-0" onclick={closeSheet}></button>
  <Photo {id} class="relative max-h-full max-w-full object-contain" />
  <button type="button" aria-label="Close" class="absolute top-[max(16px,env(safe-area-inset-top))] right-4 flex h-10 w-10 items-center justify-center rounded-full bg-white/15 text-white" onclick={closeSheet}><Ic name="close" size={20} stroke={2.2} /></button>
</div>
