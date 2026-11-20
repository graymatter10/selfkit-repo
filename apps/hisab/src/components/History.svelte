<script lang="ts">
  // All transactions: search + filters (period, type, category, account,
  // tag, amount), grouped by day with day totals.
  import { api, type Filter, type Tx } from "../lib/api";
  import { addDays, addMonths, dayLabel, money, monthIso, todayIso } from "../lib/fmt";
  import { app, main, openSheet } from "../lib/state.svelte";
  import Ic from "./Ic.svelte";
  import TxRow from "./TxRow.svelte";

  let { initial = {} }: { initial?: Partial<Filter> & { period?: string } } = $props();
  let q = $state("");
  let period = $state(initial.period ?? "month");
  let kind = $state(initial.kind ?? "");
  let category = $state(initial.category ?? "");
  let account = $state(initial.account ?? "");
  let tag = $state(initial.tag ?? "");
  let from = $state(""), to = $state("");
  let min = $state(""), max = $state("");
  let more = $state(false);
  let list = $state<Tx[]>([]);
  let loading = $state(true);

  function range(): [string, string] {
    const t = todayIso(), m = monthIso();
    switch (period) {
      case "month": return [m + "-01", ""];
      case "last": { const lm = addMonths(m, -1); return [lm + "-01", addDays(m + "-01", -1)]; }
      case "3m": return [addMonths(m, -2) + "-01", ""];
      case "year": return [t.slice(0, 4) + "-01-01", ""];
      case "custom": return [from, to];
      default: return ["", ""];
    }
  }

  let timer: ReturnType<typeof setTimeout>;
  let seq = 0; // replies can arrive out of order (async commands): keep only the newest (C7)
  $effect(() => {
    const [f, t] = range();
    const flt: Filter = { q, kind, category, account, tag, from: f, to: t, min: min ? Number(min) : undefined, max: max ? Number(max) : undefined };
    void app.rev;
    clearTimeout(timer);
    timer = setTimeout(async () => {
      const n = ++seq;
      const r = await api.transactions(flt);
      if (n !== seq) return;
      list = r;
      loading = false;
    }, q ? 120 : 0);
  });

  const groups = $derived.by(() => {
    const out: { date: string; items: Tx[]; out: number; inc: number }[] = [];
    for (const t of list) {
      let g = out[out.length - 1];
      if (!g || g.date !== t.date) out.push((g = { date: t.date, items: [], out: 0, inc: 0 }));
      g.items.push(t);
      if (t.kind === "expense") g.out += t.amount_main ?? 0;
      if (t.kind === "income") g.inc += t.amount_main ?? 0;
    }
    return out;
  });
  const totals = $derived(groups.reduce((a, g) => ({ out: a.out + g.out, inc: a.inc + g.inc }), { out: 0, inc: 0 }));
  const active = $derived([kind, category, account, tag, min, max].filter(Boolean).length);
  const cats = $derived((app.snap?.categories ?? []).filter((c) => !kind || kind === "transfer" || c.kind === kind));

  function clear() {
    kind = category = account = tag = min = max = "";
  }
  const sel = "h-8 shrink-0 appearance-none rounded-full px-3 text-[13px] font-medium outline-none";
</script>

