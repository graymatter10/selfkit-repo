<script lang="ts">
  // Opens here: summary on top, the last entries, and the fixed input panel.
  import type { Tx } from "../lib/api";
  import { app, go, openSheet } from "../lib/state.svelte";
  import AddPanel from "./AddPanel.svelte";
  import Ic from "./Ic.svelte";
  import StaleOffer from "./StaleOffer.svelte";
  import Summary from "./Summary.svelte";
  import TxRow from "./TxRow.svelte";
  let { desktop = false, panel = $bindable() }: { desktop?: boolean; panel?: AddPanel } = $props();
  const open = (t: Tx) => openSheet({ kind: "tx", id: t.id });
  const recent = $derived((app.snap?.recent ?? []).slice(0, desktop ? 6 : 3));
</script>

{#snippet recentList()}
  <div class="flex items-center justify-between px-1 pt-1 pb-1.5">
    <span class="text-[13px] font-semibold text-muted">Recent</span>
    <button type="button" class="press flex items-center text-[13px] font-medium text-accent" onclick={() => go("history")}>All<Ic name="chevron" size={14} stroke={2.2} /></button>
  </div>
  {#if recent.length}
    <div class="card overflow-hidden">
      {#each recent as t, i (t.id)}<TxRow {t} onopen={open} last={i === recent.length - 1} />{/each}
    </div>
  {:else}
    <p class="m-0 px-1 py-3 text-[14px] text-muted">Nothing yet. Type an amount below, pick a category, Save.</p>
  {/if}
{/snippet}

{#if desktop}
  <div class="flex h-full gap-6 overflow-hidden px-8 pt-6 pb-6">
    <div class="flex min-w-0 flex-1 flex-col gap-4 overflow-y-auto">
      <div class="empty:hidden"><StaleOffer banner /></div>
      <Summary big />
      <div>{@render recentList()}</div>
    </div>
    <div class="card w-[400px] shrink-0 self-start px-4 pb-4"><AddPanel bind:this={panel} /></div>
  </div>
{:else}
  <div class="flex h-full flex-col overflow-hidden">
    <div class="min-h-0 flex-1 overflow-y-auto px-4 pt-1 pb-3">
      <StaleOffer banner />
      <Summary compact />
      <div class="mt-1.5">{@render recentList()}</div>
    </div>
    <div class="shrink-0 rounded-t-[22px] bg-card px-3.5 pb-[max(10px,env(safe-area-inset-bottom))] shadow-[0_-6px_24px_rgba(0,0,0,0.06)]">
      <AddPanel bind:this={panel} />
    </div>
  </div>
{/if}
