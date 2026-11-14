<script lang="ts">
  // Hisab main window. Phone (< 900 px): opens to the add screen; other pages
  // open over it with a back arrow; sheets slide up. Desktop: sidebar + page.
  import AccountSheet from "./components/AccountSheet.svelte";
  import Accounts from "./components/Accounts.svelte";
  import AddPanel from "./components/AddPanel.svelte";
  import Categories from "./components/Categories.svelte";
  import CategorySheet from "./components/CategorySheet.svelte";
  import CurrencySheet from "./components/CurrencySheet.svelte";
  import ExportSheet from "./components/ExportSheet.svelte";
  import GoalSheet from "./components/GoalSheet.svelte";
  import Goals from "./components/Goals.svelte";
  import History from "./components/History.svelte";
  import Home from "./components/Home.svelte";
  import Ic from "./components/Ic.svelte";
  import Insights from "./components/Insights.svelte";
  import LoanSheet from "./components/LoanSheet.svelte";
  import Loans from "./components/Loans.svelte";
  import More from "./components/More.svelte";
  import PhotoViewer from "./components/PhotoViewer.svelte";
  import { PhotoSourceSheet, closePhotoSource } from "./lib/photo-source";
  import RecurringList from "./components/RecurringList.svelte";
  import RecurringSheet from "./components/RecurringSheet.svelte";
  import Settings from "./components/Settings.svelte";
  import SmsReview from "./components/SmsReview.svelte";
  import SplitSheet from "./components/SplitSheet.svelte";
  import Toast from "./components/Toast.svelte";
  import TxSheet from "./components/TxSheet.svelte";
  import { api } from "./lib/api";
  import { onBackButtonPress } from "@tauri-apps/api/app";
  import { minimize, takeLaunchAction } from "./lib/native";
  import { maybeFetchRates } from "./lib/rates";
  import { syncSms } from "./lib/sms";
  import { app, back, closeSheet, go, historyPreset, init, openSheet, page, type Page } from "./lib/state.svelte";

  init();

  let width = $state(window.innerWidth);
  const desktop = $derived(width >= 900);
  let vh = $state(window.visualViewport?.height ?? window.innerHeight);
  window.visualViewport?.addEventListener("resize", () => {
    vh = window.visualViewport!.height;
    window.scrollTo(0, 0);
  });
  let panel: AddPanel | undefined = $state();

  const TITLES: Record<Page, string> = {
    home: "Hisab", history: "History", insights: "Insights", accounts: "Accounts", loans: "Loans", goals: "Savings goals", recurring: "Recurring",
    categories: "Categories", sms: "SMS imports", settings: "Settings", more: "More",
  };
  const NAV: { id: Page; icon: string; key: string }[] = [
    { id: "home", icon: "plus", key: "N" }, { id: "history", icon: "list", key: "H" }, { id: "insights", icon: "chart", key: "I" }, { id: "accounts", icon: "wallet", key: "A" },
    { id: "loans", icon: "people", key: "L" }, { id: "goals", icon: "target", key: "G" }, { id: "recurring", icon: "repeat", key: "R" }, { id: "categories", icon: "tag", key: "C" },
    { id: "sms", icon: "sms", key: "" },
  ];

  // After start: rates (if stale), SMS (Android, if on), widget launch.
  $effect(() => {
    if (!app.ready) return;
    maybeFetchRates();
    syncSms();
    if (takeLaunchAction() === "add") go("home");
  });
  document.addEventListener("visibilitychange", () => {
    if (document.hidden || !app.ready) return;
    syncSms();
    if (takeLaunchAction() === "add") {
      app.sheets = [];
      go("home");
    }
  });

  // Android back (Tauri's back-button hook; webview history is not used, since
  // Chrome skips history entries added without a tap). Back asks an open form
  // first (unsaved input → "Discard?"), then closes the topmost sheet, then
  // the page. On the add screen it sends the app to the background; the typed
  // draft is saved and comes back next time.
  const FORMS = ["tx", "account", "category", "loan", "goal", "recurring"];
  // Only the form on top may stop Back/Escape (not a picker opened over it).
  const guarded = () => (!app.sheets.length || FORMS.includes(app.sheets[app.sheets.length - 1].kind)) && !!app.guard?.();
  function onBack() {
    if (closePhotoSource()) return; // "Add photo" camera/gallery sheet
    if (guarded()) return;
    if (app.sheets.length) closeSheet();
    else if (app.pages.length > 1) back();
    else minimize();
  }
  onBackButtonPress(() => onBack()).catch(() => {});

  // Test hook (HISAB_DEMO env, desktop only) for screenshots and automation.
  api.devHooks().then(async (h) => {
    if (!h) return;
    const p = Object.fromEntries(new URLSearchParams(h));
    app.demo = p;
    for (let i = 0; i < 50 && !app.ready; i++) await new Promise((r) => setTimeout(r, 100));
    if (p.page) for (const x of p.page.split(",")) go(x as Page);
    if (p.amount) app.draft.amount = p.amount;
    if (p.cat) app.draft.category = app.snap?.categories.find((c) => c.name === p.cat)?.id ?? "";
    if (p.kind) app.draft.kind = p.kind as "expense";
    if (p.sheet === "loan") openSheet({ kind: "loan", id: app.snap?.loans[Number(p.i ?? 0)]?.id });
    if (p.sheet === "newloan") openSheet({ kind: "loan" });
    if (p.sheet === "tx") openSheet({ kind: "tx", id: app.snap?.recent[Number(p.i ?? 0)]?.id ?? "" });
    if (p.sheet === "split") openSheet({ kind: "split" });
    if (p.sheet === "category") openSheet({ kind: "category", id: app.snap?.categories[Number(p.i ?? 0)]?.id });
    if (p.sheet === "export") openSheet({ kind: "export" });
    if (p.sheet === "goal") openSheet({ kind: "goal", id: app.snap?.goals[0]?.id });
    if (p.sheet === "recurring") openSheet({ kind: "recurring", id: app.snap?.recurring[0]?.id });
    if (p.sheet === "account") openSheet({ kind: "account", id: app.snap?.accounts[0]?.id });
    if (p.save) setTimeout(() => panel?.save(), 300);
  });

  function keys(e: KeyboardEvent) {
    const tag = (e.target as HTMLElement)?.tagName;
    const typing = ["INPUT", "TEXTAREA", "SELECT"].includes(tag);
    if (e.key === "Escape") {
      if (closePhotoSource()) return;
      if (guarded()) return;
      if (app.sheets.length) closeSheet();
      else if (typing) (e.target as HTMLElement).blur();
      else if (app.pages.length > 1) back();
      return;
    }
    if (typing || app.sheets.length || e.ctrlKey || e.metaKey || e.altKey) return;
    if (page() === "home") {
      if (/^[0-9]$/.test(e.key) || e.key === "." || e.key === ",") return void panel?.key(e.key === "," ? "." : e.key);
      if (e.key === "Backspace") return void panel?.key("back");
      if (e.key === "Enter") return void panel?.save();
      if (e.key === "Delete") return void panel?.key("clear");
    }
    if (!desktop) return;
    const n = NAV.find((x) => x.key && x.key.toLowerCase() === e.key.toLowerCase());
    if (n) {
      e.preventDefault();
      go(n.id);
    } else if (e.key === "/") {
      e.preventDefault();
      go("history");
      setTimeout(() => document.querySelector<HTMLInputElement>("input[placeholder^='Search']")?.focus(), 50);
    } else if (e.key === ",") go("settings");
  }
  const hist = $derived({ ...historyPreset.v, k: historyPreset.n });
