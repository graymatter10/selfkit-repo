<script lang="ts">
  import LockGate from "../../../ui/src/lock/LockGate.svelte"; // App lock (docs/components/app-lock.md)
  // Chrono main window. Phone (< 900 px): header + bottom tabs, settings and
  // sources as full pages. Desktop: left rail (screens, recording status with
  // the one Pause control, settings) + content.
  import { api, type Source } from "./lib/api";
  import { addDays, ago, dayMonth, longDay, mondayOf, weekRange, weekdayName } from "./lib/fmt";
  import { app, guard, init, initDone, isPaused, pause, pauseLabel, resume, today, type View } from "./lib/state.svelte";
  import Icon from "./components/Icon.svelte";
  import NoteEditor from "./components/NoteEditor.svelte";
  import PhotoViewer from "./components/PhotoViewer.svelte";
  import { PhotoSourceSheet, closePhotoSource } from "./lib/photo-source";
  import { onBackButtonPress } from "@tauri-apps/api/app";
  import Toast from "./components/Toast.svelte";
  import AppChanges from "./screens/AppChanges.svelte";
  import Report from "./screens/Report.svelte";
  import Settings from "./screens/Settings.svelte";
  import Setup from "./screens/Setup.svelte";
  import Sources from "./screens/Sources.svelte";
  import SyncSettings from "../../../ui/src/sync/SyncSettings.svelte";
  import Conflicts from "../../../ui/src/sync/Conflicts.svelte";
  import Timeline from "./screens/Timeline.svelte";
  import Today from "./screens/Today.svelte";
  import Trends from "./screens/Trends.svelte";

  init();
  let width = $state(window.innerWidth);
  const phone = $derived(width < 900);
  let scroller: HTMLElement | null = $state(null);
  let sources = $state<Source[]>([]);
  let trendsTab = $state<"apps" | "sites" | "devices" | "cats">("apps");
  $effect(() => { void app.rev; guard(api.sources()).then((s) => s && (sources = s)); });
  const recent = $derived(sources.filter((s) => app.now - s.last_seen < 7 * 86400000));
  const latest = $derived(Math.max(0, ...sources.map((s) => s.last_seen)));

  const NAVS: { id: View; name: string; short: string; key: string }[] = [
    { id: "today", name: "Today", short: "Today", key: "T" }, { id: "timeline", name: "Timeline", short: "Timeline", key: "L" },
    { id: "trends", name: "Trends", short: "Trends", key: "R" }, { id: "report", name: "Weekly report", short: "Report", key: "W" },
  ];
  const day = $derived(app.day ?? today());
  const isToday = $derived(day === today());
  const weekStart = $derived(mondayOf(app.week ?? addDays(today(), -7)));
  const head = $derived.by(() => {
    if (app.changesOpen && !phone) return ["App changes", "Apps installed, removed and reinstalled on your phones"];
    if (app.settingsOpen && !phone && app.conflictsOpen) return ["Changed on two devices", "Choose which version to keep"];
    if (app.settingsOpen && !phone && app.syncOpen) return ["Sync", "Your devices, server and encryption"];
    if (app.settingsOpen && !phone) return app.sourcesOpen ? ["Devices and sources", "Where Chrono gets its data"] : ["Settings", ""];
    switch (app.view) {
      case "today": return isToday ? ["Today", longDay(day)] : [weekdayName(day), dayMonth(day)];
      case "timeline": return ["Timeline", isToday ? longDay(day) : longDay(day)];
      case "trends": return ["Trends", ""];
      case "report": return ["Weekly report", weekRange(weekStart, addDays(weekStart, 6))];
    }
  });
  const dayNav = $derived(app.view === "today" || app.view === "timeline" || app.view === "report");
  const canNext = $derived(app.view === "report" ? addDays(weekStart, 7) <= today() : !isToday);
  function step(n: number) {
    if (app.view === "report") app.week = addDays(weekStart, 7 * n);
    else {
      const d = addDays(day, n);
      app.day = d >= today() ? null : d;
    }
  }
  function go(v: View) {
    app.view = v;
    app.settingsOpen = false;
    app.sourcesOpen = false;
    app.changesOpen = false;
    app.syncOpen = false;
    app.conflictsOpen = false;
    scroller?.scrollTo(0, 0);
  }
  function openSettings() { app.settingsOpen = true; app.sourcesOpen = false; app.changesOpen = false; app.syncOpen = false; app.conflictsOpen = false; }
  /** Back inside Settings: conflicts → sync → sources → settings. */
  function settingsBack() {
    if (app.conflictsOpen) app.conflictsOpen = false;
    else if (app.syncOpen) app.syncOpen = false;
    else if (app.sourcesOpen) app.sourcesOpen = false;
    else app.settingsOpen = false;
  }
  function seeAll() { trendsTab = "apps"; go("trends"); }

  // Android Back: one history entry while any layer is open; Back closes the topmost.
  const layered = $derived(phone && (!!app.photo || !!app.noteDay || app.setup > 0 || app.settingsOpen || app.changesOpen || app.pauseMenu || !!app.day || app.view !== "today"));
  // Android uses Tauri's back-button event instead (like Agenda/Hisab): the
  // WebView skips history entries re-added without a tap, so after closing an
  // inner layer (photo source sheet, photo viewer) the next Back left the app.
  const isAndroid = /Android/i.test(navigator.userAgent);
  if (isAndroid) {
    onBackButtonPress(() => {
      if (!closeTop()) (window as unknown as { ChronoBars?: { toBackground?(): void } }).ChronoBars?.toBackground?.();
    }).catch(() => {});
  }
  let pushed = false, skip = false;
  $effect(() => {
    if (isAndroid) return;
    if (layered && !pushed) { history.pushState({ chrono: 1 }, ""); pushed = true; }
    else if (!layered && pushed) { pushed = false; skip = true; history.back(); }
  });
  function closeTop(): boolean {
    if (closePhotoSource()) return true; // "Add photo" camera/gallery sheet
    if (app.photo) return void (app.photo = null), true;
    if (app.closeGuard?.()) return true;
    if (app.noteDay) return void (app.noteDay = null), true;
    if (app.pauseMenu) return void (app.pauseMenu = false), true;
    if (app.changesOpen) return void (app.changesOpen = false), true;
    if (app.conflictsOpen) return void (app.conflictsOpen = false), true;
    if (app.syncOpen) return void (app.syncOpen = false), true;
    if (app.sourcesOpen) return void (app.sourcesOpen = false), true;
    if (app.settingsOpen) return void (app.settingsOpen = false), true;
    if (app.day) return void (app.day = null), true;
    if (app.view !== "today") return go("today"), true;
    return false;
  }
  function onPop() {
    if (isAndroid) return;
    if (skip) return void (skip = false);
    pushed = false;
    closeTop();
  }

  function keys(e: KeyboardEvent) {
    const tag = (e.target as HTMLElement)?.tagName;
    if (["INPUT", "TEXTAREA", "SELECT"].includes(tag) || app.noteDay || e.ctrlKey || e.metaKey || e.altKey) return;
    if (e.key === "Escape") { if (app.photo || app.pauseMenu || app.changesOpen || app.sourcesOpen || app.syncOpen || app.settingsOpen || app.day) closeTop(); return; }
    const k = e.key.toLowerCase();
    if (k === "t") { go("today"); app.day = null; }
    else if (k === "l") go("timeline");
    else if (k === "r") go("trends");
    else if (k === "w") go("report");
    else if (k === ",") openSettings();
    else if (k === "n" && (app.view === "today")) app.noteDay = day;
    else if (k === "p") isPaused() ? resume() : (app.pauseMenu = !app.pauseMenu);
    else if (e.key === "ArrowLeft" && dayNav && !app.settingsOpen) step(-1);
    else if (e.key === "ArrowRight" && dayNav && canNext && !app.settingsOpen) step(1);
    else return;
    e.preventDefault();
  }

  // Test hook (CHRONO_DEMO env, desktop only) for screenshots.
  api.devHooks().then(async (h) => {
    if (!h) return;
    const p = Object.fromEntries(new URLSearchParams(h));
    app.demo = p;
    await initDone;
    await new Promise((r) => setTimeout(r, 200));
    if (p.theme) { app.theme = p.theme as typeof app.theme; document.documentElement.classList.toggle("dark", p.theme === "dark"); }
    if (p.view) app.view = p.view as View;
    if (p.day) app.day = addDays(today(), Number(p.day));
    if (p.week) app.week = addDays(today(), Number(p.week));
    if (p.settings) app.settingsOpen = true;
    if (p.sources) app.sourcesOpen = true;
    if (p.sync) { app.settingsOpen = true; app.syncOpen = true; }
    if (p.conflicts) { app.settingsOpen = true; app.syncOpen = true; app.conflictsOpen = true; }
    if (p.changes) app.changesOpen = true;
    if (p.note) app.noteDay = p.day ? addDays(today(), Number(p.day)) : today();
    if (p.setup) app.setup = Number(p.setup);
    if (p.pause) app.pauseMenu = true;
    if (p.scroll) setTimeout(() => scroller?.scrollTo(0, Number(p.scroll)), 600);
  });
