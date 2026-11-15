<script lang="ts">
  import type { AppKey, RSettings, StreakDef } from "../lib/api";
  import { APP_NAMES, APP_WHAT, clock, durShort } from "../lib/fmt";
  import { app, saveSettings, setTheme } from "../lib/state.svelte";
  import Icon from "./Icon.svelte";
  import Seg from "./Seg.svelte";
  import { syncApi } from "../../../../ui/src/sync/api";
  import LockSettings from "../../../../ui/src/lock/LockSettings.svelte"; // App lock section
  import { openTab } from "../lib/state.svelte";
  let { desktop }: { desktop: boolean } = $props();
  let syncLine = $state("");
  $effect(() => { syncApi.status().then((st) => (syncLine = st.conflicts ? `${st.headline.split(" · ")[0]} · ${st.conflicts} to review` : st.headline)).catch(() => (syncLine = "")); });
  const s = $derived(app.settings);
  const KINDS: { kind: string; name: string; sub: string; value: number }[] = [
    { kind: "phone_under", name: "Phone under a time", sub: "Phone screen time each day", value: 180 },
    { kind: "screen_under", name: "All screens under a time", sub: "Phone and PCs together", value: 300 },
    { kind: "asleep_by", name: "Asleep by a time", sub: "From Dusk", value: 0 },
    { kind: "tasks_min", name: "Tasks done each day", sub: "From Agenda", value: 3 },
    { kind: "budget", name: "Spending within budget", sub: "Your Hisab budgets", value: 0 },
  ];
  function name(d: StreakDef) {
    switch (d.kind) {
      case "phone_under": return "Phone under";
      case "screen_under": return "All screens under";
      case "asleep_by": return "Asleep by";
      case "tasks_min": return "Tasks done";
      default: return "Spending within budget";
    }
  }
  function sub(d: StreakDef) {
    return ({ phone_under: "Each day", screen_under: "Phone and PCs, each day", asleep_by: "Each night", tasks_min: "At least, each day", budget: "Your Hisab budgets" } as Record<string, string>)[d.kind] ?? "";
  }
  const shown = (d: StreakDef) => (d.kind === "asleep_by" ? clock(d.value) : d.kind === "tasks_min" ? String(d.value) : durShort(d.value));
  const clone = (x: RSettings): RSettings => JSON.parse(JSON.stringify(x));
  function patch(i: number, p: Partial<StreakDef>) {
    if (!s) return;
    const n = clone(s);
    n.streaks[i] = { ...n.streaks[i], ...p };
    saveSettings(n);
  }
  function step(i: number, dir: number) {
    const d = s!.streaks[i];
    const v = d.kind === "tasks_min" ? Math.max(1, d.value + dir) : d.kind === "asleep_by" ? (d.value + dir * 15 + 1440) % 1440 : Math.max(15, Math.min(960, d.value + dir * 15));
    patch(i, { value: v });
  }
  function add(kind: string, value: number) {
    if (!s) return;
    const n = clone(s);
    n.streaks.push({ id: `s-${Date.now().toString(36)}`, kind, value, on: true });
    app.adding = false;
    saveSettings(n);
  }
  function remove(i: number) {
    if (!s) return;
    const before = clone(s);
    const n = clone(s);
    n.streaks.splice(i, 1);
    saveSettings(n, "Streak removed", before);
  }
  const first = $derived(Object.fromEntries(app.sources?.first ?? []) as Record<AppKey, string | null>);
  const found = $derived(Object.fromEntries(app.sources?.found ?? []) as Record<AppKey, boolean>);
  const since = (d: string | null) => (d ? new Date(d + "T12:00").toLocaleDateString("en-GB", { day: "numeric", month: "short", year: "numeric" }) : "");
  const dayStart = $derived(app.shared ? clock(app.shared.day_start_min) : "");
</script>

