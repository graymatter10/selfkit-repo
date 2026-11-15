<script lang="ts">
  // Note editor: full screen on phone, a dialog on desktop. Text (≤ 1000
  // characters) plus photos scaled to document quality (~2400 px WebP).
  import { showToast } from "@selfkit/ui";
  import { api } from "../lib/api";
  import { longDay } from "../lib/fmt";
  import { attachPhoto } from "../lib/photos";
  import { askPhotoSource, cleanCaptures } from "../lib/photo-source";
  import { app, bump, guard } from "../lib/state.svelte";
  import Icon from "./Icon.svelte";
  import Photo from "./Photo.svelte";
  let { day, phone }: { day: string; phone: boolean } = $props();
  let text = $state(""), photos = $state<string[]>([]), orig = { text: "", photos: [] as string[] };
  let loaded = $state(false), busy = $state(false), hadNote = $state(false), err = $state("");
  let ta: HTMLTextAreaElement | undefined = $state();
  let file: HTMLInputElement | undefined = $state();
  let camera: HTMLInputElement | undefined = $state();

  $effect(() => {
    api.note(day).then((n) => {
      text = n?.text ?? "";
      photos = n?.photos ?? [];
      orig = { text, photos: [...photos] };
      hadNote = !!n;
      loaded = true;
      setTimeout(() => ta?.focus(), 50);
    });
  });
  const dirty = () => text.trim() !== orig.text.trim() || photos.join() !== orig.photos.join();
  const close = () => (app.noteDay = null);

  // Back / Escape with unsaved changes: save them (nothing is lost, no question asked).
  $effect(() => {
    const prev = app.closeGuard;
    app.closeGuard = () => { if (dirty()) save(); else close(); return true; };
    return () => (app.closeGuard = prev);
  });

  async function save() {
    if (busy) return;
    const changed = dirty();
    busy = true;
    const r = await guard(api.saveNote(day, text, $state.snapshot(photos)));
    busy = false;
    close();
    bump();
    if (changed) showToast(r ? "Note saved" : "Note removed");
  }
  async function remove() {
    const before = { text: orig.text, photos: [...orig.photos] };
    await guard(api.saveNote(day, "", []));
    close();
    bump();
    showToast("Note deleted", async () => { await api.saveNote(day, before.text, before.photos); bump(); });
  }
  async function picked(e: Event) {
    const files = [...((e.target as HTMLInputElement).files ?? [])];
    (e.target as HTMLInputElement).value = "";
    err = "";
    for (const f of files) {
      try {
        busy = true;
        photos = [...photos, await attachPhoto(f)];
      } catch (x) {
        err = String(x).replace(/^Error: /, "");
      } finally {
        busy = false;
      }
    }
    cleanCaptures(); // drop the temporary camera file (photo is stored by now)
  }
  function keys(e: KeyboardEvent) {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) { e.preventDefault(); save(); }
    // Escape keeps what was typed (like Android Back); Cancel discards.
    if (e.key === "Escape" && !app.photo) { e.preventDefault(); e.stopPropagation(); dirty() ? save() : close(); }
  }
  const ts = $derived(phone ? 96 : 104);
</script>