</script>

<svelte:window bind:innerWidth={width} onkeydown={keys} onpopstate={onPop} />

{#snippet pauseMenu(style: string)}
  {#if app.pauseMenu}
    <button type="button" aria-label="Close menu" style="position:fixed;inset:0;z-index:45;cursor:default" onclick={() => (app.pauseMenu = false)}></button>
    <div class="menu fade" role="menu" {style}>
      <div class="muted" style="font-size:13px;padding:8px 12px 4px">Pause recording on all devices</div>
      <button type="button" role="menuitem" onclick={() => pause(15)}>For 15 min</button>
      <button type="button" role="menuitem" onclick={() => pause(60)}>For 1 hour</button>
      <button type="button" role="menuitem" onclick={() => pause("tomorrow")}>Until tomorrow</button>
      <button type="button" role="menuitem" onclick={() => pause(null)}>Until I resume</button>
    </div>
  {/if}
{/snippet}

{#snippet screen()}
  {#if app.error}<p role="alert" class="card" style="color:var(--danger);font-size:14px;margin:0 0 14px;padding:12px 16px">{app.error}</p>{/if}
  {#if app.changesOpen && !phone}<AppChanges {phone} />
  {:else if app.settingsOpen && !phone}
    {@render settingsBody()}
  {:else if app.view === "today"}<Today {phone} onseeall={seeAll} onsettings={openSettings} />
  {:else if app.view === "timeline"}<Timeline {phone} {scroller} />
  {:else if app.view === "trends"}<Trends {phone} {scroller} initialTab={trendsTab} onchanges={() => (app.changesOpen = true)} />
  {:else}<Report {phone} />{/if}
{/snippet}

{#snippet settingsBody()}
  {#if app.conflictsOpen}<div style={phone ? "flex:1;min-height:0;display:flex;flex-direction:column" : "height:calc(100vh - 150px);border:1px solid var(--line);border-radius:16px;overflow:hidden;display:flex;flex-direction:column"}><Conflicts {phone} onclose={() => (app.conflictsOpen = false)} names={{ day_notes: "Chrono day note", photos: "Chrono photo", chrono_settings: "Chrono settings" }} /></div>
  {:else if app.syncOpen}<SyncSettings {phone} onconflicts={() => (app.conflictsOpen = true)} />
  {:else if app.sourcesOpen}<Sources {phone} />
  {:else}<Settings {phone} onsources={() => (app.sourcesOpen = true)} onsync={() => (app.syncOpen = true)} />{/if}
{/snippet}

{#snippet dayButtons()}
  {#if dayNav}
    <button type="button" class="ib {phone ? '' : 'box'}" aria-label={app.view === "report" ? "Previous week" : "Previous day"} onclick={() => step(-1)}><Icon name="left" size={phone ? 22 : 18} /></button>
    <button type="button" class="ib {phone ? '' : 'box'}" aria-label={app.view === "report" ? "Next week" : "Next day"} disabled={!canNext} onclick={() => step(1)}><Icon name="right" size={phone ? 22 : 18} /></button>
  {/if}
{/snippet}

{#if phone}
  <div class="phone">
    <div class="top">
      <div style="flex:1;min-width:0"><h1>{head[0]}</h1>{#if head[1]}<div class="sub">{head[1]}</div>{/if}</div>
      {@render dayButtons()}
      <button type="button" class="ib" aria-label="Settings" onclick={openSettings}><Icon name="gear" size={22} /></button>
    </div>
    {#if app.view === "today"}
      <div class="statusline" style="position:relative">
        <span class="live" class:off={isPaused() || (app.android.supported && !app.android.usage)}></span>
        <span>{isPaused() ? pauseLabel() : app.android.supported && !app.android.usage ? "Phone time is off" : recent.length > 1 ? `Recording on ${recent.length} devices` : "Recording"}</span>
        {#if app.android.supported && !app.android.usage && !isPaused()}<button type="button" class="pill pz" style="color:var(--accent-ink)" onclick={() => (app.setup = 1)}>Allow</button>
        {:else if isPaused()}<button type="button" class="pill pz" onclick={resume}><Icon name="play" size={15} /> Resume</button>
        {:else}<button type="button" class="pill pz" aria-haspopup="menu" aria-expanded={app.pauseMenu} onclick={() => (app.pauseMenu = !app.pauseMenu)}><Icon name="pause" size={15} /> Pause</button>{/if}
        {@render pauseMenu("top:46px;right:16px")}
      </div>
    {/if}
    <div class="content" style="flex:1;overflow-y:auto" bind:this={scroller}>{@render screen()}</div>
    <nav class="tabbar">
      {#each NAVS as n}
        <button type="button" class="tab" class:on={app.view === n.id} aria-current={app.view === n.id ? "page" : undefined} onclick={() => (app.view === n.id && n.id === "today" ? (app.day = null) : go(n.id))}>
          <span class="ico"><Icon name={n.id} size={22} stroke={1.7} /></span>{n.short}
        </button>
      {/each}
    </nav>
  </div>
  {#if app.changesOpen}
    <div class="page rise" style="z-index:41">
      <div class="appbar">
        <button type="button" class="ib" aria-label="Back" onclick={() => (app.changesOpen = false)}><Icon name="back" size={22} /></button>
        <span class="t">App changes</span>
      </div>
      <div class="content" style="flex:1;overflow-y:auto;padding:8px 16px 24px"><AppChanges {phone} /></div>
    </div>
  {/if}
  {#if app.settingsOpen}
    <div class="page rise">
      <div class="appbar">
        <button type="button" class="ib" aria-label="Back" onclick={settingsBack}><Icon name="back" size={22} /></button>
        <span class="t">{app.conflictsOpen ? "Changed on two devices" : app.syncOpen ? "Sync" : app.sourcesOpen ? "Devices and sources" : "Settings"}</span>
      </div>
      {#if app.conflictsOpen}
        <div style="flex:1;min-height:0;display:flex;flex-direction:column">{@render settingsBody()}</div>
      {:else}
        <div class="content" style="flex:1;overflow-y:auto;padding:8px 16px 24px">{@render settingsBody()}</div>
      {/if}
    </div>
  {/if}
{:else}
  <div class="shell">
    <aside class="rail">
      <div class="brand"><i><svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="#fff" stroke-width="2.4" stroke-linecap="round" aria-hidden="true"><circle cx="12" cy="12" r="8" /><path d="M12 8v4l2.5 1.6" /></svg></i>Chrono</div>
      {#each NAVS as n}
        <button type="button" class="nav" class:on={!app.settingsOpen && !app.changesOpen && app.view === n.id} onclick={() => (go(n.id), n.id === "today" && (app.day = null))}><Icon name={n.id} />{n.name}<span class="k">{n.key}</span></button>
      {/each}
      <div class="spacer"></div>
      <div class="status" style="position:relative">
        <div class="t"><span class="live" class:off={isPaused()}></span>{isPaused() ? "Paused" : "Recording"}</div>
        <div class="s">{#if isPaused()}{pauseLabel().replace("Paused ", "")}{:else}{recent.length} {recent.length === 1 ? "device" : "devices"}<br />Latest data {ago(latest, app.now)}{/if}</div>
        {#if isPaused()}<button type="button" class="p" style="color:var(--accent-ink)" onclick={resume}><Icon name="play" size={15} /> Resume</button>
        {:else}<button type="button" class="p" aria-haspopup="menu" aria-expanded={app.pauseMenu} onclick={() => (app.pauseMenu = !app.pauseMenu)}><Icon name="pause" size={15} /> Pause</button>{/if}
        {@render pauseMenu("bottom:52px;left:0")}
      </div>
      <button type="button" class="nav" class:on={app.settingsOpen} onclick={openSettings}><Icon name="gear" />Settings<span class="k">,</span></button>
    </aside>
    <main class="main">
      <div class="top">
        {#if app.changesOpen}<button type="button" class="ib box" style="margin-right:4px" aria-label="Back" onclick={() => (app.changesOpen = false)}><Icon name="back" /></button>{/if}
        {#if app.settingsOpen && (app.sourcesOpen || app.syncOpen) && !app.changesOpen}<button type="button" class="ib box" style="margin-right:4px" aria-label="Back" onclick={settingsBack}><Icon name="back" /></button>{/if}
        <div><h1>{head[0]}</h1>{#if head[1]}<div class="sub">{head[1]}</div>{/if}</div>
        <div style="flex:1"></div>
        {#if !app.settingsOpen && !app.changesOpen}
          {#if (app.view === "today" || app.view === "timeline") && !isToday}<button type="button" class="btn" style="height:36px;margin-right:4px" onclick={() => (app.day = null)}>Back to today</button>{/if}
          <div class="daynav">{@render dayButtons()}</div>
        {/if}
      </div>
      <div class="content" bind:this={scroller}><div style="max-width:1180px">{@render screen()}</div></div>
    </main>
  </div>
{/if}

{#if app.noteDay}<NoteEditor day={app.noteDay} {phone} />{/if}
{#if app.setup > 0}<Setup />{/if}
<PhotoViewer />
<PhotoSourceSheet privateNote="Stays private in Chrono, not in your gallery" />
<div style="position:fixed;left:50%;transform:translateX(-50%);bottom:{phone ? 96 : 24}px;width:min(420px,calc(100vw - 32px));z-index:95;pointer-events:none"><Toast /></div>
<!-- App lock: covers everything while locked (ui/src/lock, docs/components/app-lock.md). -->
<LockGate appName="Chrono" phone={phone} />
