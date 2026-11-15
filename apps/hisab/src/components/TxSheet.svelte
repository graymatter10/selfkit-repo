<script lang="ts">
  // View and edit one transaction (amount, currency, date, account, category
  // or split, tags, note, photos). Delete is undoable.
  import { showToast } from "@selfkit/ui";
  import type { Tx } from "../lib/api";
  import { money } from "../lib/fmt";
  import { addPhoto } from "../lib/photo";
  import { accountById, app, catById, closeSheet, editRec, main, openSheet, removeRec } from "../lib/state.svelte";
  import Confirm from "./Confirm.svelte";
  import Ic from "./Ic.svelte";
  import Photo from "./Photo.svelte";
  import SheetFrame from "./SheetFrame.svelte";
  import Seg from "./Seg.svelte";

  let { id, desktop = false }: { id: string; desktop?: boolean } = $props();
  const orig = $state.snapshot(app.snap?.recent.find((t) => t.id === id)) as Tx | undefined;
  let t = $state<Tx | null>(null);
  let start = "";
  let tagText = $state("");
  let splits = $state<{ category: string; amount: string }[]>([]);
  let asking = $state(false);

  import("../lib/api").then(async ({ api }) => {
    const r = (await api.get<Tx>(id)) ?? orig ?? null;
    if (r) {
      t = { ...r, tags: r.tags ?? [], photos: r.photos ?? [], splits: r.splits ?? [] };
      tagText = t.tags.map((x) => "#" + x).join(" ");
      splits = (t.splits.length > 1 ? t.splits : []).map((s) => ({ category: s.category, amount: String(s.amount) }));
      start = sig();
    }
  });

  function sig() {
    return JSON.stringify([t, tagText, splits]);
  }
  const dirty = () => !!t && start !== "" && sig() !== start;

  app.guard = () => {
    if (dirty()) {
      asking = true;
      return true;
    }
    return false;
  };
  $effect(() => () => (app.guard = null));

  function leave() {
    if (dirty()) asking = true;
    else closeSheet();
  }

  async function save() {
    if (!t) return;
    const data: Partial<Tx> = { ...t, amount: Number(t.amount), tags: tagText.split(/[\s,]+/).map((x) => x.replace(/^#/, "")).filter(Boolean) };
    if (t.kind === "expense" && splits.length > 1) {
      data.splits = splits.map((s) => ({ category: s.category, amount: Number(s.amount) || 0 }));
      data.category = splits[0].category;
    } else data.splits = [];
    if (t.kind === "transfer") data.to_amount = Number(t.to_amount) || 0;
    const r = await editRec<Tx>("transactions", JSON.parse(start)[0], data);
    if (r) {
      app.guard = null;
      closeSheet();
    }
  }

  async function del() {
    if (!t) return;
    app.guard = null;
    closeSheet();
    await removeRec(t.id, `Deleted · ${money(t.amount, t.currency)}`);
  }

  async function photo() {
    try {
      const p = await addPhoto();
      if (p && t) t.photos = [...t.photos, p];
    } catch (e) {
      showToast(String(e));
    }
  }

  const cats = $derived((app.snap?.categories ?? []).filter((c) => t && c.kind === t.kind && (!c.archived || c.id === t.category || splits.some((s) => s.category === c.id))));
  const splitSum = $derived(splits.reduce((a, s) => a + (Number(s.amount) || 0), 0));
  const row = "flex min-h-12 items-center gap-3 border-b border-sep px-4 last:border-0";
  const lbl = "w-24 shrink-0 text-[15px] text-ink2";
  const inp = "min-w-0 flex-1 bg-transparent py-3 text-right text-[15px] outline-none";
</script>

<SheetFrame title={t ? (t.kind === "transfer" ? "Transfer" : t.kind === "income" ? "Income" : "Expense") : ""} {desktop} left="Cancel" right="Save" onleft={leave} onright={save} rightDisabled={!t}>
  {#if t}
    <div class="flex flex-col gap-4 pt-1">
      <Seg label="Type" items={[{ id: "expense", name: "Expense" }, { id: "income", name: "Income" }, { id: "transfer", name: "Transfer" }]} value={t.kind}
        onchange={(k) => t && ((t.kind = k), (t.category = ""), (splits = []))} />
      <div class="card overflow-hidden">
        <label class={row}><span class={lbl}>Amount</span><input inputmode="decimal" bind:value={t.amount} class="{inp} num text-[17px] font-semibold" /></label>
        <div class={row}><span class={lbl}>Currency</span><button type="button" class="press ml-auto text-[15px] font-medium text-accent" onclick={() => t && openSheet({ kind: "currency", value: t.currency, onpick: (c) => t && (t.currency = c) })}>{t.currency}</button></div>
        {#if t.currency !== main() && t.amount_main !== undefined}<div class="{row} text-[13px] text-muted"><span class={lbl}>In {main()}</span><span class="num ml-auto">{money(t.amount_main, main())}</span></div>{/if}
        <label class={row}><span class={lbl}>Date</span><input type="date" bind:value={t.date} class={inp} /></label>
        <label class={row}><span class={lbl}>Time</span><input type="time" bind:value={t.time} class={inp} /></label>
        <label class={row}><span class={lbl}>{t.kind === "transfer" ? "From" : "Account"}</span>
          <select bind:value={t.account} class="{inp} appearance-none">{#each app.snap?.accounts ?? [] as a (a.id)}<option value={a.id}>{a.name}</option>{/each}</select></label>
        {#if t.kind === "transfer"}
          <label class={row}><span class={lbl}>To</span>
            <select bind:value={t.to_account} class="{inp} appearance-none"><option value="">Choose…</option>{#each app.snap?.accounts ?? [] as a (a.id)}<option value={a.id}>{a.name}</option>{/each}</select></label>
          <label class={row}><span class={lbl}>Arrives as</span><input inputmode="decimal" bind:value={t.to_amount} class="{inp} num" /><span class="text-[13px] text-muted">{accountById(t.to_account ?? "")?.currency ?? ""}</span></label>
        {/if}
      </div>

      {#if t.kind !== "transfer"}
        <div class="card overflow-hidden">
          {#if splits.length < 2}
            <label class={row}><span class={lbl}>Category</span>
              <select bind:value={t.category} class="{inp} appearance-none"><option value="">Choose…</option>{#each cats as c (c.id)}<option value={c.id}>{c.name}{c.archived ? " (archived)" : ""}</option>{/each}</select></label>
          {:else}
            {#each splits as s, i}
              <div class={row}>
                <select bind:value={s.category} aria-label="Split category {i + 1}" class="min-w-0 flex-1 appearance-none bg-transparent py-3 text-[15px] outline-none">{#each cats as c (c.id)}<option value={c.id}>{c.name}</option>{/each}</select>
                <input inputmode="decimal" bind:value={s.amount} aria-label="Split amount {i + 1}" class="num w-24 bg-transparent text-right text-[15px] outline-none" />
                <button type="button" aria-label="Remove part" class="press text-muted" onclick={() => (splits = splits.filter((_, j) => j !== i))}><Ic name="close" size={16} /></button>
              </div>
            {/each}
            <div class="{row} text-[13px] {Math.abs(splitSum - Number(t.amount)) > 0.005 ? 'text-danger' : 'text-muted'}">Parts add up to {splitSum.toFixed(2)} of {Number(t.amount).toFixed(2)}</div>
          {/if}
          {#if t.kind === "expense"}
            <button type="button" class="{row} w-full text-[15px] text-accent" onclick={() => {
              if (!t) return;
              if (splits.length < 2) splits = [{ category: t.category || cats[0]?.id || "", amount: String(t.amount) }, { category: cats.find((c) => c.id !== t?.category)?.id ?? "", amount: "0" }];
              else splits = [...splits, { category: cats[0]?.id ?? "", amount: "0" }];
            }}><Ic name="plus" size={17} />{splits.length < 2 ? "Split across categories" : "Add part"}</button>
          {/if}
        </div>
      {/if}

      <div class="card overflow-hidden">
        <label class="flex items-start gap-3 border-b border-sep px-4"><span class="{lbl} pt-3">Note</span><textarea bind:value={t.note} rows="2" class="min-w-0 flex-1 resize-none bg-transparent py-3 text-[15px] outline-none"></textarea></label>
        <label class={row}><span class={lbl}>Tags</span><input bind:value={tagText} placeholder="#trip #work" class="{inp} text-left" /></label>
      </div>

      <div>
        <div class="flex items-center justify-between px-1 pb-1.5"><span class="text-[13px] font-semibold text-muted">Photos</span>
          <button type="button" class="press flex items-center gap-1 text-[14px] font-medium text-accent" onclick={photo}><Ic name="camera" size={16} />Add</button></div>
        {#if t.photos.length}
          <div class="flex flex-wrap gap-2">
            {#each t.photos as p (p)}
              <div class="relative">
                <button type="button" aria-label="View photo" class="press block overflow-hidden rounded-[12px]" onclick={() => openSheet({ kind: "photo", id: p })}><Photo id={p} class="h-24 w-24 object-cover" /></button>
                <button type="button" aria-label="Remove photo" class="absolute -top-1.5 -right-1.5 flex h-6 w-6 items-center justify-center rounded-full bg-ink text-bg" onclick={() => t && (t.photos = t.photos.filter((x) => x !== p))}><Ic name="close" size={13} stroke={2.4} /></button>
              </div>
            {/each}
          </div>
        {:else}<p class="m-0 px-1 text-[13px] text-muted">No photos. Add a receipt to keep proof.</p>{/if}
      </div>

      <p class="m-0 px-1 text-[12px] text-muted">
        {t.source === "sms" ? "Imported from SMS" : t.source === "import" ? "Imported from CSV" : t.source === "recurring" ? "Made by a recurring item" : "Added by hand"}{t.created_at ? " · " + new Date(t.created_at).toLocaleString() : ""}
        {#if t.kind !== "transfer" && catById(t.category)?.archived} · category archived{/if}
      </p>
      <button type="button" class="press card h-12 w-full text-[16px] font-medium text-danger" onclick={del}>Delete</button>
    </div>
  {/if}
</SheetFrame>
{#if asking}<Confirm title="Discard your changes?" onconfirm={() => ((asking = false), (app.guard = null), closeSheet())} oncancel={() => (asking = false)} />{/if}
