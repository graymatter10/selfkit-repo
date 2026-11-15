<script lang="ts">
  // New / edit account (cash, bank, mobile wallet, card).
  import { showToast } from "@selfkit/ui";
  import type { Account } from "../lib/api";
  import { api } from "../lib/api";
  import { accountById, app, closeSheet, guardForm, main, openSheet, refresh, saveRec } from "../lib/state.svelte";
  import Confirm from "./Confirm.svelte";
  import SheetFrame from "./SheetFrame.svelte";
  import Seg from "./Seg.svelte";
  let { id, desktop = false }: { id?: string; desktop?: boolean } = $props();
  const orig = id ? accountById(id) : undefined;
  let f = $state({ name: orig?.name ?? "", kind: orig?.kind ?? "cash", currency: orig?.currency ?? main(), opening: orig ? String(orig.opening) : "", explicit: orig?.explicit_currency ?? false });
  const start = JSON.stringify(f);
  let asking = $state(false);
  $effect(() => guardForm(() => JSON.stringify(f) !== start, () => (asking = true)));
  const leave = () => (JSON.stringify(f) !== start ? (asking = true) : closeSheet());
  async function save() {
    const data = { ...(orig ?? { order: (app.snap?.accounts.length ?? 0), archived: false }), name: f.name, kind: f.kind, currency: f.currency, opening: Number(f.opening) || 0, ...(f.explicit || !orig ? { explicit_currency: f.explicit } : {}) };
    const r = await saveRec<Account>("accounts", data, orig?.id);
    if (r) {
      app.guard = null;
      if (!orig && !app.draft.account) app.draft.account = r.id;
      closeSheet();
    }
  }
  async function archive() {
    if (!orig) return;
    const was = { ...orig };
    await api.save("accounts", { ...orig, archived: !orig.archived }, orig.id);
    await refresh();
    app.guard = null;
    closeSheet();
    showToast(orig.archived ? "Account restored" : "Account archived · history kept", async () => {
      await api.save("accounts", was, was.id);
      await refresh();
    });
  }
  const row = "flex min-h-12 items-center gap-3 border-b border-sep px-4 last:border-0";
</script>

<SheetFrame title={orig ? "Account" : "New account"} {desktop} right={orig ? "Save" : "Add"} onleft={leave} onright={save} rightDisabled={!f.name.trim()}>
  <div class="flex flex-col gap-4 pt-1">
    <div class="card overflow-hidden">
      <label class={row}><span class="w-24 text-[15px] text-ink2">Name</span><input bind:value={f.name} placeholder="bKash, City Bank…" class="min-w-0 flex-1 bg-transparent py-3 text-right text-[15px] outline-none" /></label>
      <div class={row}><span class="w-24 text-[15px] text-ink2">Currency</span><button type="button" class="press ml-auto text-[15px] font-medium text-accent" onclick={() => openSheet({ kind: "currency", value: f.currency, onpick: (c) => { f.currency = c; f.explicit = true; } })}>{f.currency}</button></div>
      <label class={row}><span class="w-24 text-[15px] text-ink2">Starting balance</span><input inputmode="decimal" bind:value={f.opening} placeholder="0" class="num min-w-0 flex-1 bg-transparent py-3 text-right text-[15px] outline-none" /></label>
    </div>
    <Seg label="Kind" items={[{ id: "cash", name: "Cash" }, { id: "bank", name: "Bank" }, { id: "wallet", name: "Wallet" }, { id: "card", name: "Card" }, { id: "other", name: "Other" }]} value={f.kind} onchange={(k) => (f.kind = k)} />
    {#if orig}<button type="button" class="press card h-12 w-full text-[16px] font-medium {orig.archived ? 'text-accent' : 'text-danger'}" onclick={archive}>{orig.archived ? "Restore account" : "Archive account"}</button>{/if}
  </div>
</SheetFrame>
{#if asking}<Confirm title="Discard this account?" onconfirm={() => ((asking = false), (app.guard = null), closeSheet())} oncancel={() => (asking = false)} />{/if}
