<script lang="ts">
  // Category manager: expense/income lists, ranking (My order = drag, or Most
  // used), budgets, archive (delete) and restore.
  import { showToast } from "@selfkit/ui";
  import { api, type Category } from "../lib/api";
  import { money } from "../lib/fmt";
  import { app, main, openSheet, refresh, setPrefs } from "../lib/state.svelte";
  import CatBadge from "./CatBadge.svelte";
  import Ic from "./Ic.svelte";
  import Seg from "./Seg.svelte";
  let { desktop = false }: { desktop?: boolean } = $props();
  let kind = $state<"expense" | "income">("expense");
  let showArchived = $state(false);
  const rank = $derived(app.prefs.rank ?? "manual");
  const base = $derived((app.snap?.categories ?? []).filter((c) => c.kind === kind && !c.archived));
  const archived = $derived((app.snap?.categories ?? []).filter((c) => c.kind === kind && c.archived));
  let order = $state<Category[]>([]);
  $effect(() => {
    order = rank === "used" ? [...base].sort((a, b) => b.usage - a.usage || a.order - b.order) : [...base];
  });

  // Drag to reorder (pointer events on the handle; rows are ROW px tall).
  const ROW = 60;
  let drag = $state<{ id: string; startY: number; startI: number; dy: number } | null>(null);
  function down(e: PointerEvent, c: Category, i: number) {
    if (rank !== "manual") return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    drag = { id: c.id, startY: e.clientY, startI: i, dy: 0 };
  }
  function move(e: PointerEvent) {
    if (!drag) return;
    drag.dy = e.clientY - drag.startY;
    const to = Math.max(0, Math.min(order.length - 1, drag.startI + Math.round(drag.dy / ROW)));
    const from = order.findIndex((c) => c.id === drag!.id);
    if (to !== from) {
      const a = [...order];
      const [x] = a.splice(from, 1);
      a.splice(to, 0, x);
      order = a;
    }
  }
  async function up() {
    if (!drag) return;
    drag = null;
    await saveOrder(order);
  }
  async function saveOrder(list: Category[]) {
    const changed = list.map((c, i) => ({ c, i })).filter(({ c, i }) => c.order !== i);
    for (const { c, i } of changed) await api.save("categories", { ...c, order: i }, c.id);
    if (changed.length) await refresh();
  }
  async function nudge(i: number, d: number) {
    const j = i + d;
    if (j < 0 || j >= order.length) return;
    const a = [...order];
    [a[i], a[j]] = [a[j], a[i]];
    order = a;
    await saveOrder(a);
  }
  async function archive(c: Category) {
    await api.save("categories", { ...c, archived: true }, c.id);
    await refresh();
    showToast(`${c.name} archived · history kept`, async () => {
      await api.save("categories", { ...c, archived: false }, c.id);
      await refresh();
    });
  }
  async function restore(c: Category) {
    await api.save("categories", { ...c, archived: false, order: base.length }, c.id);
    await refresh();
    showToast(`${c.name} restored`);
  }
</script>

<svelte:window onpointermove={move} onpointerup={up} />

<div class="flex flex-col gap-3 {desktop ? 'max-w-[680px]' : ''}">
  <Seg label="List" items={[{ id: "expense", name: "Expense" }, { id: "income", name: "Income" }]} value={kind} onchange={(v) => (kind = v)} />
  <div class="flex items-center justify-between px-1">
    <span class="text-[13px] text-muted">Order on the add screen</span>
    <div class="w-[200px]"><Seg small label="Ranking" items={[{ id: "manual", name: "My order" }, { id: "used", name: "Most used" }]} value={rank} onchange={(v) => setPrefs({ rank: v })} /></div>
  </div>
  <div class="card overflow-hidden" role="list">
    {#each order as c, i (c.id)}
      <div role="listitem" class="flex h-[60px] items-center gap-3 bg-card px-3.5 {drag?.id === c.id ? 'relative z-10 shadow-lg' : ''}">
        <button type="button" class="press flex min-w-0 flex-1 items-center gap-3 text-left" onclick={() => openSheet({ kind: "category", id: c.id })}>
          <CatBadge icon={c.icon} color={c.color} />
          <span class="flex min-w-0 flex-1 flex-col">
            <span class="flex items-baseline gap-2"><span class="min-w-0 flex-1 truncate text-[15px] font-medium">{c.name}</span>
              <span class="num text-[13px] {c.budget && c.spent > c.budget ? 'text-danger' : 'text-muted'}">{kind === "expense" ? money(c.spent, main()) + (c.budget ? ` / ${money(c.budget, main())}` : "") : money(c.spent, main())}</span></span>
            {#if c.budget}<span class="mt-1.5 h-[5px] overflow-hidden rounded-full bg-fill"><span class="block h-full rounded-full {c.spent > c.budget ? 'bg-danger' : 'bg-accent'}" style="width:{Math.min(100, (c.spent / c.budget) * 100)}%"></span></span>
            {:else}<span class="text-[12.5px] text-muted">{rank === "used" ? `${c.usage} ${c.usage === 1 ? "entry" : "entries"}` : kind === "expense" ? "No budget" : `${c.usage} ${c.usage === 1 ? "entry" : "entries"}`}</span>{/if}
          </span>
        </button>
        <button type="button" aria-label="Archive {c.name}" class="press flex h-9 w-9 items-center justify-center rounded-full text-muted" onclick={() => archive(c)}><Ic name="trash" size={17} /></button>
        {#if rank === "manual"}
          <span role="button" tabindex="0" aria-label="Drag to reorder {c.name} (arrow keys move)" class="flex h-10 w-8 cursor-grab touch-none items-center justify-center text-faint"
            onpointerdown={(e) => down(e, c, i)} onkeydown={(e) => (e.key === "ArrowUp" ? nudge(i, -1) : e.key === "ArrowDown" ? nudge(i, 1) : null)}>
            <svg width="14" height="18" viewBox="0 0 14 18" fill="currentColor" aria-hidden="true">{#each [3, 9, 15] as y}<circle cx="4" cy={y} r="1.6" /><circle cx="10" cy={y} r="1.6" />{/each}</svg>
          </span>
        {/if}
      </div>
    {/each}
  </div>
  <p class="m-0 px-1 text-[12.5px] text-muted">{rank === "manual" ? "Drag the handle to reorder. " : "Sorted by how often you use them. "}The bin archives a category: it leaves the add screen, history keeps it.</p>
  <button type="button" class="press card flex h-12 items-center justify-center gap-1.5 text-[15px] font-semibold text-accent-ink" onclick={() => openSheet({ kind: "category", type: kind })}><Ic name="plus" size={18} stroke={2.2} />New category</button>
  {#if archived.length}
    <button type="button" class="flex items-baseline justify-between px-1 pt-2 text-[13px]" onclick={() => (showArchived = !showArchived)}><b class="text-ink2">Archived · {archived.length}</b><span class="font-semibold text-accent">{showArchived ? "Hide" : "Show"}</span></button>
    {#if showArchived}
      <div class="card fade overflow-hidden">
        {#each archived as c (c.id)}
          <div class="flex h-14 items-center gap-3 border-b border-sep px-3.5 last:border-0">
            <CatBadge icon={c.icon} color={c.color} size={30} /><span class="flex-1 truncate text-[15px] text-muted">{c.name}</span>
            <button type="button" class="press rounded-full bg-accent-soft px-3 py-1 text-[13px] font-semibold text-accent-ink" onclick={() => restore(c)}>Restore</button>
          </div>
        {/each}
      </div>
    {/if}
  {/if}
</div>
