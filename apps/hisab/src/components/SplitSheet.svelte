<script lang="ts">
  // Split the draft amount across several categories.
  import { app, closeSheet, rankedCats } from "../lib/state.svelte";
  import CatBadge from "./CatBadge.svelte";
  import Ic from "./Ic.svelte";
  import SheetFrame from "./SheetFrame.svelte";
  let { desktop = false }: { desktop?: boolean } = $props();
  const total = parseFloat(app.draft.amount || "0") || 0;
  const cats = rankedCats("expense");
  let parts = $state<{ category: string; amount: string }[]>(
    app.draft.splits.length > 1 ? app.draft.splits.map((s) => ({ ...s })) : [{ category: app.draft.category || cats[0]?.id || "", amount: total ? String(total) : "" }, { category: "", amount: "" }],
  );
  const sum = $derived(parts.reduce((a, p) => a + (parseFloat(p.amount) || 0), 0));
  const left = $derived(Math.round((total - sum) * 100) / 100);
  const valid = $derived(total > 0 && parts.filter((p) => p.category && parseFloat(p.amount) > 0).length >= 2 && Math.abs(left) < 0.005);
  function fill(i: number) {
    const v = (parseFloat(parts[i].amount) || 0) + left;
    parts[i].amount = String(Math.round(v * 100) / 100);
  }
  function done() {
    app.draft.splits = parts.filter((p) => p.category && parseFloat(p.amount) > 0);
    app.draft.category = app.draft.splits[0]?.category ?? "";
    closeSheet();
  }
  function unsplit() {
    app.draft.splits = [];
    closeSheet();
  }
</script>

<SheetFrame title="Split" {desktop} left="Cancel" right="Done" onleft={closeSheet} onright={done} rightDisabled={!valid}>
  {#if total <= 0}<p class="pt-6 text-center text-[15px] text-muted">Type the total amount first.</p>{:else}
    <p class="m-0 px-1 pb-3 text-[14px] text-muted">Total <span class="num font-semibold text-ink">{total.toFixed(2)}</span> · {Math.abs(left) < 0.005 ? "all assigned" : left > 0 ? `${left.toFixed(2)} left to assign` : `${(-left).toFixed(2)} too much`}</p>
    <div class="flex flex-col gap-2.5">
      {#each parts as p, i}
        <div class="card flex flex-col gap-2 p-3">
          <div class="flex gap-1 overflow-x-auto fade-r">
            {#each cats as c (c.id)}
              <button type="button" aria-pressed={p.category === c.id} class="press flex shrink-0 items-center gap-1.5 rounded-full py-1 pr-3 pl-1 text-[13px] {p.category === c.id ? 'bg-accent-soft font-semibold' : 'bg-fill2'}" onclick={() => (p.category = c.id)}>
                <CatBadge icon={c.icon} color={c.color} size={24} />{c.name}</button>
            {/each}
          </div>
          <div class="flex items-center gap-2">
            <input inputmode="decimal" bind:value={p.amount} placeholder="0.00" aria-label="Amount for part {i + 1}" class="field num flex-1 text-[17px] font-semibold" />
            {#if Math.abs(left) >= 0.005}<button type="button" class="press h-11 rounded-[12px] bg-fill px-3 text-[13px] font-medium" onclick={() => fill(i)}>Rest</button>{/if}
            {#if parts.length > 2}<button type="button" aria-label="Remove part" class="press flex h-11 w-11 items-center justify-center rounded-[12px] bg-fill text-muted" onclick={() => (parts = parts.filter((_, j) => j !== i))}><Ic name="close" size={16} /></button>{/if}
          </div>
        </div>
      {/each}
      <button type="button" class="press flex h-11 items-center justify-center gap-1 rounded-[12px] text-[15px] font-medium text-accent" onclick={() => (parts = [...parts, { category: "", amount: left > 0 ? String(left) : "" }])}><Ic name="plus" size={17} />Add part</button>
      {#if app.draft.splits.length > 1}<button type="button" class="press h-11 text-[15px] text-danger" onclick={unsplit}>Don't split</button>{/if}
    </div>
  {/if}
</SheetFrame>
