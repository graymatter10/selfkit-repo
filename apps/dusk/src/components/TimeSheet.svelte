<script lang="ts">
  // Holds the TimeDial: a bottom sheet on phones, a small dialog on desktop,
  // with Cancel / Done. Android Back and Esc cancel (it takes over
  // app.closeGuard while open and gives it back after), Enter is Done.
  import { onMount } from "svelte";
  import { app } from "../lib/state.svelte";
  import { tdisp, uses12h } from "../lib/fmt";
  import TimeDial from "./TimeDial.svelte";

  let { title, value, hint = "", desktop, ondone, oncancel }: {
    title: string; value: string; hint?: string; desktop: boolean; ondone: (t: string) => void; oncancel: () => void;
  } = $props();
  // svelte-ignore state_referenced_locally
  let t = $state(value);
  let dial: TimeDial | undefined = $state();
  const h12 = uses12h();

  onMount(() => {
    const prev = app.closeGuard;
    const opener = document.activeElement as HTMLElement | null;
    const mine = () => (oncancel(), true);
    app.closeGuard = mine;
    if (desktop) dial?.focus();
    return () => {
      if (app.closeGuard === mine) app.closeGuard = prev;
      if (desktop) opener?.focus?.();
    };
  });
  function keys(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      oncancel();
    } else if (e.key === "Enter") {
      e.stopPropagation();
      if ((e.target as HTMLElement).dataset.act === "cancel") return;
      e.preventDefault();
      ondone(t);
    } else if (e.key === "Tab") {
      // keep focus inside the sheet
      const f = [...(e.currentTarget as HTMLElement).querySelectorAll<HTMLElement>("button")];
      const i = f.indexOf(document.activeElement as HTMLElement);
      if (e.shiftKey && i <= 0) (e.preventDefault(), f[f.length - 1].focus());
      else if (!e.shiftKey && i === f.length - 1) (e.preventDefault(), f[0].focus());
    }
  }
</script>

<div class="fixed inset-0 z-[65] flex {desktop ? 'items-center justify-center' : 'items-end'}">
  <button type="button" tabindex="-1" aria-label="Cancel" class="fade absolute inset-0 bg-scrim" onclick={oncancel}></button>
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div role="dialog" aria-modal="true" aria-label={title} tabindex="-1" onkeydown={keys}
    class="relative flex flex-col bg-card {desktop ? 'pop w-[340px] rounded-[22px] px-5 pt-4 pb-4 shadow-[0_12px_48px_rgba(10,8,20,0.28)]' : 'rise w-full rounded-t-[26px] px-5 pt-2.5 pb-[max(16px,env(safe-area-inset-bottom))]'}">
    {#if !desktop}<div class="mx-auto mb-3 h-1 w-9 rounded-full bg-faint"></div>{/if}
    <div class="mb-4 flex items-baseline">
      <h3 class="m-0 text-[16px] font-[650] tracking-[-0.015em]">{title}</h3>
      <span class="flex-1"></span>
      {#if hint}<span class="num text-[13px] text-muted">{hint}</span>{/if}
    </div>
    <TimeDial bind:this={dial} bind:value={t} {h12} />
    <div class="mt-5 flex gap-2.5">
      <button type="button" data-act="cancel" class="press h-[50px] flex-1 rounded-[14px] bg-fill text-[16px] font-semibold text-ink2" onclick={oncancel}>Cancel</button>
      <button type="button" class="press h-[50px] flex-1 rounded-[14px] bg-accent text-[16px] font-semibold text-on-accent" aria-label="Done, {tdisp(t)}" onclick={() => ondone(t)}>Done</button>
    </div>
    {#if desktop}<p class="m-0 mt-3 text-center text-[11.5px] text-muted">Drag or click · <b class="font-semibold text-ink2">← →</b> change · <b class="font-semibold text-ink2">Tab</b> hour/minutes · <b class="font-semibold text-ink2">Enter</b> done · <b class="font-semibold text-ink2">Esc</b> cancel</p>{/if}
  </div>
</div>

<style>
  .pop { animation: pop 170ms cubic-bezier(0.2, 0.9, 0.25, 1); }
  @keyframes pop { from { transform: scale(0.96); opacity: 0; } }
  @media (prefers-reduced-motion: reduce) { .pop { animation: none; } }
</style>
