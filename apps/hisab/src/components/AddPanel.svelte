<script lang="ts">
  // The fixed input panel: Expense / Income / Transfer, big amount, account
  // and currency, sliding category row, note (with #tags), date, photo,
  // split, number pad and one Save button. The draft is persisted on every
  // change, so Back, a crash or a restart never loses what was typed.
  import { showToast } from "@selfkit/ui";
  import { api, type Tx } from "../lib/api";
  import { LOCALE, money, nowHm, shortDate, symbol, todayIso, dayLabel } from "../lib/fmt";
  import { addPhoto } from "../lib/photo";
  import { accountById, accounts, app, catById, defaultAccount, emptyDraft, openSheet, persistDraft, rankedCats, refresh, resetDraft, main } from "../lib/state.svelte";
  import CategoryRow from "./CategoryRow.svelte";
  import Ic from "./Ic.svelte";
  import NumPad from "./NumPad.svelte";
  import Seg from "./Seg.svelte";

  let { compact = false, onsaved }: { compact?: boolean; onsaved?: (t: Tx) => void } = $props();
  let menu = $state<"" | "account" | "to">("");
  let noteFocus = $state(false);
  let busy = $state(false);
  let shake = $state(false);

  const acct = $derived(accountById(app.draft.account) ?? accountById(defaultAccount()));
  const toAcct = $derived(accountById(app.draft.to_account));
  // Main currency unless the account's own was chosen by the user (Rust: currency::entry_currency).
  const cur = $derived(app.draft.currency || acct?.entry_currency || main());
  const cats = $derived(app.draft.kind === "transfer" ? [] : rankedCats(app.draft.kind));
  const amountNum = $derived(parseFloat(app.draft.amount || "0") || 0);
  const splitIds = $derived(app.draft.splits.map((s) => s.category));
  const canSave = $derived(amountNum > 0 && !busy);
  // Long amounts shrink so "9,999,999,999.99" fits next to the chips on a 360 dp phone.
  const amountPx = $derived.by(() => {
    const n = display(app.draft.amount).length;
    return compact ? (n > 12 ? 22 : n > 9 ? 27 : 34) : (n > 12 ? 25 : n > 9 ? 31 : 40);
  });

  $effect(() => {
    // Keep the draft saved (and pick a default account once data is loaded).
    JSON.stringify(app.draft);
    if (!app.draft.account && app.snap) app.draft.account = defaultAccount();
    persistDraft();
  });

  export function key(k: string) {
    let a = app.draft.amount;
    if (k === "clear") a = "";
    else if (k === "back") a = a.slice(0, -1);
    else if (k === ".") a = a.includes(".") ? a : (a || "0") + ".";
    else if (/^\d$/.test(k)) {
      const [, frac] = a.split(".");
      if (frac !== undefined && frac.length >= 2) return;
      if (a.replace(".", "").length >= 10) return;
      a = a === "0" ? k : a + k;
    }
    app.draft.amount = a;
  }

  function display(a: string) {
    if (!a) return "0";
    const [i, f] = a.split(".");
    const int = Number(i || "0").toLocaleString(LOCALE);
    return f !== undefined ? `${int}.${f}` : int;
  }

  function setKind(k: "expense" | "income" | "transfer") {
    if (k === app.draft.kind) return;
    app.draft.kind = k;
    app.draft.category = "";
    app.draft.splits = [];
  }

  function pickCat(id: string) {
    if (app.draft.splits.length > 1) {
      showToast("This entry is split. Change it with Split.");
      return;
    }
    app.draft.category = app.draft.category === id ? "" : id;
  }

  async function photo() {
    try {
      const id = await addPhoto();
      if (id) app.draft.photos = [...app.draft.photos, id];
    } catch (e) {
      showToast(String(e));
    }
  }

  function bump() {
    shake = true;
    setTimeout(() => (shake = false), 350);
  }

  export async function save() {
    if (busy) return;
    if (amountNum <= 0) return bump();
    const dr = app.draft;
    if (dr.kind !== "transfer" && !dr.category && dr.splits.length < 2) {
      showToast("Pick a category");
      return;
    }
    if (dr.kind === "transfer" && (!dr.to_account || dr.to_account === (acct?.id ?? ""))) {
      menu = "to";
      showToast("Pick where the money goes");
      return;
    }
    // "#tags" in the note become tags.
    const tags = [...dr.tags];
    const note = dr.note.replace(/(^|\s)#([\p{L}\p{N}_-]+)/gu, (_, sp, t) => {
      tags.push(t.toLowerCase());
      return sp;
    }).replace(/\s+/g, " ").trim();
    const data: Partial<Tx> = {
      kind: dr.kind, amount: amountNum, currency: cur, date: dr.date || todayIso(), time: dr.time || (dr.date === todayIso() || !dr.date ? nowHm() : ""),
      account: acct?.id ?? "", note, tags, photos: dr.photos, source: "manual",
    };
    if (dr.kind === "transfer") {
      data.to_account = dr.to_account;
    } else if (dr.splits.length > 1) {
      data.splits = dr.splits.map((s) => ({ category: s.category, amount: parseFloat(s.amount) || 0 }));
      data.category = dr.splits[0].category;
    } else data.category = dr.category;
    busy = true;
    try {
      // The cleared draft is written by the same command as the entry, and reset
      // here before anything else is awaited: a kill or Home right after Save
      // used to bring the saved amount back as a draft (double entry on next Save).
      const keep = { kind: dr.kind, account: dr.account };
      const t = await api.save<Tx>("transactions", data, undefined, { ...emptyDraft(), ...keep });
      resetDraft(keep);
      await refresh();
      const label = dr.kind === "transfer" ? "Moved" : dr.kind === "income" ? "Income saved" : "Saved";
      showToast(`${label} · ${money(t.amount, t.currency)}${dr.kind !== "transfer" ? " · " + (catById(t.category)?.name ?? "") : ""}`, async () => {
        await api.remove(t.id);
        await refresh();
        app.draft = { ...app.draft, ...{ amount: String(t.amount), category: dr.category, splits: dr.splits, note: dr.note, photos: dr.photos, date: dr.date, kind: dr.kind, to_account: dr.to_account, currency: dr.currency } };
      });
      onsaved?.(t);
    } catch (e) {
      showToast(String(e));
    } finally {
      busy = false;
    }
  }

  function pickAccount(id: string) {
    if (menu === "to") app.draft.to_account = id;
    else {
      app.draft.account = id;
      app.draft.currency = "";
      if (app.draft.to_account === id) app.draft.to_account = "";
    }
    menu = "";
  }
