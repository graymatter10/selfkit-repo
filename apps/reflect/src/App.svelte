<script lang="ts">
  import LockGate from "../../../ui/src/lock/LockGate.svelte"; // App lock (docs/components/app-lock.md)
  // Reflect main window. Phone (< 900 px): top bar with Health and Settings,
  // one page, bottom tabs Today / Streaks / Connections / Review (Week|Month).
  // Desktop: left nav (Ctrl+1–5, Ctrl+, settings), Health at the bottom.
  // Opens on Today; everything else is one tap away.
  import ConnectionsPage from "./components/ConnectionsPage.svelte";
  import HealthPage from "./components/HealthPage.svelte";
  import Icon from "./components/Icon.svelte";
  import ReviewPage from "./components/ReviewPage.svelte";
  import SettingsPage from "./components/SettingsPage.svelte";
  import SyncSettings from "../../../ui/src/sync/SyncSettings.svelte";
  import Conflicts from "../../../ui/src/sync/Conflicts.svelte";
  import StreaksPage from "./components/StreaksPage.svelte";
  import Toast from "./components/Toast.svelte";
  import TodayPage from "./components/TodayPage.svelte";
  import { api } from "./lib/api";
  import { app, bridge, healthLevel, init, load, openTab, type Tab } from "./lib/state.svelte";
  import { exit, onBackButtonPress } from "@tauri-apps/api/app";

  init();
  api.sources().then((s) => (app.sources = s)).catch(() => {});

  let width = $state(window.innerWidth);
  const desktop = $derived(width >= 900);
  let scroller: HTMLElement | undefined = $state();
  $effect(() => {
    void app.tab;
    void app.reviewKind;
    scroller?.scrollTo(0, 0);
  });
  const lvl = $derived(healthLevel(app.health));
  const dot = $derived(lvl === "error" ? "bg-danger" : lvl === "warn" ? "bg-accent" : "");

  const TABS: { id: Tab; icon: string; name: string }[] = [
    { id: "today", icon: "today", name: "Today" },
    { id: "streaks", icon: "streak", name: "Streaks" },
    { id: "connections", icon: "link", name: "Connections" },
    { id: "week", icon: "review", name: "Review" },
  ];
  const NAV: { id: Tab; icon: string; name: string; key: string }[] = [
    { id: "today", icon: "today", name: "Today", key: "Ctrl 1" },
    { id: "streaks", icon: "streak", name: "Streaks", key: "Ctrl 2" },
    { id: "connections", icon: "link", name: "Connections", key: "Ctrl 3" },
    { id: "week", icon: "review", name: "Weekly review", key: "Ctrl 4" },
    { id: "month", icon: "month", name: "Monthly review", key: "Ctrl 5" },
  ];
  const TITLES: Record<Tab, string> = { today: "Today", streaks: "Streaks", connections: "Connections", week: "Review", month: "Review", health: "Health", settings: "Settings", sync: "Sync", conflicts: "Changed on two devices" };
  const isTab = (t: Tab) => (t === "week" ? app.tab === "week" || app.tab === "month" : app.tab === t);
  const sub = $derived(app.tab === "health" || app.tab === "settings" || app.tab === "sync" || app.tab === "conflicts");

  // Android Back: Health/Settings → where you came from → Today → leave.
  function back(): boolean {
    if (app.adding) app.adding = false;
    else if (app.tab === "conflicts") openTab("sync");
    else if (app.tab === "sync") openTab("settings");
    else if (sub) openTab(app.prevTab);
    else if (app.tab !== "today") openTab("today");
    else return false;
    return true;
  }
  if (bridge()) onBackButtonPress(() => void (back() || exit(0))).catch((e) => console.warn("back button", e));

  function keys(e: KeyboardEvent) {
    if (!e.ctrlKey || e.altKey) {
      if (e.key === "Escape" && sub && !desktop) back();
      return;
    }
    const t = ({ "1": "today", "2": "streaks", "3": "connections", "4": "week", "5": "month", ",": "settings" } as Record<string, Tab>)[e.key];
    if (t) {
      e.preventDefault();
      openTab(t);
    } else if (e.key.toLowerCase() === "r") {
      e.preventDefault();
      load();
    }
  }

  // Test hook (REFLECT_DEMO env, desktop): tab=, offset=, open=<connection id>, adding=1, scroll=<px>.
  api.devHooks().then(async (h) => {
    if (!h) return;
    const p = Object.fromEntries(new URLSearchParams(h));
    app.demo = p;
    for (let i = 0; i < 150 && !app.overview; i++) await new Promise((r) => setTimeout(r, 100));
    if (p.offset && (p.tab === "week" || p.tab === "month")) app.offset[p.tab] = Number(p.offset);
    if (p.tab) openTab(p.tab as Tab);
    if (p.open) app.open[p.open] = true;
    if (p.adding) app.adding = true;
    if (p.scroll) setTimeout(() => scroller?.scrollTo(0, Number(p.scroll)), 600);
  });