{#snippet streaks()}
  <div class="card">
    {#each s!.streaks as d, i (d.id)}
      <div class="lrow {desktop ? '' : 'flex-wrap'}">
        <div class="lbl"><b>{name(d)}</b><small>{sub(d)}</small></div>
        {#if d.kind !== "budget" && d.on}
          <div class="{desktop ? 'contents' : 'order-last flex basis-full pb-1'}"><span class="flex h-11 shrink-0 items-center rounded-[12px] bg-fill">
            <button type="button" class="press grid h-11 w-10 place-items-center text-ink2" aria-label="Less" onclick={() => step(i, -1)}><Icon name="minus" size={17} stroke={2.1} /></button>
            <b class="num min-w-[58px] text-center text-[14.5px] font-[650]">{shown(d)}</b>
            <button type="button" class="press grid h-11 w-10 place-items-center text-ink2" aria-label="More" onclick={() => step(i, 1)}><Icon name="plus" size={17} stroke={2.1} /></button>
          </span></div>
        {/if}
        <button type="button" role="switch" aria-checked={d.on} aria-label="{name(d)} on" onclick={() => patch(i, { on: !d.on })} class="relative ml-1 h-11 w-12 shrink-0 {d.kind === 'budget' || !d.on || !desktop ? 'ml-auto' : ''}">
          <span class="absolute top-2 left-0 h-7 w-12 rounded-full transition-colors {d.on ? 'bg-accent' : 'bg-faint'}"></span>
          <span class="absolute top-3 h-5 w-5 rounded-full transition-[left] {d.on ? 'left-6 bg-on-accent' : 'left-1 bg-white'}"></span>
        </button>
        {#if d.id.startsWith("s-")}<button type="button" class="iconbtn press -mr-2" aria-label="Remove streak" onclick={() => remove(i)}><Icon name="trash" size={18} /></button>{/if}
      </div>
    {/each}
  </div>
  {#if app.adding}
    <div class="card fade mt-2.5">
      <div class="ctitle px-4 pt-3.5">New streak<button type="button" class="iconbtn press ml-auto -mr-2" aria-label="Close" onclick={() => (app.adding = false)}><Icon name="close" size={18} /></button></div>
      {#each KINDS.filter((k) => k.kind !== "budget" || !s!.streaks.some((x) => x.kind === "budget")) as k}
        <button type="button" class="lrow press w-full text-left" onclick={() => add(k.kind, k.value)}><div class="lbl"><b>{k.name}</b><small>{k.sub}</small></div><Icon name="plus" size={18} /></button>
      {/each}
    </div>
  {:else}
    <button type="button" class="btn press mt-2.5" onclick={() => (app.adding = true)}><Icon name="plus" size={18} stroke={2.1} />New streak</button>
  {/if}
{/snippet}
{#snippet sources()}
  <div class="card">
    {#each ["chrono", "hisab", "agenda", "dusk"] as AppKey[] as a}
      <div class="lrow"><span class="grid w-5 shrink-0 place-items-center"><i class="block h-2 w-2 rounded-full" style="background:var(--{a})"></i></span>
        <div class="lbl"><b>{APP_NAMES[a]}</b><small>{APP_WHAT[a]}{first[a] ? ` · since ${since(first[a])}` : ""}</small></div>
        <span class="v">{#if app.sources?.has[a]}<span class="text-accent"><Icon name="check" size={17} stroke={2.4} /></span>{:else}<span class="text-[13px] font-medium text-muted">{app.sources?.mobile ? "Not shared yet" : found[a] ? "No data yet" : "Not installed"}</span>{/if}</span></div>
    {/each}
  </div>
  <p class="mx-1 mt-2 text-[13px] leading-[1.45] text-muted">Reflect only reads these apps and never changes them. An app that isn't installed is simply left out.{app.sources?.mobile ? " On this phone Reflect reads the Selfkit apps installed here (each app shares its data only with your other Selfkit apps)." : ""}</p>
{/snippet}
{#snippet other()}
  <div class="card">
    <div class="lrow {desktop ? '' : 'flex-col !items-stretch'}"><div class="lbl"><b>Connections look back</b></div>
      <Seg label="Look back" items={[{ id: 30, name: "1 month" }, { id: 90, name: "3 months" }, { id: 365, name: "1 year" }]} value={s!.lookback_days} onchange={(v) => saveSettings({ ...clone(s!), lookback_days: v })} /></div>
    <div class="lrow"><div class="lbl"><b>Week starts on</b></div>
      <Seg label="Week starts on" items={[{ id: 0, name: "Monday" }, { id: 6, name: "Sunday" }]} value={s!.week_start} onchange={(v) => saveSettings({ ...clone(s!), week_start: v })} /></div>
    <div class="lrow"><div class="lbl"><b>Day starts at</b><small>Shared by all Selfkit apps, set in Chrono or Dusk</small></div><span class="v num">{dayStart}</span></div>
  </div>
{/snippet}
{#snippet syncRow()}
  <div class="card"><button type="button" class="lrow w-full text-left" onclick={() => openTab("sync")}><div class="lbl"><b>Sync</b><small>{syncLine || "Your devices, server and encryption"}</small></div><span class="flex text-muted"><Icon name="right" size={18} /></span></button></div>
{/snippet}
{#snippet look()}
  <div class="card"><div class="lrow"><div class="lbl"><b>Theme</b><small>On this device</small></div>
    <Seg label="Theme" items={[{ id: "system", name: "System" }, { id: "light", name: "Light" }, { id: "dark", name: "Dark" }]} value={app.theme} onchange={(v) => setTheme(v)} /></div></div>
{/snippet}

{#if !s}<p class="pt-16 text-center text-muted">Loading…</p>
{:else if desktop}
  <div class="grid max-w-[1020px] grid-cols-2 gap-7 pb-8">
    <div><h1 class="dtitle">Settings</h1><div class="sect !mt-5">Streaks</div>{@render streaks()}</div>
    <div class="pt-[31px]"><div class="sect !mt-5">Data sources</div>{@render sources()}<div class="sect">Reviews and connections</div>{@render other()}<div class="sect">Sync</div>{@render syncRow()}<div class="sect">App lock</div><LockSettings phone={!desktop} appName="Reflect" heading={false} /><div class="sect">Appearance</div>{@render look()}</div>
  </div>
{:else}
  <div class="pb-4"><div class="sect !mt-1">Streaks</div>{@render streaks()}<div class="sect">Data sources</div>{@render sources()}<div class="sect">Reviews and connections</div>{@render other()}<div class="sect">Sync</div>{@render syncRow()}<div class="sect">App lock</div><LockSettings phone={!desktop} appName="Reflect" heading={false} /><div class="sect">Appearance</div>{@render look()}</div>
{/if}
