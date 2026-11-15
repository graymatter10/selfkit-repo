<script lang="ts">
  import { money } from "../lib/fmt";
  import { app, go, main, openSheet, type Page } from "../lib/state.svelte";
  import CatBadge from "./CatBadge.svelte";
  import Ic from "./Ic.svelte";
  const s = $derived(app.snap);
  const lent = $derived((s?.loans ?? []).filter((l) => l.direction === "lent").reduce((a, l) => a + l.remaining_main, 0));
  const overdue = $derived((s?.loans ?? []).filter((l) => l.status === "overdue").length);
  const subs = $derived((s?.recurring ?? []).filter((r) => r.subscription && r.active && r.template.kind === "expense").reduce((a, r) => a + r.monthly, 0));
  const items = $derived<[string, string, string, Page | "export"][]>([
    ["people", "Loans", `${money(lent, main())} lent${overdue ? ` · ${overdue} overdue` : ""}`, "loans"],
    ["target", "Savings goals", (() => { const n = s?.goals.filter((g) => !g.archived).length ?? 0; return `${n} goal${n === 1 ? "" : "s"}`; })(), "goals"],
    ["repeat", "Recurring & subscriptions", `${money(subs, main())} / month`, "recurring"],
    ["tag", "Categories", `${s?.categories.filter((c) => !c.archived).length ?? 0} · budgets`, "categories"],
  ]);
</script>

<div class="flex flex-col gap-3.5">
  <div class="card overflow-hidden">
    {#each items as [icon, title, sub, to], i}
      <button type="button" class="hl flex min-h-[56px] w-full items-center gap-3 px-3.5 text-left" onclick={() => (to === "export" ? openSheet({ kind: "export" }) : go(to))}>
        <CatBadge {icon} color="#2e7a4d" size={32} />
        <span class="flex flex-1 items-center gap-2 py-4 {i < items.length - 1 ? 'border-b border-sep' : ''}"><span class="flex-1 text-[15px] font-medium">{title}</span><span class="text-[13px] text-muted">{sub}</span><Ic name="chevron" size={16} class="text-muted" /></span>
      </button>
    {/each}
  </div>
  <div class="card overflow-hidden">
    <button type="button" class="hl flex min-h-[56px] w-full items-center gap-3 px-3.5 text-left" onclick={() => openSheet({ kind: "export" })}><CatBadge icon="export" color="#8a8a8e" size={32} /><span class="flex flex-1 items-center gap-2 border-b border-sep py-4"><span class="flex-1 text-[15px] font-medium">Export CSV / PDF</span><Ic name="chevron" size={16} class="text-muted" /></span></button>
    <button type="button" class="hl flex min-h-[56px] w-full items-center gap-3 px-3.5 text-left" onclick={() => openSheet({ kind: "import" })}><CatBadge icon="import" color="#8a8a8e" size={32} /><span class="flex flex-1 items-center gap-2 border-b border-sep py-4"><span class="flex-1 text-[15px] font-medium">Import CSV</span><span class="text-[13px] text-muted">Hisab or bank file</span><Ic name="chevron" size={16} class="text-muted" /></span></button>
    <button type="button" class="hl flex min-h-[56px] w-full items-center gap-3 px-3.5 text-left" onclick={() => go("settings")}><CatBadge icon="gear" color="#8a8a8e" size={32} /><span class="flex flex-1 items-center gap-2 py-4"><span class="flex-1 text-[15px] font-medium">Settings</span><span class="text-[13px] text-muted">Theme, currency, rates</span><Ic name="chevron" size={16} class="text-muted" /></span></button>
  </div>
</div>
