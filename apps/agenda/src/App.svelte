<script lang="ts">
  // Agenda main window, iOS-minimal.
  // Phone (< 900 px): large title, search field, Active/All/Done, inset grouped
  // list, "New Task" at the bottom; forms and details slide up as sheets.
  // Desktop: sidebar + list + inspector.
  import { Icon } from "@selfkit/ui";
  import Seg from "./components/Seg.svelte";
  import SettingsSheet from "./components/SettingsSheet.svelte";
  import Sidebar from "./components/Sidebar.svelte";
  import TaskDetail from "./components/TaskDetail.svelte";
  import TaskForm from "./components/TaskForm.svelte";
  import TaskList from "./components/TaskList.svelte";
  import TaskRow from "./components/TaskRow.svelte";
  import Toast from "./components/Toast.svelte";
  import { api, type Task } from "./lib/api";
  import { app, groups, init, refresh, remove, taskById, toggle, counts } from "./lib/state.svelte";
  import type { Sort } from "./lib/time";

  init();

  let width = $state(window.innerWidth);
  const desktop = $derived(width >= 900);
  let vh = $state(window.visualViewport?.height ?? window.innerHeight);
  window.visualViewport?.addEventListener("resize", () => {
    vh = window.visualViewport!.height;
    window.scrollTo(0, 0);
  });
  let menu = $state(false);
  let searchEl: HTMLInputElement | undefined = $state();
  let searchFocus = $state(false);

  const panelTask = $derived("id" in app.panel ? taskById(app.panel.id) : undefined);
  const open = (t: Task) => (app.panel = { kind: "detail", id: t.id });
  const close = () => (app.panel = { kind: "none" });
  const newTask = () => (app.panel = { kind: "new" });
  const TITLES = { active: "Tasks", all: "All Tasks", done: "Completed", overdue: "Overdue" } as const;
  const SORTS: { id: Sort; name: string }[] = [{ id: "start", name: "Start Date" }, { id: "severity", name: "Severity" }, { id: "newest", name: "Newest First" }];

  // Android back: one history entry while anything is open. Back first asks the
  // open form (unsaved input → confirm), then closes the topmost layer.
  const layered = $derived(!desktop && (app.panel.kind !== "none" || app.settingsOpen || app.searching || menu));
  let pushed = false, skip = false;
  $effect(() => {
    if (layered && !pushed) {
      history.pushState({ agenda: 1 }, "");
      pushed = true;
    } else if (!layered && pushed) {
      pushed = false;
      skip = true;
      history.back();
    }
  });
  function onPop() {
    if (skip) return void (skip = false);
    pushed = false;
    if (app.closeGuard?.()) {
      history.pushState({ agenda: 1 }, "");
      pushed = true;
      return;
    }
    if (menu) menu = false;
    else if (app.settingsOpen) app.settingsOpen = false;
    else if (app.panel.kind === "edit" && panelTask) app.panel = { kind: "detail", id: panelTask.id };
    else if (app.panel.kind !== "none") close();
    else if (app.searching) stopSearch();
  }

  function stopSearch() {
    app.searching = false;
    app.query = "";
    app.results = [];
    searchEl?.blur();
  }
  let st: ReturnType<typeof setTimeout>;
  function onSearch() {
    app.searching = !!app.query.trim() || searchFocus;
    clearTimeout(st);
    st = setTimeout(refresh, 120);
  }

  // Test hook (AGENDA_DEMO env, desktop only) for screenshots.
  api.devHooks().then(async (h) => {
    if (!h) return;
    const p = Object.fromEntries(new URLSearchParams(h));
    app.demo = p;
    await new Promise((r) => setTimeout(r, 500));
    if (p.filter) app.filter = p.filter as typeof app.filter;
    if (p.panel === "new") newTask();
    if (p.detail) {
      const t = groups().flatMap((g) => g.tasks)[Number(p.detail)];
      if (t) app.panel = p.edit ? { kind: "edit", id: t.id } : { kind: "detail", id: t.id };
    }
    if (p.settings) app.settingsOpen = true;
    if (p.menu) menu = true;
    if (p.toast) {
      const t = groups().flatMap((g) => g.tasks)[0];
      if (t) toggle(t);
    }
  });

  function keys(e: KeyboardEvent) {
    const tag = (e.target as HTMLElement)?.tagName;
    if (["INPUT", "TEXTAREA", "SELECT"].includes(tag)) {
      if (e.key === "Escape" && app.searching) stopSearch();
      return;
    }
    if (app.settingsOpen || e.ctrlKey || e.metaKey || e.altKey) return;
    if (app.panel.kind === "new" || app.panel.kind === "edit") {
      if (e.key === "Escape" && !app.closeGuard?.()) close();
      return;
    }
    const list = app.searching ? app.results : groups().flatMap((g) => g.tasks);
    const cur = panelTask;
    const i = cur ? list.findIndex((t) => t.id === cur.id) : -1;
    switch (e.key.toLowerCase()) {
      case "n": e.preventDefault(); newTask(); break;
      case "/": e.preventDefault(); searchEl?.focus(); break;
      case "j": case "arrowdown": e.preventDefault(); if (list.length) open(list[Math.min(i + 1, list.length - 1)]); break;
      case "k": case "arrowup": e.preventDefault(); if (list.length) open(list[Math.max(i - 1, 0)]); break;
      case "x": if (cur) toggle(cur); break;
      case "e": if (cur) app.panel = { kind: "edit", id: cur.id }; break;
      case "delete": if (cur) remove(cur); break;
      case "escape": app.searching ? stopSearch() : close(); break;
    }
  }
