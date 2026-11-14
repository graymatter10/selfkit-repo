<script lang="ts">
  // Sheet chrome: bottom sheet on phones, centred panel on desktop.
  // Header: left action · title · right action.
  import type { Snippet } from "svelte";
  let { title, left = "Cancel", right = "", onleft, onright, rightDisabled = false, children, desktop = false, danger = false }:
    { title: string; left?: string; right?: string; onleft: () => void; onright?: () => void; rightDisabled?: boolean; children: Snippet; desktop?: boolean; danger?: boolean } = $props();
</script>

<div class="fixed inset-0 z-40 flex {desktop ? 'items-start justify-center px-8 pt-[9vh] pb-8' : 'flex-col'}">
  <button type="button" aria-label="Close" tabindex="-1" class="fade absolute inset-0 bg-scrim" onclick={onleft}></button>
  <div role="dialog" aria-label={title}
    class="{desktop ? 'pop max-h-[86vh] w-[520px] rounded-[20px]' : 'rise mt-[max(36px,calc(env(safe-area-inset-top)+20px))] flex-1 rounded-t-[20px]'} relative flex min-h-0 flex-col overflow-hidden bg-bg">
    <div class="flex h-14 shrink-0 items-center gap-2 px-4">
      <button type="button" class="press min-w-16 text-left text-[16px] text-accent" onclick={onleft}>{left}</button>
      <h2 class="m-0 flex-1 truncate text-center text-[16px] font-semibold">{title}</h2>
      <div class="min-w-16 text-right">
        {#if right}<button type="button" disabled={rightDisabled} class="press text-[16px] font-semibold {danger ? 'text-danger' : 'text-accent'} disabled:opacity-35" onclick={onright}>{right}</button>{/if}
      </div>
    </div>
    <div class="min-h-0 flex-1 overflow-y-auto px-4 pb-[max(24px,env(safe-area-inset-bottom))]">{@render children()}</div>
  </div>
</div>
