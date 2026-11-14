<script lang="ts">
  // Dusk main window. Phone (< 900 px): top bar, one page, bottom tabs
  // Night / Trends / Connections; settings and Adjust are full pages with a
  // back arrow. Desktop: left nav (Ctrl+1/2/3, Ctrl+,), page, and Adjust as a
  // side panel. Android Back closes the top layer and never loses input.
  import Adjust from "./components/Adjust.svelte";
  import ConnectionsPage from "./components/ConnectionsPage.svelte";
  import Icon from "./components/Icon.svelte";
  import NightPage from "./components/NightPage.svelte";
  import SettingsPage from "./components/SettingsPage.svelte";
  import Toast from "./components/Toast.svelte";
  import TrendsPage from "./components/TrendsPage.svelte";
  import { api } from "./lib/api";
  import { addDays } from "./lib/fmt";
  import { app, bridge, currentKey, init, openTab, rate, show, shownKey, step, type Tab } from "./lib/state.svelte";
  import { exit, onBackButtonPress } from "@tauri-apps/api/app";

  init();

  let width = $state(window.innerWidth);
  const desktop = $derived(width >= 900);
  let vh = $state(window.visualViewport?.height ?? window.innerHeight);
  window.visualViewport?.addEventListener("resize", () => {
    vh = window.visualViewport!.height;
    window.scrollTo(0, 0);
  });
  let scroller: HTMLElement | undefined = $state();
  $effect(() => {
    void app.tab;
    scroller?.scrollTo(0, 0);
  });

  const TABS: { id: Tab; icon: string; name: string; key: string }[] = [
    { id: "night", icon: "moon", name: "Night", key: "Ctrl 1" },
    { id: "trends", icon: "trend", name: "Trends", key: "Ctrl 2" },
    { id: "connections", icon: "link", name: "Connections", key: "Ctrl 3" },
  ];
  const closeEdit = () => (app.edit = null);

  // Android Back (Tauri's back-button hook, no history tricks): the open form
  // may ask first ("Discard your changes?"), then the top layer closes:
  // Adjust → Settings → Trends/Connections → an older night. At the bottom the app exits.
  function back(): boolean {
    if (app.closeGuard?.()) return true;
    if (app.edit) app.edit = null;
    else if (app.tab === "settings") app.tab = app.prevTab;
    else if (app.tab !== "night") app.tab = "night";
    else if (app.shown) show(currentKey());
    else return false;
    return true;
  }
  if (bridge()) onBackButtonPress(() => void (back() || exit(0))).catch((e) => console.warn("back button", e));

  function keys(e: KeyboardEvent) {
    const tag = (e.target as HTMLElement)?.tagName;
    if (e.ctrlKey && !e.altKey) {
      const t = ({ "1": "night", "2": "trends", "3": "connections", ",": "settings" } as Record<string, Tab>)[e.key];
      if (t) {
        e.preventDefault();
        if (!app.edit || !app.closeGuard?.()) (app.edit = null), openTab(t);
      }
      return;
    }
    if (["INPUT", "TEXTAREA", "SELECT"].includes(tag) || e.altKey || e.metaKey || app.edit) return;
    if (app.tab !== "night") return;
    if (/^[1-5]$/.test(e.key)) rate(shownKey(), Number(e.key));
    else if (e.key === "ArrowLeft") step(-1);
    else if (e.key === "ArrowRight") step(1);
    else if (e.key.toLowerCase() === "a") (e.preventDefault(), (app.edit = { kind: "adjust", night: shownKey() }));
    else if (e.key.toLowerCase() === "n") (e.preventDefault(), (app.edit = { kind: "nap", night: shownKey() }));
  }

  // Test hook (DUSK_DEMO env, desktop) for screenshots: tab=, back=<nights>, edit=adjust|nap, focus=, details=<id>.
  api.devHooks().then(async (h) => {
    if (!h) return;
    const p = Object.fromEntries(new URLSearchParams(h));
    app.demo = p;
    for (let i = 0; i < 50 && !app.dash; i++) await new Promise((r) => setTimeout(r, 100));
    if (p.back) await show(addDays(currentKey(), -Number(p.back)));
    if (p.tab) openTab(p.tab as Tab);
    if (p.edit) app.edit = { kind: p.edit as "adjust" | "nap", night: shownKey(), focus: (p.focus as "bed") || undefined };
  });
</script>

<svelte:window bind:innerWidth={width} onkeydown={keys} />

