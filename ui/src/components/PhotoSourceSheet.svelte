<script lang="ts">
  // "Add photo" source sheet (Android only): Take photo / Choose from gallery /
  // Cancel. Mounted once in each app's App.svelte; opened by askPhotoSource().
  // Plain scoped CSS on the app's own tokens (--card or --surface, --ink,
  // --muted, --accent, --accent-soft, --scrim), so it takes each app's accent
  // and follows its light/dark theme. Docs: docs/components/photo-source.md.
  import { choosePhotoSource, closePhotoSource, photoSource } from "../photo-source.svelte";
  let { privateNote = "Stays private in the app" }: { privateNote?: string } = $props();
</script>

<svelte:window onkeydown={(e) => photoSource.open && e.key === "Escape" && (e.stopPropagation(), closePhotoSource())} />
{#if photoSource.open}
  <div class="ps-root" role="presentation">
    <button type="button" class="ps-scrim" aria-label="Cancel" onclick={closePhotoSource}></button>
    <div class="ps-sheet" role="dialog" aria-modal="true" aria-label="Add photo">
      <div class="ps-grab" aria-hidden="true"></div>
      <div class="ps-title">Add photo</div>
      <div class="ps-group">
        <button type="button" class="ps-row" onclick={() => choosePhotoSource("camera")}>
          <span class="ps-ic" aria-hidden="true">
            <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M4 8.5A2.5 2.5 0 0 1 6.5 6h1.3l1.4-2h5.6l1.4 2h1.3A2.5 2.5 0 0 1 20 8.5v9a2.5 2.5 0 0 1-2.5 2.5h-11A2.5 2.5 0 0 1 4 17.5z" /><circle cx="12" cy="12.8" r="3.6" /></svg>
          </span>
          <span class="ps-txt"><span class="ps-main">Take photo</span><span class="ps-sub">{privateNote}</span></span>
        </button>
        <div class="ps-sep"></div>
        <button type="button" class="ps-row" onclick={() => choosePhotoSource("gallery")}>
          <span class="ps-ic" aria-hidden="true">
            <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><rect x="3.5" y="4.5" width="17" height="15" rx="2.5" /><circle cx="9" cy="9.5" r="1.6" /><path d="m4 17 4.6-4.6a1.5 1.5 0 0 1 2.1 0L16 17.7M14 15.5l1.6-1.6a1.5 1.5 0 0 1 2.1 0L20 16.2" /></svg>
          </span>
          <span class="ps-txt"><span class="ps-main">Choose from gallery</span><span class="ps-sub">Pick photos already on the phone</span></span>
        </button>
      </div>
      <button type="button" class="ps-cancel" onclick={closePhotoSource}>Cancel</button>
    </div>
  </div>
{/if}

<style>
  .ps-root { position: fixed; inset: 0; z-index: 120; display: flex; flex-direction: column; justify-content: flex-end; }
  .ps-scrim { position: absolute; inset: 0; border: 0; padding: 0; background: var(--scrim, rgba(0, 0, 0, 0.4)); animation: ps-fade 160ms ease-out; cursor: default; }
  .ps-sheet {
    position: relative; margin: 0 auto; width: 100%; max-width: 520px; box-sizing: border-box;
    padding: 8px 12px max(14px, calc(env(safe-area-inset-bottom) + 10px));
    background: var(--bg, var(--page, #f4f4f4)); color: var(--ink, #111);
    border-radius: 22px 22px 0 0; box-shadow: 0 -8px 30px rgba(0, 0, 0, 0.18), inset 0 1px 0 var(--sep, var(--line, transparent));
    animation: ps-rise 220ms cubic-bezier(0.2, 0.9, 0.25, 1);
  }
  .ps-grab { width: 36px; height: 5px; border-radius: 3px; margin: 2px auto 10px; background: var(--faint, #c7c7cc); }
  .ps-title { padding: 0 6px 10px; font-size: 13px; font-weight: 600; color: var(--muted, #888); letter-spacing: 0.01em; }
  .ps-group { overflow: hidden; border-radius: 16px; background: var(--card, var(--surface, #fff)); }
  .ps-row {
    display: flex; width: 100%; align-items: center; gap: 14px; min-height: 64px; padding: 10px 14px;
    border: 0; background: transparent; color: inherit; font: inherit; text-align: left; cursor: pointer;
    -webkit-tap-highlight-color: transparent;
  }
  .ps-row:active { background: var(--fill, rgba(120, 120, 128, 0.12)); }
  .ps-ic {
    display: flex; flex-shrink: 0; width: 42px; height: 42px; align-items: center; justify-content: center; border-radius: 13px;
    background: var(--accent-soft, rgba(0, 0, 0, 0.06)); color: var(--accent-ink, var(--accent, #333));
  }
  .ps-txt { display: flex; min-width: 0; flex-direction: column; gap: 2px; }
  .ps-main { font-size: 16.5px; font-weight: 550; }
  .ps-sub { font-size: 13px; color: var(--muted, #888); }
  .ps-sep { height: 1px; margin-left: 70px; background: var(--sep, var(--line, #e5e5e5)); }
  .ps-cancel {
    display: block; width: 100%; min-height: 52px; margin-top: 10px; border: 0; border-radius: 16px;
    background: var(--card, var(--surface, #fff)); color: var(--accent-ink, var(--accent, #333)); text-align: center;
    font: inherit; font-size: 16.5px; font-weight: 600; cursor: pointer; -webkit-tap-highlight-color: transparent;
  }
  .ps-cancel:active { background: var(--fill, rgba(120, 120, 128, 0.12)); }
  @keyframes ps-fade { from { opacity: 0; } }
  @keyframes ps-rise { from { transform: translateY(40px); opacity: 0; } }
</style>