</script>

<div class="relative flex flex-col {compact ? 'gap-2.5' : 'gap-2 pt-2.5'}">
  <Seg label="Type" small={compact} items={[{ id: "expense", name: "Expense" }, { id: "income", name: "Income" }, { id: "transfer", name: "Transfer" }]} value={app.draft.kind} onchange={setKind} />

  <!-- Amount, currency, account -->
  <div class="flex items-end gap-2 px-1">
    <div class="min-w-0 flex-1 {shake ? 'animate-[shake_.3s]' : ''}">
      <div class="num flex items-baseline gap-1 truncate {app.draft.amount ? 'text-ink' : 'text-faint'} {app.draft.kind === 'income' && app.draft.amount ? 'text-accent-ink' : ''}" aria-live="polite" aria-label="Amount">
        <span class="text-[22px] font-medium text-muted">{symbol(cur)}</span>
        <span class="leading-none font-semibold tracking-[-1px]" style="font-size: {amountPx}px">{display(app.draft.amount)}</span>
      </div>
    </div>
    <div class="flex shrink-0 flex-col items-end gap-1 pb-0.5">
      <div class="flex items-center gap-1">
        <button type="button" class="press h-7 rounded-full bg-fill px-2.5 text-[13px] font-semibold" aria-label="Currency {cur}"
          onclick={() => openSheet({ kind: "currency", value: cur, onpick: (c) => (app.draft.currency = c === (acct?.entry_currency || main()) ? "" : c) })}>{cur}</button>
        <button type="button" class="press flex h-7 max-w-[150px] items-center gap-1 rounded-full bg-fill px-2.5 text-[13px] font-medium" aria-haspopup="menu" onclick={() => (menu = menu === "account" ? "" : "account")}>
          <span class="truncate">{acct?.name ?? "Account"}</span><Ic name="down" size={13} stroke={2.2} />
        </button>
      </div>
      {#if app.draft.kind === "transfer"}
        <button type="button" class="press flex h-7 max-w-[200px] items-center gap-1 rounded-full bg-accent-soft px-2.5 text-[13px] font-medium text-accent-ink" aria-haspopup="menu" onclick={() => (menu = menu === "to" ? "" : "to")}>
          <Ic name="arrow" size={13} stroke={2.2} /><span class="truncate">{toAcct?.name ?? "To account"}</span>
        </button>
      {/if}
    </div>
  </div>

  {#if menu}
    <button type="button" aria-label="Close menu" class="fixed inset-0 z-20 cursor-default" onclick={() => (menu = "")}></button>
    <div role="menu" class="pop absolute top-[88px] right-0 z-30 max-h-72 w-60 overflow-y-auto rounded-[14px] bg-card py-1 shadow-[0_10px_40px_rgba(0,0,0,0.2)] ring-1 ring-sep">
      <p class="m-0 px-4 pt-2 pb-1 text-[12px] text-muted">{menu === "to" ? "Move to" : "From account"}</p>
      {#each accounts().filter((a) => menu !== "to" || a.id !== acct?.id) as a (a.id)}
        <button type="button" role="menuitem" class="hl flex h-11 w-full items-center justify-between gap-2 px-4 text-left text-[15px]" onclick={() => pickAccount(a.id)}>
          <span class="truncate">{a.name}</span><span class="num text-[13px] text-muted">{money(a.balance, a.currency)}</span>
        </button>
      {/each}
      <button type="button" role="menuitem" class="hl flex h-11 w-full items-center gap-2 px-4 text-left text-[15px] text-accent" onclick={() => ((menu = ""), openSheet({ kind: "account" }))}><Ic name="plus" size={16} />New account</button>
    </div>
  {/if}

  {#if app.draft.kind !== "transfer"}
    <CategoryRow {cats} value={app.draft.category} {splitIds} onpick={pickCat} onnew={() => openSheet({ kind: "category", type: app.draft.kind === "income" ? "income" : "expense", pick: true })} />
  {/if}

  <!-- Note + extras -->
  <div class="flex items-center gap-1.5">
    <label class="flex h-10 min-w-0 flex-1 items-center rounded-[12px] bg-fill2 px-3">
      <span class="sr">Note</span>
      <input bind:value={app.draft.note} placeholder={app.draft.kind === "transfer" ? "Note" : "Note, #tag"} enterkeyhint="done" class="min-w-0 flex-1 bg-transparent text-[15px] outline-none"
        onfocus={() => (noteFocus = true)} onblur={() => setTimeout(() => (noteFocus = false), 120)} onkeydown={(e) => e.key === "Enter" && (e.currentTarget as HTMLInputElement).blur()} />
    </label>
    <label class="press relative flex h-10 shrink-0 items-center gap-1 rounded-[12px] bg-fill2 px-2.5 text-[13px] font-medium {app.draft.date && app.draft.date !== todayIso() ? 'text-accent-ink' : 'text-ink2'}">
      <Ic name="calendar" size={16} />{!app.draft.date || app.draft.date === todayIso() ? "Today" : dayLabel(app.draft.date).replace(/^\w+, /, "") || shortDate(app.draft.date)}
      <input type="date" aria-label="Date" bind:value={app.draft.date} max="2100-12-31" class="absolute inset-0 opacity-0" />
    </label>
    <button type="button" aria-label="Add photo" class="press relative flex h-10 w-10 shrink-0 items-center justify-center rounded-[12px] bg-fill2 {app.draft.photos.length ? 'text-accent-ink' : 'text-ink2'}" onclick={photo}>
      <Ic name="camera" size={18} />{#if app.draft.photos.length}<span class="absolute -top-1 -right-1 flex h-4 min-w-4 items-center justify-center rounded-full bg-accent px-1 text-[10px] font-bold text-on-accent">{app.draft.photos.length}</span>{/if}
    </button>
    {#if app.draft.kind === "expense"}
      <button type="button" aria-label="Split across categories" class="press flex h-10 shrink-0 items-center rounded-[12px] bg-fill2 px-2.5 text-[13px] font-medium {app.draft.splits.length > 1 ? 'text-accent-ink' : 'text-ink2'}" onclick={() => openSheet({ kind: "split" })}>Split</button>
    {/if}
  </div>
  {#if app.draft.photos.length || app.draft.splits.length > 1}
    <p class="m-0 -mt-1 px-1 text-[12px] text-muted">
      {#if app.draft.splits.length > 1}Split: {app.draft.splits.map((s) => `${catById(s.category)?.name ?? "?"} ${s.amount}`).join(" + ")}{/if}
      {#if app.draft.photos.length}<button type="button" class="ml-1 text-danger" onclick={() => (app.draft.photos = [])}>Remove photo{app.draft.photos.length > 1 ? "s" : ""}</button>{/if}
    </p>
  {/if}

  {#if !noteFocus || compact}
    <NumPad onkey={key} {compact} />
  {/if}
  <button type="button" disabled={!canSave} onclick={save}
    class="press h-12 w-full rounded-[14px] bg-accent text-[17px] font-semibold text-on-accent transition-opacity disabled:opacity-40">
    {app.draft.kind === "transfer" ? "Move money" : "Save"}
  </button>
</div>

<style>
  @keyframes -global-shake { 25% { transform: translateX(-6px); } 75% { transform: translateX(6px); } }
</style>
