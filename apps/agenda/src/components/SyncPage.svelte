<script lang="ts">
  // Settings → Sync and its conflict screen (shared ui/src/sync), as a layer above
  // Settings: full screen on the phone, a large dialog on the desktop.
  import { app } from "../lib/state.svelte";
  import SyncSettings from "../../../../ui/src/sync/SyncSettings.svelte";
  import Conflicts from "../../../../ui/src/sync/Conflicts.svelte";
  let { desktop = false }: { desktop?: boolean } = $props();
  const back = () => (app.conflictsOpen ? (app.conflictsOpen = false) : (app.syncOpen = false));
  // Android Back: conflicts → sync → settings.
  $effect(() => {
    if (!app.syncOpen) return;
    const prev = app.closeGuard;
    app.closeGuard = () => (back(), true);
    return () => (app.closeGuard = prev);
  });
</script>

{#if app.syncOpen}
  <div class="fixed inset-0 z-[60] flex {desktop ? 'items-center justify-center' : ''}">
    {#if desktop}<button type="button" aria-label="Close sync" class="fade absolute inset-0 cursor-default bg-scrim" onclick={() => (app.syncOpen = false)}></button>{/if}
    <div role="dialog" aria-modal="true" aria-label="Sync" class="rise relative flex w-full flex-col overflow-hidden bg-bg {desktop ? 'h-[min(760px,92dvh)] max-w-[1100px] rounded-[14px]' : 'h-full pt-[env(safe-area-inset-top)]'}">
      <div class="grid grid-cols-[1fr_auto_1fr] items-center px-4 py-3">
        <button type="button" class="press justify-self-start text-[17px] text-accent" onclick={back}>‹ {app.conflictsOpen ? "Sync" : "Settings"}</button>
        <span class="text-[17px] font-semibold">{app.conflictsOpen ? "Changed on two devices" : "Sync"}</span>
        <span></span>
      </div>
      {#if app.conflictsOpen}
        <div class="flex min-h-0 flex-1 flex-col pb-[env(safe-area-inset-bottom)]"><Conflicts phone={!desktop} onclose={() => (app.conflictsOpen = false)} names={{ tasks: "Agenda task", completions: "Completed task", focus: "Focus session", lists: "List", photos: "Photo" }} /></div>
      {:else}
        <div class="min-h-0 flex-1 overflow-y-auto px-4 pt-2 pb-[max(28px,env(safe-area-inset-bottom))]"><SyncSettings phone={!desktop} onconflicts={() => (app.conflictsOpen = true)} /></div>
      {/if}
    </div>
  </div>
{/if}
