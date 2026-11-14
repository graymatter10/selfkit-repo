<script lang="ts">
  import { app } from "../lib/state.svelte";
  import ConnCard from "./ConnCard.svelte";
  let { desktop }: { desktop: boolean } = $props();
  const c = $derived(app.conns);
  const span = $derived(c ? (c.lookback_days <= 30 ? "month" : c.lookback_days <= 90 ? "3 months" : "year") : "");
  const intro = $derived(`Patterns across your apps over the last ${span}. A link doesn't prove a cause.`);
  const toggle = (id: string) => (app.open[id] = !app.open[id]);
</script>

{#if !c}<p class="pt-16 text-center text-muted">Loading…</p>
{:else if desktop}
  <div class="max-w-[1000px]"><h1 class="dtitle">Connections</h1><p class="mt-1.5 mb-5 text-[14px] text-muted">{intro}</p>
    <div class="flex flex-col gap-3">{#each c.items as x (x.id)}<ConnCard c={x} sym={c.sym} desk open={!!app.open[x.id]} ontoggle={() => toggle(x.id)} />{/each}</div></div>
{:else}
  <p class="intro">{intro}</p>
  <div class="flex flex-col gap-3">{#each c.items as x (x.id)}<ConnCard c={x} sym={c.sym} open={!!app.open[x.id]} ontoggle={() => toggle(x.id)} />{/each}</div>
{/if}
