<script lang="ts">
  // Recurring transaction or subscription.
  import OptionalDate from "./OptionalDate.svelte";
  import type { Recurring } from "../lib/api";
  import { todayIso } from "../lib/fmt";
  import { accounts, app, closeSheet, defaultAccount, guardForm, main, openSheet, removeRec, saveRec } from "../lib/state.svelte";
  import Confirm from "./Confirm.svelte";
  import SheetFrame from "./SheetFrame.svelte";
  import Seg from "./Seg.svelte";
  let { id, desktop = false }: { id?: string; desktop?: boolean } = $props();
  const o = id ? app.snap?.recurring.find((r) => r.id === id) : undefined;
  const t = o?.template ?? {};
  let f = $state({
    name: o?.name ?? "", subscription: o?.subscription ?? true, kind: (t.kind as "expense" | "income") ?? "expense", amount: t.amount ? String(t.amount) : "",
    currency: t.currency ?? main(), account: t.account ?? defaultAccount(), category: t.category ?? "", freq: o?.freq ?? "monthly", interval: String(o?.interval ?? 1),
    start: o?.start ?? todayIso(), next: o?.next ?? "", end: o?.end ?? "", active: o?.active ?? true,
  });
  const begin = JSON.stringify(f);
  let asking = $state(false);
  $effect(() => guardForm(() => JSON.stringify(f) !== begin, () => (asking = true)));
  const leave = () => (JSON.stringify(f) !== begin ? (asking = true) : closeSheet());
  const cats = $derived((app.snap?.categories ?? []).filter((c) => c.kind === f.kind && (!c.archived || c.id === f.category)));
  async function save() {
    const next = !o || f.start !== o.start ? (f.start < todayIso() && !o ? f.start : f.start) : f.next;
    const data = {
      name: f.name, subscription: f.subscription, freq: f.freq, interval: Number(f.interval) || 1, start: f.start, next: o && f.start === o.start ? f.next : next, end: f.end, active: f.active,
      template: { kind: f.kind, amount: Number(f.amount) || 0, currency: f.currency, account: f.account, category: f.category, note: f.name },
    };
    const r = await saveRec<Recurring>("recurring", data, id);
    if (r) {
      app.guard = null;
      closeSheet();
      const { api } = await import("../lib/api");
      await api.generateRecurring();
    }
  }
  async function del() {
    if (!o) return;
    app.guard = null;
    closeSheet();
    await removeRec(o.id, `${o.name} stopped`);
  }
  const row = "flex min-h-12 items-center gap-3 border-b border-sep px-4 last:border-0";
  const lbl = "w-24 shrink-0 text-[15px] text-ink2";
  const inp = "min-w-0 flex-1 bg-transparent py-3 text-right text-[15px] outline-none";
</script>

<SheetFrame title={id ? "Recurring" : "New recurring"} {desktop} right={id ? "Save" : "Add"} onleft={leave} onright={save} rightDisabled={!f.name.trim() || !(Number(f.amount) > 0) || !f.category || !f.account}>
  <div class="flex flex-col gap-4 pt-1">
    <Seg label="Type" items={[{ id: "expense", name: "Expense" }, { id: "income", name: "Income" }]} value={f.kind} onchange={(k) => ((f.kind = k), (f.category = ""))} />
    <div class="card overflow-hidden">
      <label class={row}><span class={lbl}>Name</span><input bind:value={f.name} placeholder="Netflix, Rent, Salary" class={inp} /></label>
      <label class={row}><span class={lbl}>Amount</span><input inputmode="decimal" bind:value={f.amount} placeholder="0" class="{inp} num font-semibold" /></label>
      <div class={row}><span class={lbl}>Currency</span><button type="button" class="press ml-auto text-[15px] font-medium text-accent" onclick={() => openSheet({ kind: "currency", value: f.currency, onpick: (c) => (f.currency = c) })}>{f.currency}</button></div>
      <label class={row}><span class={lbl}>Category</span><select bind:value={f.category} class="{inp} appearance-none"><option value="">Choose…</option>{#each cats as c (c.id)}<option value={c.id}>{c.name}</option>{/each}</select></label>
      <label class={row}><span class={lbl}>Account</span><select bind:value={f.account} class="{inp} appearance-none">{#each accounts() as a (a.id)}<option value={a.id}>{a.name}</option>{/each}</select></label>
    </div>
    <div class="card overflow-hidden">
      <label class={row}><span class={lbl}>Every</span>
        <select bind:value={f.interval} class="w-14 appearance-none bg-transparent py-3 text-right text-[15px] outline-none">{#each [1, 2, 3, 4, 6] as n}<option value={String(n)}>{n}</option>{/each}</select>
        <select bind:value={f.freq} class="{inp} appearance-none"><option value="daily">day(s)</option><option value="weekly">week(s)</option><option value="monthly">month(s)</option><option value="yearly">year(s)</option></select></label>
      <label class={row}><span class={lbl}>Starts</span><input type="date" bind:value={f.start} class={inp} /></label>
      {#if o}<div class="{row} text-[14px] text-muted"><span class={lbl}>Next</span><span class="ml-auto">{f.next}</span></div>{/if}
      <label class={row}><span class={lbl}>Ends</span><OptionalDate bind:value={f.end} label="Ends" empty="Never" cls={inp} /></label>
      <label class={row}><span class="flex-1 text-[15px] text-ink2">Subscription</span><input type="checkbox" bind:checked={f.subscription} class="h-5 w-5 accent-[var(--accent)]" /></label>
      {#if o}<label class={row}><span class="flex-1 text-[15px] text-ink2">Active</span><input type="checkbox" bind:checked={f.active} class="h-5 w-5 accent-[var(--accent)]" /></label>{/if}
    </div>
    <p class="m-0 -mt-2 px-1 text-[12px] text-muted">Entries are added automatically on their date (also past dates since the start).</p>
    {#if o}<button type="button" class="press card h-12 w-full text-[16px] font-medium text-danger" onclick={del}>Delete</button>{/if}
  </div>
</SheetFrame>
{#if asking}<Confirm title="Discard your changes?" onconfirm={() => ((asking = false), (app.guard = null), closeSheet())} oncancel={() => (asking = false)} />{/if}