</script>

<svelte:window bind:innerWidth={width} onkeydown={keys} />

{#snippet page()}
  {#if app.tab === "today"}
    {#if app.overview}<TodayPage {desktop} />{:else}<p class="pt-24 text-center text-muted">{app.error || "Loading…"}</p>{/if}
  {:else if app.tab === "streaks"}<StreaksPage {desktop} />
  {:else if app.tab === "connections"}<ConnectionsPage {desktop} />
  {:else if app.tab === "week" || app.tab === "month"}<ReviewPage {desktop} kind={app.tab} />
  {:else if app.tab === "health"}<HealthPage {desktop} />
  {:else if app.tab === "sync"}
    {#if desktop}<div class="mb-5 flex items-center gap-2"><button type="button" class="iconbtn press" aria-label="Back" onclick={() => openTab("settings")}><Icon name="back" size={20} /></button><h1 class="dtitle !m-0">Sync</h1></div>{/if}
    <SyncSettings phone={!desktop} onconflicts={() => openTab("conflicts")} />
  {:else if app.tab === "conflicts"}<div class={desktop ? "card flex h-[calc(100vh-110px)] flex-col overflow-hidden !p-0" : "-mx-4 flex h-full min-h-0 flex-col"}><Conflicts phone={!desktop} onclose={() => openTab("sync")} names={{ streaks: "Reflect streak", reflect_settings: "Reflect settings" }} /></div>
  {:else}<SettingsPage {desktop} />{/if}
{/snippet}

{#if desktop}
  <div class="flex h-screen overflow-hidden">
    <nav class="flex w-[244px] shrink-0 flex-col gap-1 border-r border-sep px-3.5 py-5" aria-label="Main">
      <div class="flex items-center gap-2.5 px-2.5 pt-1 pb-[18px] text-[17px] font-bold tracking-[-0.02em]"><span class="grid h-7 w-7 place-items-center rounded-[8px] bg-accent-soft text-accent"><Icon name="bars" size={17} stroke={2.3} /></span>Reflect</div>
      {#each NAV as t}
        <button type="button" aria-current={app.tab === t.id ? "page" : undefined} onclick={() => openTab(t.id)}
          class="flex h-[42px] items-center gap-[11px] rounded-[10px] px-3 text-left text-[14.5px] {app.tab === t.id ? 'bg-accent-soft font-semibold text-accent' : 'font-medium text-ink2 hover:bg-fill2'}">
          <Icon name={t.icon} size={19} /><span class="whitespace-nowrap">{t.name}</span><span class="ml-auto whitespace-nowrap text-[12px] font-medium text-muted">{t.key}</span></button>
      {/each}
      <span class="flex-1"></span>
      <button type="button" aria-current={app.tab === "health" ? "page" : undefined} onclick={() => openTab("health")}
        class="flex h-[42px] items-center gap-[11px] rounded-[10px] px-3 text-left text-[14.5px] {app.tab === 'health' ? 'bg-accent-soft font-semibold text-accent' : 'font-medium text-ink2 hover:bg-fill2'}">
        <Icon name="pulse" size={19} />Health{#if dot}<span class="ml-auto h-2 w-2 rounded-full {dot}" aria-label={lvl === "error" ? "Needs a look" : "One thing to look at"}></span>{/if}</button>
      <button type="button" aria-current={app.tab === "settings" ? "page" : undefined} onclick={() => openTab("settings")}
        class="flex h-[42px] items-center gap-[11px] rounded-[10px] px-3 text-left text-[14.5px] {app.tab === 'settings' ? 'bg-accent-soft font-semibold text-accent' : 'font-medium text-ink2 hover:bg-fill2'}">
        <Icon name="sliders" size={19} />Settings<span class="ml-auto text-[12px] font-medium text-muted">Ctrl ,</span></button>
    </nav>
    <main bind:this={scroller} class="min-w-0 flex-1 overflow-y-auto px-10 pt-7 pb-10">
      {#if app.error && app.overview}<p role="alert" class="card mb-4 px-4 py-3 text-[14px] text-danger">{app.error}</p>{/if}
      {@render page()}
    </main>
    <div class="pointer-events-none fixed bottom-6 left-1/2 z-50 w-[420px] -translate-x-1/2"><Toast /></div>
  </div>
{:else}
  <div class="relative flex h-screen flex-col overflow-hidden pt-[env(safe-area-inset-top)]">
    <header class="flex h-14 shrink-0 items-center {sub ? 'pl-1' : 'pr-1.5 pl-5'}">
      {#if sub}
        <button type="button" class="iconbtn press" aria-label="Back" onclick={back}><Icon name="back" size={22} /></button>
        <h1 class="m-0 ml-1 text-[20px] font-bold tracking-[-0.02em]">{TITLES[app.tab]}</h1>
      {:else}
        <h1 class="m-0 flex-1 text-[22px] font-bold tracking-[-0.025em]">{TITLES[app.tab]}</h1>
        <button type="button" class="iconbtn press" aria-label="Health" onclick={() => openTab("health")}><Icon name="pulse" size={21} />{#if dot}<span class="absolute top-2.5 right-[9px] h-2 w-2 rounded-full {dot} shadow-[0_0_0_2px_var(--bg)]"></span>{/if}</button>
        <button type="button" class="iconbtn press" aria-label="Settings" onclick={() => openTab("settings")}><Icon name="sliders" size={21} /></button>
      {/if}
    </header>
    <div bind:this={scroller} class="min-h-0 flex-1 overflow-y-auto px-4 {sub ? 'pb-8' : 'pb-24'}">
      {#if app.error && app.overview}<p role="alert" class="card mb-4 px-4 py-3 text-[14px] text-danger">{app.error}</p>{/if}
      {@render page()}
    </div>
    {#if !sub}
      <nav class="absolute inset-x-0 bottom-0 flex border-t border-sep bg-bg px-2 pt-2 pb-[max(14px,env(safe-area-inset-bottom))]" aria-label="Main">
        {#each TABS as t}
          <button type="button" aria-current={isTab(t.id) ? "page" : undefined} onclick={() => openTab(t.id === "week" ? app.reviewKind : t.id)}
            class="flex h-12 flex-1 flex-col items-center justify-center gap-1 text-[12px] font-semibold {isTab(t.id) ? 'text-accent' : 'text-muted'}">
            <span class="flex rounded-full {isTab(t.id) ? 'bg-accent-soft px-4 py-[3px]' : 'py-[3px]'}"><Icon name={t.icon} size={22} stroke={isTab(t.id) ? 2.1 : 1.8} /></span>{t.name}</button>
        {/each}
      </nav>
    {/if}
  </div>
  <div class="pointer-events-none fixed inset-x-3 bottom-[max(84px,calc(env(safe-area-inset-bottom)+76px))] z-[60]"><Toast /></div>
{/if}
<!-- App lock: covers everything while locked (ui/src/lock, docs/components/app-lock.md). -->
<LockGate appName="Reflect" phone={!desktop} />