{#snippet page()}
  {#if !app.dash}<p class="pt-24 text-center text-muted">{app.error || "Loading…"}</p>
  {:else if app.tab === "night"}<NightPage {desktop} />
  {:else if app.tab === "trends"}<TrendsPage {desktop} />
  {:else if app.tab === "connections"}<ConnectionsPage {desktop} />
  {:else}<SettingsPage {desktop} />{/if}
{/snippet}

{#if desktop}
  <div class="flex h-screen overflow-hidden">
    <nav class="flex w-56 shrink-0 flex-col gap-1 border-r border-sep px-3.5 py-[22px]" aria-label="Main">
      <div class="flex items-center gap-2 px-2.5 pt-0.5 pb-[18px]"><span class="flex text-accent"><Icon name="moon" size={22} stroke={2.2} /></span><span class="text-[18px] font-bold tracking-[-0.02em]">Dusk</span></div>
      {#each TABS as t}
        <button type="button" aria-current={app.tab === t.id ? "page" : undefined} onclick={() => ((app.edit = null), openTab(t.id))}
          class="flex h-10 items-center gap-2.5 rounded-[10px] px-3 text-left text-[14.5px] {app.tab === t.id ? 'bg-accent-soft font-semibold text-accent' : 'font-medium text-ink2 hover:bg-fill2'}">
          <Icon name={t.icon} size={19} stroke={2} />{t.name}<span class="ml-auto text-[11px] font-medium text-muted">{t.key}</span></button>
      {/each}
      <span class="flex-1"></span>
      {#if app.tab === "night"}
        <div class="px-3 pb-3 text-[12px] leading-[1.7] text-muted"><b class="font-semibold text-ink2">1–5</b> rate · <b class="font-semibold text-ink2">← →</b> other nights<br /><b class="font-semibold text-ink2">A</b> adjust · <b class="font-semibold text-ink2">N</b> add nap</div>
      {/if}
      <button type="button" aria-current={app.tab === "settings" ? "page" : undefined} onclick={() => ((app.edit = null), openTab("settings"))}
        class="flex h-10 items-center gap-2.5 rounded-[10px] px-3 text-left text-[14.5px] {app.tab === 'settings' ? 'bg-accent-soft font-semibold text-accent' : 'font-medium text-ink2 hover:bg-fill2'}">
        <Icon name="gear" size={19} stroke={2} />Settings<span class="ml-auto text-[11px] font-medium text-muted">Ctrl ,</span></button>
    </nav>
    <main bind:this={scroller} class="min-w-0 flex-1 overflow-y-auto px-9 pt-7">
      {#if app.error && app.dash}<p role="alert" class="card mb-4 px-4 py-3 text-[14px] text-danger">{app.error}</p>{/if}
      {@render page()}
    </main>
    {#if app.edit}
      <aside class="fade flex w-[380px] shrink-0 flex-col border-l border-sep bg-bg">
        {#key app.edit.kind + app.edit.night + (app.edit.focus ?? "")}<Adjust edit={app.edit} {desktop} onclose={closeEdit} />{/key}
      </aside>
    {/if}
    <div class="pointer-events-none fixed bottom-6 left-1/2 z-50 w-[420px] -translate-x-1/2"><Toast /></div>
  </div>
{:else}
  <div class="relative flex flex-col overflow-hidden pt-[env(safe-area-inset-top)]" style="height:{vh}px">
    {#if app.edit}
      <div class="rise flex min-h-0 flex-1 flex-col">
        {#key app.edit.kind + app.edit.night}<Adjust edit={app.edit} {desktop} onclose={closeEdit} />{/key}
      </div>
    {:else}
      <header class="flex h-14 shrink-0 items-center {app.tab === 'settings' ? 'pl-1' : 'pr-1.5 pl-5'}">
        {#if app.tab === "settings"}
          <button type="button" class="press grid h-11 w-11 place-items-center rounded-[12px] text-ink2" aria-label="Back" onclick={() => (app.tab = app.prevTab)}><Icon name="back" /></button>
          <h1 class="m-0 ml-1 text-[18px] font-bold tracking-[-0.02em]">Settings</h1>
        {:else}
          {#if app.tab === "night"}
            <span class="flex text-accent"><Icon name="moon" size={20} stroke={2.2} /></span><span class="ml-2 text-[18px] font-bold tracking-[-0.02em]">Dusk</span>
          {:else}
            <h1 class="m-0 text-[22px] font-bold tracking-[-0.025em]">{app.tab === "trends" ? "Trends" : "Connections"}</h1>
          {/if}
          <span class="flex-1"></span>
          <button type="button" class="press grid h-11 w-11 place-items-center rounded-[12px] text-ink2" aria-label="Settings" onclick={() => openTab("settings")}><Icon name="gear" /></button>
        {/if}
      </header>
      <div bind:this={scroller} class="min-h-0 flex-1 overflow-y-auto px-4 {app.tab === 'settings' ? 'pb-6' : 'pb-24'}">
        {#if app.error && app.dash}<p role="alert" class="card mb-4 px-4 py-3 text-[14px] text-danger">{app.error}</p>{/if}
        {@render page()}
      </div>
      {#if app.tab !== "settings"}
        <nav class="absolute inset-x-0 bottom-0 flex border-t border-sep bg-bg px-3 pt-2 pb-[max(14px,env(safe-area-inset-bottom))]" aria-label="Main">
          {#each TABS as t}
            <button type="button" aria-current={app.tab === t.id ? "page" : undefined} onclick={() => openTab(t.id)}
              class="flex h-12 flex-1 flex-col items-center justify-center gap-1 text-[12px] font-semibold {app.tab === t.id ? 'text-accent' : 'text-muted'}">
              <span class="flex rounded-full {app.tab === t.id ? 'bg-accent-soft px-[18px] py-[3px]' : 'py-[3px]'}"><Icon name={t.icon} size={22} stroke={app.tab === t.id ? 2.2 : 1.8} /></span>{t.name}</button>
          {/each}
        </nav>
      {/if}
    {/if}
  </div>
  <!-- Above the tab bar; at the top while Adjust is open, so it never covers the number pad. -->
  <div class="pointer-events-none fixed inset-x-3 z-[60] {app.edit ? 'top-[calc(env(safe-area-inset-top)+64px)]' : 'bottom-[max(84px,calc(env(safe-area-inset-bottom)+76px))]'}"><Toast /></div>
{/if}
