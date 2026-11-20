<script lang="ts">
  // Savings goal: name, target, deadline; add or take out money.
  import OptionalDate from "./OptionalDate.svelte";
  import { showToast } from "@selfkit/ui";
  import { api, type Goal } from "../lib/api";
  import { daysBetween, money, shortDate, todayIso } from "../lib/fmt";
  import { app, closeSheet, guardForm, main, openSheet, refresh, removeRec, saveRec } from "../lib/state.svelte";
  import Confirm from "./Confirm.svelte";
  import Ic from "./Ic.svelte";
  import SheetFrame from "./SheetFrame.svelte";
  let { id, desktop = false }: { id?: string; desktop?: boolean } = $props();
  const goal = $derived(id ? app.snap?.goals.find((g) => g.id === id) : undefined);
  const o = id ? app.snap?.goals.find((g) => g.id === id) : undefined;
  let f = $state({ name: o?.name ?? "", target: o ? String(o.target) : "", currency: o?.currency ?? main(), deadline: o?.deadline ?? "" });
  const start = JSON.stringify(f);
  let asking = $state(false);
  let add = $state("");
  $effect(() => guardForm(() => JSON.stringify(f) !== start, () => (asking = true)));
  const leave = () => (JSON.stringify(f) !== start ? (asking = true) : closeSheet());
  async function save() {
    const r = await saveRec<Goal>("goals", { ...(goal ?? { contributions: [], archived: false }), ...f, target: Number(f.target) || 0 }, id);
    if (r) {
      app.guard = null;
      closeSheet();
    }
  }
  async function contribute(sign: 1 | -1) {
    if (!goal) return;
    const a = Number(add);
    if (!(a > 0)) return showToast("Type an amount");
    const saved = goal.contributions.reduce((t, c) => t + c.amount, 0);
    if (sign < 0 && a > saved + 0.005) return showToast(`Only ${money(Math.max(0, saved), goal.currency)} is saved`); // L2-11
    const before = { ...goal };
    const r = await saveRec("goals", { ...goal, ...f, target: Number(f.target) || goal.target, contributions: [...goal.contributions, { id: "", date: todayIso(), amount: sign * a, note: "" }] }, goal.id);
    if (r) {
      add = "";
      showToast(sign > 0 ? `Added ${money(a, goal.currency)} to ${goal.name}` : `Took out ${money(a, goal.currency)}`, async () => {
        await api.save("goals", before, before.id);
        await refresh();
      });
    }
  }
  async function del() {
    if (!goal) return;
    app.guard = null;
    closeSheet();
    await removeRec(goal.id, `Goal ${goal.name} deleted`);
  }
  const row = "flex min-h-12 items-center gap-3 border-b border-sep px-4 last:border-0";
  const pct = $derived(goal ? Math.max(0, Math.min(1, goal.saved / goal.target)) : 0);
</script>

<SheetFrame title={id ? "Savings goal" : "New goal"} {desktop} right={id ? "Save" : "Add"} onleft={leave} onright={save} rightDisabled={!f.name.trim() || !(Number(f.target) > 0)}>
  <div class="flex flex-col gap-4 pt-1">
    {#if goal}
      <div class="card p-4">
        <div class="num text-[28px] font-semibold tracking-[-0.6px]">{money(goal.saved, goal.currency)}<span class="text-[14px] font-medium text-muted">&nbsp;of {money(goal.target, goal.currency)}</span></div>
        <div class="mt-2 h-[7px] overflow-hidden rounded-full bg-fill"><div class="h-full rounded-full bg-accent" style="width:{pct * 100}%"></div></div>
        <div class="mt-2 text-[13px] text-muted">{pct >= 1 ? "Reached" : `${money(goal.target - goal.saved, goal.currency)} to go`}{goal.deadline ? ` · ${daysBetween(todayIso(), goal.deadline)} days left (${shortDate(goal.deadline)})` : ""}</div>
        <div class="mt-3 flex gap-2">
          <input inputmode="decimal" bind:value={add} placeholder="Amount" aria-label="Amount" class="field num flex-1 text-[16px] font-semibold" />
          <button type="button" class="press h-11 rounded-[12px] bg-fill px-3 text-[14px] font-medium" onclick={() => contribute(-1)}>Take out</button>
          <button type="button" class="press h-11 rounded-[12px] bg-accent px-4 text-[15px] font-semibold text-on-accent" onclick={() => contribute(1)}>Add</button>
        </div>
      </div>
    {/if}
    <div class="card overflow-hidden">
      <label class={row}><span class="w-24 text-[15px] text-ink2">Name</span><input bind:value={f.name} placeholder="New phone, Emergency fund" class="min-w-0 flex-1 bg-transparent py-3 text-right text-[15px] outline-none" /></label>
      <label class={row}><span class="w-24 text-[15px] text-ink2">Target</span><input inputmode="decimal" bind:value={f.target} placeholder="0" class="num min-w-0 flex-1 bg-transparent py-3 text-right text-[15px] outline-none" /></label>
      <div class={row}><span class="w-24 text-[15px] text-ink2">Currency</span><button type="button" class="press ml-auto text-[15px] font-medium text-accent" onclick={() => openSheet({ kind: "currency", value: f.currency, onpick: (c) => (f.currency = c) })}>{f.currency}</button></div>
      <label class={row}><span class="w-24 text-[15px] text-ink2">Deadline</span><OptionalDate bind:value={f.deadline} label="Deadline" cls="min-w-0 flex-1 bg-transparent py-3 text-right text-[15px] outline-none" /></label>
    </div>
    {#if goal?.contributions.length}
      <div class="card overflow-hidden">
        {#each [...goal.contributions].reverse().slice(0, 12) as c (c.id)}
          <div class="flex h-11 items-center justify-between border-b border-sep px-4 text-[14px] last:border-0"><span class="text-muted">{shortDate(c.date)}</span><span class="num font-medium {c.amount < 0 ? 'text-ink2' : 'text-accent-ink'}">{c.amount > 0 ? "+" : "−"}{money(Math.abs(c.amount), goal.currency)}</span></div>
        {/each}
      </div>
    {/if}
    {#if goal}<button type="button" class="press card h-12 w-full text-[16px] font-medium text-danger" onclick={del}><Ic name="trash" size={17} class="mr-1 inline" />Delete goal</button>{/if}
  </div>
</SheetFrame>
{#if asking}<Confirm title="Discard your changes?" onconfirm={() => ((asking = false), (app.guard = null), closeSheet())} oncancel={() => (asking = false)} />{/if}
