<script lang="ts">
  import { daysBetween, money, todayIso } from "../lib/fmt";
  import { app, openSheet } from "../lib/state.svelte";
  import Ic from "./Ic.svelte";
  let { desktop = false }: { desktop?: boolean } = $props();
  const goals = $derived(app.snap?.goals.filter((g) => !g.archived) ?? []);
</script>

<div class="flex flex-col gap-3">
  <div class="grid gap-3 {desktop ? 'grid-cols-2' : ''}">
    {#each goals as g (g.id)}
      {@const pct = Math.min(1, g.saved / g.target)}
      <button type="button" class="press card block p-4 text-left" onclick={() => openSheet({ kind: "goal", id: g.id })}>
        <div class="flex items-baseline justify-between"><span class="truncate text-[15px] font-semibold">{g.name}</span><span class="text-[12.5px] text-muted">{Math.round(pct * 100)}%</span></div>
        <div class="num mt-2 text-[20px] font-semibold">{money(g.saved, g.currency)}<span class="text-[13px] font-medium text-muted">&nbsp;of {money(g.target, g.currency)}</span></div>
        <div class="mt-2 h-[7px] overflow-hidden rounded-full bg-fill"><div class="h-full rounded-full bg-accent" style="width:{pct * 100}%"></div></div>
        <div class="mt-2 text-[12.5px] text-muted">{pct >= 1 ? "Reached" : `${money(g.target - g.saved, g.currency)} to go`}{g.deadline && pct < 1 ? (() => { const d = daysBetween(todayIso(), g.deadline); const months = Math.max(1, d / 30.4); return ` · ${d} days left · ${money((g.target - g.saved) / months, g.currency)}/month`; })() : ""}</div>
      </button>
    {:else}
      <p class="m-0 py-8 text-center text-[15px] text-muted">No savings goals yet.</p>
    {/each}
  </div>
  <button type="button" class="press flex h-[52px] items-center justify-center gap-1.5 rounded-[14px] bg-accent text-[17px] font-semibold text-on-accent {desktop ? 'w-60' : ''}" onclick={() => openSheet({ kind: "goal" })}><Ic name="plus" size={20} stroke={2.2} />New goal</button>
</div>