<div class="flex flex-col gap-2.5">
  <label class="flex h-10 items-center gap-2 rounded-[12px] bg-fill px-3 text-muted">
    <Ic name="search" size={17} stroke={2.1} /><span class="sr">Search</span>
    <input bind:value={q} placeholder="Search notes, tags, categories, amounts" enterkeyhint="search" class="min-w-0 flex-1 bg-transparent text-[15px] text-ink outline-none" />
    {#if q}<button type="button" aria-label="Clear search" class="press" onclick={() => (q = "")}><Ic name="close" size={16} /></button>{/if}
  </label>
  <div class="-mx-4 flex gap-1.5 overflow-x-auto px-4">
    <select bind:value={period} aria-label="Period" class="{sel} bg-accent-soft text-accent-ink">
      <option value="month">This month</option><option value="last">Last month</option><option value="3m">Last 3 months</option>
      <option value="year">This year</option><option value="all">All time</option><option value="custom">Custom dates</option>
    </select>
    <select bind:value={kind} aria-label="Type" class="{sel} {kind ? 'bg-accent-soft text-accent-ink' : 'bg-fill text-ink2'}">
      <option value="">All types</option><option value="expense">Expenses</option><option value="income">Income</option><option value="transfer">Transfers</option>
    </select>
    <select bind:value={category} aria-label="Category" class="{sel} {category ? 'bg-accent-soft text-accent-ink' : 'bg-fill text-ink2'}">
      <option value="">All categories</option>
      {#each cats as c (c.id)}<option value={c.id}>{c.name}{c.kind === "income" ? " (income)" : ""}{c.archived ? " · archived" : ""}</option>{/each}
    </select>
    <select bind:value={account} aria-label="Account" class="{sel} {account ? 'bg-accent-soft text-accent-ink' : 'bg-fill text-ink2'}">
      <option value="">All accounts</option>
      {#each app.snap?.accounts ?? [] as a (a.id)}<option value={a.id}>{a.name}</option>{/each}
    </select>
    {#if app.snap?.tags.length}
      <select bind:value={tag} aria-label="Tag" class="{sel} {tag ? 'bg-accent-soft text-accent-ink' : 'bg-fill text-ink2'}">
        <option value="">All tags</option>
        {#each app.snap.tags as t}<option value={t}>#{t}</option>{/each}
      </select>
    {/if}
    <button type="button" class="{sel} {min || max ? 'bg-accent-soft text-accent-ink' : 'bg-fill text-ink2'}" onclick={() => (more = !more)}>Amount</button>
    {#if active}<button type="button" class="{sel} text-accent" onclick={clear}>Clear</button>{/if}
  </div>
  {#if period === "custom" || more}
    <div class="fade flex flex-wrap gap-2">
      {#if period === "custom"}
        <label class="flex flex-1 items-center gap-2 text-[13px] text-muted">From<input type="date" bind:value={from} class="field h-9 flex-1 text-[14px] text-ink" /></label>
        <label class="flex flex-1 items-center gap-2 text-[13px] text-muted">To<input type="date" bind:value={to} class="field h-9 flex-1 text-[14px] text-ink" /></label>
      {/if}
      {#if more}
        <label class="flex flex-1 items-center gap-2 text-[13px] text-muted">Min<input inputmode="decimal" bind:value={min} class="field h-9 flex-1 text-[14px] text-ink" /></label>
        <label class="flex flex-1 items-center gap-2 text-[13px] text-muted">Max<input inputmode="decimal" bind:value={max} class="field h-9 flex-1 text-[14px] text-ink" /></label>
      {/if}
    </div>
  {/if}

  {#if list.length}
    <p class="m-0 px-1 text-[13px] text-muted">{list.length} {list.length === 1 ? "entry" : "entries"} · spent <span class="num font-medium text-ink2">{money(totals.out, main())}</span> · income <span class="num font-medium text-ink2">{money(totals.inc, main())}</span></p>
  {/if}
  {#each groups as g (g.date)}
    <section>
      <div class="flex items-baseline justify-between px-1 pt-2 pb-1.5 text-[13px]">
        <span class="font-semibold text-ink2">{dayLabel(g.date)}</span>
        <span class="num text-muted">{g.out ? "−" + money(g.out, main()) : ""}{g.inc ? `  +${money(g.inc, main())}` : ""}</span>
      </div>
      <div class="card overflow-hidden">
        {#each g.items as t, i (t.id)}<TxRow {t} onopen={(t) => openSheet({ kind: "tx", id: t.id })} last={i === g.items.length - 1} />{/each}
      </div>
    </section>
  {:else}
    {#if !loading}<p class="pt-14 text-center text-[15px] text-muted">{q ? `Nothing matches “${q}”` : "No entries for this period"}</p>{/if}
  {/each}
</div>
