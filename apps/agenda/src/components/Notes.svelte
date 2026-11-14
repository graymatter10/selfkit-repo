<script lang="ts">
  // Task description: Markdown (safe renderer, lib/markdown.ts) plus attached
  // photos. Photos referenced inline (![](photo:id)) show in place; the rest
  // show as a grid below. Links open in the system browser; tap a photo to view it.
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { inlinePhotos, renderMarkdown } from "../lib/markdown";
  import { photoUrl } from "../lib/photos";
  import { app } from "../lib/state.svelte";
  import Photo from "./Photo.svelte";

  let { notes, photos = [] }: { notes?: string; photos?: string[] } = $props();
  const html = $derived(notes ? renderMarkdown(notes) : "");
  const rest = $derived(photos.filter((p) => !inlinePhotos(notes).includes(p)));
  let el: HTMLDivElement | undefined = $state();

  $effect(() => {
    void html;
    el?.querySelectorAll<HTMLImageElement>("img[data-photo]").forEach((img) => {
      const id = img.dataset.photo!;
      photoUrl(id).then((u) => (img.src = u)).catch(() => img.replaceWith(document.createTextNode("[missing photo]")));
    });
  });

  function click(e: MouseEvent) {
    const t = e.target as HTMLElement;
    const a = t.closest("a");
    if (a) {
      e.preventDefault();
      const href = a.getAttribute("href");
      if (href) openUrl(href).catch((err) => console.warn("open link", err));
      return;
    }
    if (t instanceof HTMLImageElement && t.dataset.photo) app.photo = t.dataset.photo;
  }
</script>

{#if html || rest.length}
  <div class="overflow-hidden rounded-xl bg-card">
    {#if html}
      <!-- eslint-disable-next-line svelte/no-at-html-tags -- escaped + allowlist-sanitized in lib/markdown.ts -->
      <div bind:this={el} role="presentation" onclick={click} class="md px-4 py-3 text-[17px] leading-normal">{@html html}</div>
    {/if}
    {#if rest.length}
      <div class="grid grid-cols-3 gap-1 p-1 {html ? 'pt-0' : ''}">
        {#each rest as id (id)}
          <button type="button" aria-label="View photo" class="press aspect-square overflow-hidden rounded-lg bg-fill" onclick={() => (app.photo = id)}><Photo {id} /></button>
        {/each}
      </div>
    {/if}
  </div>
{/if}
