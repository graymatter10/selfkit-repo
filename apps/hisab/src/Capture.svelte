<script lang="ts">
  // Desktop quick-capture popup ("add expense" from anywhere): the same
  // input panel, compact. Enter saves and closes; Esc closes (draft kept).
  import AddPanel from "./components/AddPanel.svelte";
  import Toast from "./components/Toast.svelte";
  import CurrencySheet from "./components/CurrencySheet.svelte";
  import SplitSheet from "./components/SplitSheet.svelte";
  import CategorySheet from "./components/CategorySheet.svelte";
  import { api } from "./lib/api";
  import { app, closeSheet, init } from "./lib/state.svelte";
  init();
  let panel: AddPanel | undefined = $state();
  const close = () => api.closeCapture();
  function keys(e: KeyboardEvent) {
    const typing = ["INPUT", "TEXTAREA", "SELECT"].includes((e.target as HTMLElement)?.tagName);
    if (e.key === "Escape") return void (app.sheets.length ? closeSheet() : close());
    if (typing || app.sheets.length) return;
    if (/^[0-9]$/.test(e.key) || e.key === ".") panel?.key(e.key);
    else if (e.key === "Backspace") panel?.key("back");
    else if (e.key === "Enter") panel?.save();
  }
  window.addEventListener("blur", () => !app.sheets.length && setTimeout(() => !document.hasFocus() && close(), 200));
</script>

<svelte:window onkeydown={keys} />
{#if app.ready}
  <div class="flex h-screen flex-col gap-2 overflow-hidden bg-card px-4 pt-3 pb-4">
    <div class="flex items-center justify-between"><b class="text-[15px]">Add expense</b><span class="text-[12px] text-muted">Type the amount · Enter saves · Esc closes</span></div>
    <AddPanel bind:this={panel} compact onsaved={() => setTimeout(close, 500)} />
  </div>
{/if}
{#each app.sheets as s, i (i)}
  {#if s.kind === "currency"}<CurrencySheet value={s.value} onpick={s.onpick} />
  {:else if s.kind === "split"}<SplitSheet />
  {:else if s.kind === "category"}<CategorySheet type={s.type} pick={s.pick} />{/if}
{/each}
<div class="pointer-events-none fixed inset-x-3 top-3 z-[90]"><Toast /></div>
