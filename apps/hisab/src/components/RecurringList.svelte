<script lang="ts">
  import { money, shortDate } from "../lib/fmt";
  import { app, catById, main, openSheet } from "../lib/state.svelte";
  import CatBadge from "./CatBadge.svelte";
  import Ic from "./Ic.svelte";
  let { desktop = false }: { desktop?: boolean } = $props();
  const all = $derived(app.snap?.recurring ?? []);
  const subs = $derived(all.filter((r) => r.subscription));
  const other = $derived(all.filter((r) => !r.subscription));
  const monthly = $derived(subs.filter((r) => r.active && r.template.kind === "expense").reduce((a, r) => a + r.monthly, 0));
  const FREQ: Record<string, string> = { daily: "day", weekly: "week", monthly: "month", yearly: "year" };
</script>

{#snippet list(items: typeof all)}
  <div class="card overflow-hidden">
    {#each items as r (r.id)}
      {@const c = catById(r.template.category ?? "")}
      <button type="button" class="hl flex min-h-[60px] w-full items-center gap-3 px-3.5 text-left {r.active ? '' : 'opacity-50'}" onclick={() => openSheet({ kind: "recurring", id: r.id })}>
        <CatBadge icon={c?.icon ?? "repeat"} color={c?.color ?? "#8a8a8e"} />
        <span class="flex min-w-0 flex-1 flex-col border-b border-sep py-2.5">
          <span class="flex items-baseline gap-2"><span class="min-w-0 flex-1 truncate text-[15px] font-medium">{r.name}</span><span class="num text-[15px] font-semibold {r.template.kind === 'income' ? 'text-accent-ink' : ''}">{money(r.template.amount ?? 0, r.template.currency ?? main())}</span></span>
          <span class="text-[13px] text-muted">every {r.interval > 1 ? r.interval + " " : ""}{FREQ[r.freq]}{r.interval > 1 ? "s" : ""} · {r.active ? `next ${shortDate(r.next)}` : "stopped"}</span>
        </span>
      </button>
    {/each}
  </div>
{/snippet}

<div class="flex flex-col gap-3 {desktop ? 'max-w-[720px]' : ''}">
  <div class="card px-4 py-3.5"><div class="text-[12.5px] font-medium text-muted">Subscriptions cost about</div><div class="num text-[26px] font-semibold tracking-[-0.6px]">{money(monthly, main())}<span class="text-[14px] font-medium text-muted"> / month · {money(monthly * 12, main())} / year</span></div></div>
  {#if subs.length}<p class="m-0 px-1 text-[13px] font-semibold text-muted">Subscriptions</p>{@render list(subs)}{/if}
  {#if other.length}<p class="m-0 px-1 text-[13px] font-semibold text-muted">Other recurring</p>{@render list(other)}{/if}
  {#if !all.length}<p class="m-0 py-6 text-center text-[15px] text-muted">Rent, salary, Netflix… add them once and they appear on their date.</p>{/if}
  <button type="button" class="press flex h-[52px] items-center justify-center gap-1.5 rounded-[14px] bg-accent text-[17px] font-semibold text-on-accent {desktop ? 'w-60' : ''}" onclick={() => openSheet({ kind: "recurring" })}><Ic name="plus" size={20} stroke={2.2} />New recurring</button>
</div>