{#snippet body()}
  <div class="ta" style="min-height:{phone ? 210 : 170}px;display:flex">
    <textarea bind:this={ta} bind:value={text} maxlength="1000" class="note-text" placeholder="What happened today? What do you want to remember?" style="min-height:{phone ? 180 : 140}px" aria-label="Note"></textarea>
  </div>
  <div class="muted num" style="font-size:12.5px;text-align:right;margin-top:6px">{text.length} / 1000</div>
  <div class="sec" style="margin:10px 0 10px;font-weight:600;color:var(--ink)">Photos</div>
  <div style="display:flex;gap:10px;flex-wrap:wrap">
    {#each photos as id (id)}
      <span class="thumb" style="width:{ts}px;height:{ts}px">
        <button type="button" style="width:100%;height:100%;display:block" aria-label="Open photo" onclick={() => (app.photo = id)}><Photo {id} /></button>
        <button type="button" aria-label="Remove photo" onclick={() => (photos = photos.filter((p) => p !== id))}
          style="position:absolute;top:4px;right:4px;width:32px;height:32px;border-radius:50%;background:rgba(0,0,0,.55);color:#fff;display:grid;place-items:center"><Icon name="close" size={14} stroke={2.4} /></button>
      </span>
    {/each}
    <button type="button" disabled={busy} onclick={() => askPhotoSource({ gallery: () => file?.click(), camera: () => camera?.click() })}
      style="width:{ts}px;height:{ts}px;border-radius:10px;box-shadow:inset 0 0 0 1.5px var(--line);display:flex;flex-direction:column;align-items:center;justify-content:center;gap:6px;color:var(--accent-ink);font-size:13.5px;font-weight:600">
      <Icon name={phone ? "camera" : "plus"} size={22} />{busy ? "Adding…" : "Add photo"}</button>
    <input bind:this={file} type="file" accept="image/*" multiple class="sr" onchange={picked} />
    <input bind:this={camera} type="file" accept="image/*" capture="environment" class="sr" tabindex="-1" aria-hidden="true" onchange={picked} oncancel={cleanCaptures} />
  </div>
  {#if err}<p role="alert" style="color:var(--danger);font-size:13.5px;margin:10px 0 0">{err}</p>{/if}
  <div style="display:flex;gap:10px;margin-top:14px"><span class="muted" style="display:flex;margin-top:1px"><Icon name="lock" size={16} /></span><span class="hint">Photos keep their full quality and are stored on this device. {phone ? "Tap" : "Click"} a photo to see it full size.</span></div>
{/snippet}

<svelte:window onkeydown={keys} />
{#if loaded}
  {#if phone}
    <div class="page fade" style="z-index:70">
      <div class="appbar">
        <button type="button" class="ib" aria-label="Close" onclick={close}><Icon name="close" size={22} /></button>
        <div style="flex:1"><div class="t" style="font-size:17px">Note</div><div class="muted" style="font-size:13px">{longDay(day)}</div></div>
        <button type="button" class="btn pri" style="height:40px;margin-right:8px" disabled={busy} onclick={save}>Save</button>
      </div>
      <div class="content" style="padding:14px 16px 0;flex:1;overflow-y:auto">{@render body()}</div>
      {#if hadNote}<div style="padding:0 16px 26px;display:flex;justify-content:center"><button type="button" class="btn ghost danger" onclick={remove}>Delete note</button></div>{/if}
    </div>
  {:else}
    <div class="scrim fade" role="presentation" onclick={(e) => e.target === e.currentTarget && (dirty() ? save() : close())}>
      <div class="dialog rise" role="dialog" aria-modal="true" aria-label="Note">
        <div style="display:flex;align-items:flex-start;margin-bottom:16px">
          <div style="flex:1"><div style="font-size:18px;font-weight:650;letter-spacing:-0.01em">Note</div><div class="muted" style="font-size:13.5px;margin-top:2px">{longDay(day)}</div></div>
          <button type="button" class="ib" aria-label="Close" onclick={close}><Icon name="close" size={20} /></button>
        </div>
        {@render body()}
        <div style="display:flex;align-items:center;gap:10px;margin-top:20px;padding-top:16px;border-top:1px solid var(--line2)">
          {#if hadNote}<button type="button" class="btn ghost danger" style="padding:0 8px" onclick={remove}>Delete note</button>{/if}
          <span style="flex:1"></span>
          <span class="muted" style="font-size:12.5px;margin-right:6px">Ctrl+Enter to save</span>
          <button type="button" class="btn" onclick={close}>Cancel</button>
          <button type="button" class="btn pri" disabled={busy} onclick={save}>Save</button>
        </div>
      </div>
    </div>
  {/if}
{/if}
