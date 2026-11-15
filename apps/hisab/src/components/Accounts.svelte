<script lang="ts">
  // Accounts as cards, net worth with its trend, transfer and new account.
  import Trend from "../charts/Trend.svelte";
  import { api, type Trends } from "../lib/api";
  import { money, monthIso, monthName } from "../lib/fmt";
  import { ACCOUNT_ICON } from "../lib/icons";
  import { app, go, main, openHistory, openSheet } from "../lib/state.svelte";
  import CatBadge from "./CatBadge.svelte";
  import Ic from "./Ic.svelte";
  import StaleOffer from "./StaleOffer.svelte";
  let { desktop = false }: { desktop?: boolean } = $props();
  let tr = $state<Trends | null>(null);
  $effect(() => {
    void app.rev;
    api.trends(monthIso()).then((t) => (tr = t));
  });
  const list = $derived(app.snap?.accounts.filter((a) => !a.archived) ?? []);
  const archived = $derived(app.snap?.accounts.filter((a) => a.archived) ?? []);
  const lent = $derived((app.snap?.loans ?? []).filter((l) => l.direction === "lent").reduce((a, l) => a + l.remaining_main, 0));
  const COLORS: Record<string, string> = { cash: "#5a9e5f", bank: "#3b82c4", wallet: "#e0719c", card: "#a05ac4", other: "#8c6a4f" };
  function transfer(from?: string) {
    app.draft.kind = "transfer";
    app.draft.category = "";
    app.draft.splits = [];
    if (from) app.draft.account = from;
    go("home");
  }
  /** Long balances shrink so "-৳179,840.50" fits a half-width card on a 360 dp phone. */
  const balSize = (t: string) => (t.length > 12 ? "text-[15px]" : t.length > 9 ? "text-[18px]" : "text-[21px]");
</script>

<div class="flex flex-col gap-3">
  <div class="-mb-2.5 empty:hidden"><StaleOffer banner /></div>
  <div class="card px-4 pt-3.5 pb-3">
    <div class="text-[12.5px] font-medium text-muted">Net worth · accounts + lent − borrowed</div>
    <div class="num text-[30px] font-semibold tracking-[-0.8px]">{money(app.snap?.overview.net_worth ?? 0, main())}</div>
    {#if tr && tr.networth.length > 1}<Trend values={tr.networth.map((n) => n.value)} labels={tr.networth.map((n) => monthName(n.month, true))} h={60} />{/if}
  </div>
  <div class="grid gap-2.5 {desktop ? 'grid-cols-3' : 'grid-cols-2'}">
    {#each list as a (a.id)}
      <div class="card relative flex min-h-[124px] flex-col justify-between gap-3 p-4">
        <button type="button" class="press absolute inset-0 rounded-[18px]" aria-label="Show {a.name} entries" onclick={() => openHistory({ account: a.id, period: "all" })}></button>
        <div class="pointer-events-none flex items-center gap-2.5">
          <CatBadge icon={ACCOUNT_ICON[a.kind] ?? "wallet"} color={COLORS[a.kind] ?? "#8a8a8e"} size={34} />
          <div class="min-w-0 flex-1 pr-6"><div class="truncate text-[15px] font-semibold">{a.name}</div><div class="text-[12.5px] text-muted capitalize">{a.kind === "wallet" ? "Mobile wallet" : a.kind}</div></div>
        </div>
        <button type="button" aria-label="Edit {a.name}" class="press absolute top-3 right-2 flex h-8 w-8 items-center justify-center rounded-full text-muted" onclick={() => openSheet({ kind: "account", id: a.id })}><Ic name="more" size={18} stroke={2.4} /></button>
        <div class="pointer-events-none">
          <div class="num truncate {balSize(money(a.balance, a.currency))} font-semibold tracking-[-0.5px] {a.balance < 0 ? 'text-danger' : ''}">{money(a.balance, a.currency)}</div>
          <div class="text-[12.5px] text-muted">{a.currency !== main() ? `≈ ${money(a.balance_main, main())}` : a.currency}</div>
        </div>
      </div>
    {/each}
  </div>
  {#if lent > 0}<p class="m-0 px-1 text-[13px] text-muted">Plus {money(lent, main())} lent out (see Loans).</p>{/if}
  <div class="flex gap-2.5">
    <button type="button" class="press card flex h-12 flex-1 items-center justify-center gap-1.5 text-[15px] font-semibold text-accent-ink" onclick={() => transfer()}><Ic name="arrow" size={18} stroke={2.2} />Transfer</button>
    <button type="button" class="press card flex h-12 flex-1 items-center justify-center gap-1.5 text-[15px] font-semibold text-accent-ink" onclick={() => openSheet({ kind: "account" })}><Ic name="plus" size={18} stroke={2.2} />Account</button>
  </div>
  {#if archived.length}
    <p class="m-0 px-1 pt-2 text-[13px] font-semibold text-muted">Archived</p>
    <div class="card overflow-hidden">
      {#each archived as a (a.id)}<button type="button" class="hl flex h-12 w-full items-center justify-between border-b border-sep px-4 text-left text-[15px] text-muted last:border-0" onclick={() => openSheet({ kind: "account", id: a.id })}><span>{a.name}</span><span class="num">{money(a.balance, a.currency)}</span></button>{/each}
    </div>
  {/if}
</div>