</script>

<svelte:window bind:innerWidth={width} onkeydown={keys} />

{#snippet pageBody(p: Page)}
  {#if p === "history"}{#key hist.k}<History initial={historyPreset.v} />{/key}
  {:else if p === "insights"}<Insights {desktop} />
  {:else if p === "accounts"}<Accounts {desktop} />
  {:else if p === "loans"}<Loans {desktop} />
  {:else if p === "goals"}<Goals {desktop} />
  {:else if p === "recurring"}<RecurringList {desktop} />
  {:else if p === "categories"}<Categories {desktop} />
  {:else if p === "sms"}<SmsReview {desktop} />
  {:else if p === "settings"}<Settings {desktop} />
  {:else if p === "more"}<More />{/if}
{/snippet}

{#if !app.ready}
  <div class="flex h-screen items-center justify-center text-[15px] text-muted">{app.error || ""}</div>
{:else if desktop}
  <div class="flex h-screen overflow-hidden">
    <nav class="flex w-[230px] shrink-0 flex-col gap-0.5 bg-fill2 px-3 py-[18px]" aria-label="Sections">
      <div class="flex items-center gap-2.5 px-2.5 pb-[18px]">
        <span class="h-7 w-7 rounded-[8px] bg-gradient-to-br from-[#67a06b] to-[#3f6d45]"></span><b class="text-[17px]">Hisab</b>
      </div>
      {#each NAV as n}
        <button type="button" class="press flex h-9 items-center gap-2.5 rounded-[9px] px-2.5 text-left text-[14px] {page() === n.id ? 'card font-semibold text-ink' : 'font-medium text-ink2'}" onclick={() => go(n.id)} aria-current={page() === n.id ? "page" : undefined}>
          <Ic name={n.icon} size={18} />{n.id === "home" ? "Add" : TITLES[n.id]}
          {#if n.id === "sms" && app.snap?.sms_pending}<span class="ml-auto rounded-full bg-accent px-1.5 text-[11px] font-bold text-on-accent">{app.snap.sms_pending}</span>{:else}<span class="ml-auto text-[11px] font-medium text-muted">{n.key}</span>{/if}
        </button>
      {/each}
      <button type="button" class="press flex h-9 items-center gap-2.5 rounded-[9px] px-2.5 text-left text-[14px] font-medium text-ink2" onclick={() => openSheet({ kind: "export" })}><Ic name="export" size={18} />Export</button>
      <div class="flex-1"></div>
      <button type="button" class="press flex h-9 items-center gap-2.5 rounded-[9px] px-2.5 text-left text-[14px] {page() === 'settings' ? 'card font-semibold' : 'font-medium text-ink2'}" onclick={() => go("settings")}><Ic name="gear" size={18} />Settings</button>
    </nav>
    <main class="min-w-0 flex-1 overflow-hidden">
      {#if page() === "home"}
        <Home desktop bind:panel />
      {:else}
        <div class="h-full overflow-y-auto px-8 pt-6 pb-10">
          <h1 class="m-0 mb-4 text-[26px] font-bold tracking-[-0.5px]">{TITLES[page()]}</h1>
          <div class="max-w-[1100px]">{@render pageBody(page())}</div>
        </div>
      {/if}
    </main>
  </div>
{:else}
  <div class="relative flex flex-col overflow-hidden pt-[env(safe-area-inset-top)]" style="height:{vh}px">
    {#if page() === "home"}
      <header class="flex h-12 shrink-0 items-center gap-1 px-4">
        <h1 class="m-0 flex-1 text-[22px] font-bold tracking-[-0.5px]">Hisab</h1>
        <button type="button" aria-label="History" class="press flex h-10 w-10 items-center justify-center rounded-full text-ink2" onclick={() => go("history")}><Ic name="list" size={22} /></button>
        <button type="button" aria-label="Insights" class="press flex h-10 w-10 items-center justify-center rounded-full text-ink2" onclick={() => go("insights")}><Ic name="chart" size={22} /></button>
        <button type="button" aria-label="Accounts" class="press flex h-10 w-10 items-center justify-center rounded-full text-ink2" onclick={() => go("accounts")}><Ic name="wallet" size={22} /></button>
        <button type="button" aria-label="More" class="press relative flex h-10 w-10 items-center justify-center rounded-full text-ink2" onclick={() => go("more")}><Ic name="more" size={22} stroke={2.6} />
          {#if app.snap?.sms_pending || app.snap?.loans.some((l) => l.status === "overdue")}<span class="absolute top-2 right-2 h-2 w-2 rounded-full bg-accent"></span>{/if}</button>
      </header>
      <div class="min-h-0 flex-1"><Home bind:panel /></div>
    {:else}
      {#key page()}
        <div class="slide flex min-h-0 flex-1 flex-col">
          <header class="flex h-14 shrink-0 items-center gap-1 px-2">
            <button type="button" aria-label="Back" class="press flex h-10 w-10 items-center justify-center rounded-full" onclick={back}><Ic name="back" size={24} stroke={2.1} /></button>
            <h1 class="m-0 flex-1 truncate text-[22px] font-bold tracking-[-0.5px]">{TITLES[page()]}</h1>
          </header>
          <div class="min-h-0 flex-1 overflow-y-auto px-4 pt-1 pb-[calc(env(safe-area-inset-bottom)+48px)]">{@render pageBody(page())}</div>
        </div>
      {/key}
    {/if}
  </div>
{/if}

{#each app.sheets as s, i (i)}
  {#if s.kind === "tx"}<TxSheet id={s.id} {desktop} />
  {:else if s.kind === "account"}<AccountSheet id={s.id} {desktop} />
  {:else if s.kind === "category"}<CategorySheet id={s.id} type={s.type} pick={s.pick} {desktop} />
  {:else if s.kind === "loan"}<LoanSheet id={s.id} edit={s.edit} {desktop} />
  {:else if s.kind === "goal"}<GoalSheet id={s.id} {desktop} />
  {:else if s.kind === "recurring"}<RecurringSheet id={s.id} {desktop} />
  {:else if s.kind === "export"}<ExportSheet {desktop} />
  {:else if s.kind === "currency"}<CurrencySheet value={s.value} onpick={s.onpick} {desktop} />
  {:else if s.kind === "split"}<SplitSheet {desktop} />
  {:else if s.kind === "photo"}<PhotoViewer id={s.id} />{/if}
{/each}

<!-- Phone: below the top bar, so a tap on History/Insights/Accounts/More right after Save
     can't hit "Undo" by accident (QA 2026-11-15). -->
<PhotoSourceSheet privateNote="Stays private in Hisab, not in your gallery" />
<div class="pointer-events-none fixed z-[90] {desktop ? 'bottom-6 left-1/2 w-[420px] -translate-x-1/2' : 'inset-x-3 top-[calc(env(safe-area-inset-top)+64px)]'}"><Toast /></div>
