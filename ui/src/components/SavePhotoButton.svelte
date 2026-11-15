<script lang="ts">
  // "Save to gallery" pill for the full-screen photo viewers (always on black).
  // The app passes `onsave` (usually saveToGallery from photo-source.svelte.ts).
  // Docs: docs/components/photo-source.md.
  let { onsave }: { onsave: () => Promise<boolean> } = $props();
  let busy = $state(false), done = $state(false);
  // Android saves into the phone's gallery; desktop copies into ~/Pictures/Selfkit.
  const phone = "SelfkitPhotos" in window;
  const label = phone ? "Save to gallery" : "Save to Pictures";
  async function run(e: MouseEvent) {
    e.stopPropagation();
    if (busy) return;
    busy = true;
    try {
      done = await onsave();
    } finally {
      busy = false;
    }
    if (done) setTimeout(() => (done = false), 2200);
  }
</script>

<button type="button" class="sp" class:top={!phone} disabled={busy} onclick={run} aria-label={label}>
  {#if done}
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m5 12.5 4.5 4.5L19 7.5" /></svg>Saved
  {:else}
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 4v11m0 0-4-4m4 4 4-4M5 19.5h14" /></svg>{busy ? "Saving…" : label}
  {/if}
</button>

<style>
  .sp {
    position: absolute; left: 50%; bottom: max(20px, calc(env(safe-area-inset-bottom) + 14px)); transform: translateX(-50%); z-index: 2;
    display: flex; align-items: center; gap: 8px; height: 44px; padding: 0 18px; border: 0; border-radius: 22px;
    background: rgba(255, 255, 255, 0.16); color: #fff; font: inherit; font-size: 15px; font-weight: 600; white-space: nowrap;
    backdrop-filter: blur(10px); -webkit-backdrop-filter: blur(10px); cursor: pointer; -webkit-tap-highlight-color: transparent;
  }
  /* Desktop: top centre, so the bottom toast doesn't cover it. */
  .sp.top { top: 16px; bottom: auto; }
  .sp:active { background: rgba(255, 255, 255, 0.26); }
  .sp:disabled { opacity: 0.7; }
</style>
