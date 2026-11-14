<script lang="ts">
  // Loans lent / borrowed with status bars.
  import type { Loan } from "../lib/api";
  import { daysBetween, money, shortDate, todayIso } from "../lib/fmt";
  import { app, main, openSheet } from "../lib/state.svelte";
  import Ic from "./Ic.svelte";
  import Seg from "./Seg.svelte";
  let { desktop = false }: { desktop?: boolean } = $props();
  let dir = $state<"lent" | "borrowed">("lent");
  const all = $derived(app.snap?.loans ?? []);
  const sum = (d: string) => all.filter((l) => l.direction === d).reduce((a, l) => a + l.remaining_main, 0);
  const open = $derived(all.filter((l) => l.direction === dir && l.status !== "finished"));
  const done = $derived(all.filter((l) => l.direction === dir && l.status === "finished"));
  const STATUS: Record<string, [string, string]> = { overdue: ["Overdue", "bg-danger-soft text-danger"], due_soon: ["Due soon", "bg-fill text-ink2"], ongoing: ["Ongoing", "bg-accent-soft text-accent-ink"] };
  function due(l: Loan) {
    if (!l.due) return "No due date";
    const d = daysBetween(todayIso(), l.due);
    return d < 0 ? `was due ${shortDate(l.due)}` : d === 0 ? "due today" : d <= 7 ? `due in ${d} day${d > 1 ? "s" : ""}` : `due ${shortDate(l.due)}`;
  }
</script>

<div class="flex flex-col gap-3">
  <Seg label="Direction" items={[{ id: "lent", name: `Lent · ${money(sum("lent"), main())}` }, { id: "borrowed", name: `Borrowed · ${money(sum("borrowed"), main())}` }]} value={dir} onchange={(v) => (dir = v)} />
  <div class="grid gap-3 {desktop ? 'grid-cols-2' : ''}">
    {#each open as l (l.id)}
      <button type="button" class="press card block p-4 text-left" onclick={() => openSheet({ kind: "loan", id: l.id })}>
        <div class="flex items-center gap-2.5">
          <span class="flex h-9 w-9 items-center justify-center rounded-full bg-fill text-[14px] font-semibold text-ink2">{l.person.slice(0, 1).toUpperCase()}</span>
          <div class="min-w-0 flex-1"><div class="truncate text-[15px] font-semibold">{l.person}</div><div class="truncate text-[12.5px] text-muted">{l.title || l.description || (dir === "lent" ? "Owes you" : "You owe")}</div></div>
          <span class="rounded-full px-2 py-0.5 text-[11.5px] font-semibold {STATUS[l.status][1]}">{l.status === "due_soon" && l.days_left !== null ? (l.days_left === 0 ? "Due today" : `Due in ${l.days_left}d`) : STATUS[l.status][0]}</span>
        </div>
        <div class="mt-3 flex items-baseline justify-between">
          <span class="num text-[20px] font-semibold">{money(l.remaining, l.currency)}<span class="text-[13px] font-medium text-muted">&nbsp;left of {money(l.amount, l.currency)}</span></span>
          <span class="text-[12.5px] {l.status === 'overdue' ? 'text-danger' : 'text-muted'}">{due(l)}</span>
        </div>
        <div class="mt-2 h-[7px] overflow-hidden rounded-full bg-fill"><div class="h-full rounded-full {l.status === 'overdue' ? 'bg-danger' : 'bg-accent'}" style="width:{(l.repaid / l.amount) * 100}%"></div></div>
        <div class="mt-2 flex items-center gap-1.5 text-[12.5px] text-muted">
          {l.repayments.length} repayment{l.repayments.length === 1 ? "" : "s"}{l.photos.length ? ` · ${l.photos.length} proof photo${l.photos.length > 1 ? "s" : ""}` : ""}
          {#if l.due && l.remind}<Ic name="bell" size={12} class="ml-auto" />{/if}
        </div>
      </button>
    {:else}
      <p class="m-0 py-8 text-center text-[15px] text-muted">{dir === "lent" ? "Nobody owes you money." : "You don't owe anyone."}</p>
    {/each}
  </div>
  {#if done.length}
    <p class="m-0 px-1 pt-1 text-[13px] font-semibold text-muted">Finished</p>
    <div class="card overflow-hidden">
      {#each done as l (l.id)}<button type="button" class="hl flex h-12 w-full items-center justify-between border-b border-sep px-4 text-left text-[15px] last:border-0" onclick={() => openSheet({ kind: "loan", id: l.id })}><span class="truncate text-ink2">{l.person}{l.title ? ` · ${l.title}` : ""}</span><span class="num text-[13px] text-muted">{money(l.amount, l.currency)} · repaid</span></button>{/each}
    </div>
  {/if}
  <button type="button" class="press flex h-[52px] items-center justify-center gap-1.5 rounded-[14px] bg-accent text-[17px] font-semibold text-on-accent {desktop ? 'w-60' : ''}" onclick={() => openSheet({ kind: "loan" })}><Ic name="plus" size={20} stroke={2.2} />New loan</button>
</div>
