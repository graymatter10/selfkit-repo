<script lang="ts">
  // Daily note on Today / past days: an "Add a note" row, or text + thumbnails.
  import type { DayNote } from "../lib/api";
  import { app } from "../lib/state.svelte";
  import Icon from "./Icon.svelte";
  import Photo from "./Photo.svelte";
  let { day, note, phone, isToday }: { day: string; note: DayNote | null; phone: boolean; isToday: boolean } = $props();
  const ts = $derived(phone ? 64 : 72);
  const edit = () => (app.noteDay = day);
</script>

{#if !note}
  <div class="card" style="padding:0 {phone ? 18 : 22}px">
    <button type="button" class="row" style="min-height:56px;color:var(--muted)" onclick={edit}>
      <span style="display:flex"><Icon name="pen" /></span>
      <span style="flex:1;font-size:15px">{isToday ? "Add a note about today" : "Add a note about this day"}</span>
      <span style="display:flex;color:var(--faint)"><Icon name="image" /></span>
    </button>
  </div>
{:else}
  <div class="card">
    <div class="ch" style="margin-bottom:10px"><h2>Note</h2><button type="button" class="lnk" onclick={edit}>Edit</button></div>
    {#if note.text}
      <button type="button" class="note-text" style="display:block;width:100%;{phone ? 'display:-webkit-box;-webkit-line-clamp:3;-webkit-box-orient:vertical;overflow:hidden' : ''}" onclick={edit}>{note.text}</button>
    {/if}
    {#if note.photos.length}
      <div style="display:flex;gap:8px;margin-top:{note.text ? 14 : 0}px;flex-wrap:wrap">
        {#each note.photos as id (id)}
          <button type="button" class="thumb" style="width:{ts}px;height:{ts}px" aria-label="Open photo" onclick={() => (app.photo = id)}><Photo {id} /></button>
        {/each}
      </div>
    {/if}
  </div>
{/if}
