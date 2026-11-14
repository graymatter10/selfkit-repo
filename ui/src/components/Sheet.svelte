<script lang="ts">
  // Bottom sheet with scrim (phone). Esc or scrim tap closes.
  import type { Snippet } from "svelte";
  let { open, label, onclose, children }: { open: boolean; label: string; onclose: () => void; children: Snippet } = $props();
  function key(e: KeyboardEvent) {
    if (open && e.key === "Escape") onclose();
  }
</script>

<svelte:window onkeydown={key} />
{#if open}
  <div class="fixed inset-0 z-40">
    <button type="button" aria-label="Close" class="sk-fade absolute inset-0 cursor-default bg-scrim" onclick={onclose}></button>
    <div
      role="dialog"
      aria-modal="true"
      aria-label={label}
      class="sk-slide absolute inset-x-0 top-24 bottom-0 flex flex-col rounded-t-sheet border-t border-line bg-surface">
      <div class="flex justify-center pt-2 pb-0.5"><span class="h-1 w-10 rounded-full bg-line"></span></div>
      {@render children()}
    </div>
  </div>
{/if}

<style>
  .sk-fade { animation: sk-fade 160ms ease-out; }
  .sk-slide { animation: sk-slide 180ms cubic-bezier(0.2, 0.8, 0.2, 1); }
  @keyframes sk-fade { from { opacity: 0; } }
  @keyframes sk-slide { from { transform: translateY(40px); opacity: 0.6; } }
</style>
