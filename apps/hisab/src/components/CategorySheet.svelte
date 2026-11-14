<script lang="ts">
  // New / edit category: name, icon, colour, monthly budget. Delete = archive.
  import { showToast } from "@selfkit/ui";
  import { api, type Category } from "../lib/api";
  import { symbol } from "../lib/fmt";
  import { CAT_ICON_NAMES, COLORS } from "../lib/icons";
  import { app, catById, closeSheet, guardForm, main, refresh, saveRec } from "../lib/state.svelte";
  import CatBadge from "./CatBadge.svelte";
  import Confirm from "./Confirm.svelte";
  import Ic from "./Ic.svelte";
  import SheetFrame from "./SheetFrame.svelte";
  import Seg from "./Seg.svelte";
  let { id, type = "expense", pick = false, desktop = false }: { id?: string; type?: "expense" | "income"; pick?: boolean; desktop?: boolean } = $props();
  const orig = id ? catById(id) : undefined;
  let f = $state({ name: orig?.name ?? "", kind: orig?.kind ?? type, icon: orig?.icon ?? "dot", color: orig?.color ?? COLORS[(app.snap?.categories.length ?? 0) % COLORS.length], budget: orig?.budget ? String(orig.budget) : "" });
  const start = JSON.stringify(f);
  let asking = $state(false);
  $effect(() => guardForm(() => JSON.stringify(f) !== start, () => (asking = true)));
  const leave = () => (JSON.stringify(f) !== start ? (asking = true) : closeSheet());
  async function save() {
    const order = orig?.order ?? Math.max(-1, ...(app.snap?.categories ?? []).filter((c) => c.kind === f.kind).map((c) => c.order)) + 1;
    const r = await saveRec<Category>("categories", { ...(orig ?? { archived: false }), ...f, budget: Number(f.budget) || 0, order }, orig?.id);
    if (r) {
      app.guard = null;
      if (pick && r.kind === app.draft.kind) app.draft.category = r.id;
      closeSheet();
    }
  }
  async function archive() {
    if (!orig) return;
    const was = { ...orig };
    await api.save("categories", { ...orig, archived: !orig.archived }, orig.id);
    await refresh();
    app.guard = null;
    closeSheet();
    showToast(orig.archived ? `${orig.name} restored` : `${orig.name} archived · history kept`, async () => {
      await api.save("categories", was, was.id);
      await refresh();
    });
  }
</script>

<SheetFrame title={orig ? "Category" : "New category"} {desktop} right={orig ? "Save" : "Add"} onleft={leave} onright={save} rightDisabled={!f.name.trim()}>
  <div class="flex flex-col gap-4 pt-1">
    <div class="flex items-center gap-3 px-1">
      <CatBadge icon={f.icon} color={f.color} size={52} />
      <input bind:value={f.name} placeholder="Name" aria-label="Name" class="field flex-1 text-[17px] font-medium" />
    </div>
    {#if !orig}<Seg label="List" items={[{ id: "expense", name: "Expense" }, { id: "income", name: "Income" }]} value={f.kind} onchange={(k) => (f.kind = k)} />{/if}
    <div class="card p-3">
      <p class="m-0 px-1 pb-2 text-[13px] font-semibold text-muted">Colour</p>
      <div class="flex flex-wrap gap-2 px-1">
        {#each COLORS as c}<button type="button" aria-label="Colour {c}" aria-pressed={f.color === c} class="press h-8 w-8 rounded-full {f.color === c ? 'ring-2 ring-ink ring-offset-2 ring-offset-card' : ''}" style="background:{c}" onclick={() => (f.color = c)}></button>{/each}
      </div>
      <p class="m-0 px-1 pt-4 pb-2 text-[13px] font-semibold text-muted">Icon</p>
      <div class="grid grid-cols-7 gap-1">
        {#each CAT_ICON_NAMES as n}
          <button type="button" aria-label="Icon {n}" aria-pressed={f.icon === n} class="press flex h-10 items-center justify-center rounded-[10px] {f.icon === n ? 'bg-accent-soft text-accent-ink' : 'text-ink2'}" onclick={() => (f.icon = n)}><Ic name={n} size={20} /></button>
        {/each}
      </div>
    </div>
    {#if f.kind === "expense"}
      <label class="card flex min-h-12 items-center gap-3 px-4"><span class="flex-1 text-[15px] text-ink2">Monthly budget</span>
        <span class="text-muted">{symbol(main())}</span><input inputmode="decimal" bind:value={f.budget} placeholder="None" class="num w-28 bg-transparent py-3 text-right text-[15px] outline-none" /></label>
    {/if}
    {#if orig}<button type="button" class="press card h-12 w-full text-[16px] font-medium {orig.archived ? 'text-accent' : 'text-danger'}" onclick={archive}>{orig.archived ? "Restore category" : "Delete (archive) category"}</button>
      {#if !orig.archived}<p class="m-0 -mt-2 px-1 text-[12px] text-muted">Archived categories leave the add screen, but old entries keep them, so history and trends stay correct.</p>{/if}{/if}
  </div>
</SheetFrame>
{#if asking}<Confirm title="Discard this category?" onconfirm={() => ((asking = false), (app.guard = null), closeSheet())} oncancel={() => (asking = false)} />{/if}