</script>

<svelte:window bind:innerWidth={width} onkeydown={keys} onpopstate={onPop} />

{#snippet search()}
  <div class="flex items-center gap-2">
    <label class="flex h-9 flex-1 items-center gap-1.5 rounded-[10px] bg-fill px-2 text-muted">
      <Icon name="search" size={16} stroke={2.2} /><span class="sr">Search tasks</span>
      <input bind:this={searchEl} bind:value={app.query} oninput={onSearch} placeholder="Search" enterkeyhint="search"
        onfocus={() => ((searchFocus = true), (app.searching = true))} onblur={() => ((searchFocus = false), !app.query.trim() && (app.searching = false))}
        class="min-w-0 flex-1 bg-transparent text-[17px] text-ink outline-none" />
    </label>
    {#if app.searching}<button type="button" class="press fade text-[17px] text-accent" onmousedown={(e) => e.preventDefault()} onclick={stopSearch}>Cancel</button>{/if}
  </div>
{/snippet}

{#snippet moreMenu()}
  <div class="relative">
    <button type="button" aria-label="More" aria-haspopup="menu" aria-expanded={menu} class="press flex h-9 w-9 items-center justify-center rounded-full text-accent" onclick={() => (menu = !menu)}>
      <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" aria-hidden="true"><circle cx="12" cy="12" r="10" /><circle cx="7.5" cy="12" r="1.1" fill="currentColor" stroke="none" /><circle cx="12" cy="12" r="1.1" fill="currentColor" stroke="none" /><circle cx="16.5" cy="12" r="1.1" fill="currentColor" stroke="none" /></svg>
    </button>
    {#if menu}
      <button type="button" aria-label="Close menu" class="fixed inset-0 z-30 cursor-default" onclick={() => (menu = false)}></button>
      <div role="menu" class="pop absolute top-11 right-0 z-40 w-60 overflow-hidden rounded-[13px] bg-card/95 shadow-[0_10px_40px_rgba(0,0,0,0.18)] ring-1 ring-sep backdrop-blur-xl">
        <span class="block px-4 pt-2.5 pb-1 text-[13px] text-muted">Sort By</span>
        {#each SORTS as s}
          <button type="button" role="menuitemradio" aria-checked={app.sort === s.id} class="hl flex h-11 w-full items-center gap-2 pr-4 pl-4 text-left text-[17px]" onclick={() => ((app.sort = s.id), (menu = false))}>
            <span class="w-5 text-accent">{#if app.sort === s.id}<Icon name="check" size={16} stroke={2.6} />{/if}</span>{s.name}</button>
        {/each}
        <div class="h-2 bg-fill2"></div>
        <button type="button" role="menuitem" class="hl flex h-11 w-full items-center justify-between px-4 text-left text-[17px]" onclick={() => ((menu = false), (app.settingsOpen = true))}>Settings<Icon name="gear" size={18} stroke={1.9} /></button>
      </div>
    {/if}
  </div>
{/snippet}

{#snippet listArea()}
  {#if app.error}<p role="alert" class="m-0 mb-4 rounded-xl bg-card px-4 py-3 text-[15px] text-danger">{app.error}</p>{/if}
  {#if app.searching}
    {#if app.results.length}
      <div class="overflow-hidden rounded-xl bg-card">
        {#each app.results as t, i (t.id)}<TaskRow task={t} last={i === app.results.length - 1} {desktop} onopen={open} />{/each}
      </div>
    {:else if app.query.trim()}<p class="pt-16 text-center text-[17px] text-muted">No results for “{app.query}”</p>{/if}
  {:else}
    <TaskList {desktop} onopen={open} />
  {/if}
{/snippet}

{#if desktop}
  <div class="flex h-screen overflow-hidden">
    <Sidebar onsettings={() => (app.settingsOpen = true)} />
    <main class="flex min-w-0 flex-1 flex-col overflow-hidden">
      <div class="flex items-center gap-4 px-8 pt-6 pb-4">
        <h1 class="m-0 flex-1 text-[28px] font-bold tracking-[-0.5px]">{app.searching ? "Search" : TITLES[app.filter]}</h1>
        <div class="w-64">{@render search()}</div>
        <button type="button" aria-label="New task (N)" title="New task (N)" class="press flex h-9 w-9 items-center justify-center rounded-full text-accent" onclick={newTask}><Icon name="plus" size={24} stroke={2.2} /></button>
        {@render moreMenu()}
      </div>
      <div class="min-h-0 flex-1 overflow-y-auto px-8 pb-10"><div class="max-w-[760px]">{@render listArea()}</div></div>
    </main>
    {#if app.panel.kind !== "none"}
      <aside class="flex w-[400px] shrink-0 flex-col border-l border-sep bg-bg">
        {#if app.panel.kind === "new"}
          <TaskForm layout="panel" onclose={close} ondone={(t) => (app.panel = { kind: "detail", id: t.id })} />
        {:else if app.panel.kind === "edit" && panelTask}
          {#key panelTask.id}<TaskForm task={panelTask} layout="panel" onclose={() => panelTask && (app.panel = { kind: "detail", id: panelTask.id })} />{/key}
        {:else if panelTask}
          <TaskDetail task={panelTask} layout="panel" onclose={close} onedit={() => panelTask && (app.panel = { kind: "edit", id: panelTask.id })} />
        {/if}
      </aside>
    {/if}
    <div class="pointer-events-none fixed bottom-6 left-1/2 z-50 w-[400px] -translate-x-1/2"><Toast /></div>
  </div>
{:else}
  <div class="relative flex flex-col overflow-hidden pt-[env(safe-area-inset-top)]" style="height:{vh}px">
    <div class="flex flex-col gap-2.5 px-4 pt-2">
      <div class="flex h-11 items-center justify-end">{@render moreMenu()}</div>
      {#if !app.searching}<h1 class="fade m-0 px-1 text-[34px] leading-[1.15] font-bold tracking-[-0.6px]">{TITLES[app.filter]}</h1>{/if}
      {@render search()}
      {#if !app.searching}
        <div class="pt-1.5 pb-1"><Seg label="Show" items={[{ id: "active", name: `Active${counts().active ? " " + counts().active : ""}` }, { id: "all", name: "All" }, { id: "done", name: "Done" }]}
          value={app.filter === "overdue" ? "active" : app.filter} onchange={(v) => (app.filter = v as typeof app.filter)} /></div>
      {/if}
    </div>
    <div class="min-h-0 flex-1 overflow-y-auto px-4 pt-3 pb-24">{@render listArea()}</div>
    {#if !app.searching}
      <div class="absolute inset-x-0 bottom-0 z-10 flex items-center border-t border-sep bg-bg/90 px-4 pt-2 pb-[max(14px,env(safe-area-inset-bottom))] backdrop-blur-xl">
        <button type="button" onclick={newTask} class="press flex h-11 items-center gap-2 text-[17px] font-semibold text-accent">
          <svg width="26" height="26" viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="11" fill="currentColor" /><path d="M12 7v10M7 12h10" stroke="var(--on-accent)" stroke-width="2.2" stroke-linecap="round" /></svg>New Task</button>
      </div>
    {/if}
  </div>
  {#if app.panel.kind !== "none"}
    <div class="fixed inset-0 z-40 flex flex-col">
      <div class="fade absolute inset-0 bg-scrim"></div>
      <div class="rise relative mt-[max(10px,env(safe-area-inset-top))] flex min-h-0 flex-1 flex-col overflow-hidden rounded-t-[12px] bg-bg" style="max-height:{vh}px">
        {#if app.panel.kind === "new"}
          <TaskForm onclose={close} />
        {:else if app.panel.kind === "edit" && panelTask}
          <TaskForm task={panelTask} onclose={() => panelTask && (app.panel = { kind: "detail", id: panelTask.id })} />
        {:else if panelTask}
          <TaskDetail task={panelTask} onclose={close} onedit={() => panelTask && (app.panel = { kind: "edit", id: panelTask.id })} />
        {/if}
      </div>
    </div>
  {/if}
  <!-- Above sheets, so "Saved · Undo" is visible from the details screen too. -->
  <div class="pointer-events-none fixed inset-x-3 bottom-[max(76px,calc(env(safe-area-inset-bottom)+64px))] z-[60]"><Toast /></div>
{/if}
<SettingsSheet {desktop} />
