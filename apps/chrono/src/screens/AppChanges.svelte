<script lang="ts">
  // App changes (Android): installed / removed / reinstalled (and updates on
  // request), newest first, grouped by month, with each app's lifetime use.
  import { api, type AppChange } from "../lib/api";
  import { dayStr, dur, hm, monthName, shortDay } from "../lib/fmt";
  import { app, guard } from "../lib/state.svelte";
  import Icon from "../components/Icon.svelte";
  import Seg from "../components/Seg.svelte";
  let { phone, compact = false, onall }: { phone: boolean; compact?: boolean; onall?: () => void } = $props();
  let list = $state<AppChange[]>([]);
  let filter = $state<"all" | "installed" | "uninstalled" | "reinstalled">("all");
  let updates = $state(false);
  $effect(() => { void app.rev; guard(api.appChanges()).then((l) => l && (list = l)); });
  const KIND: Record<string, string> = { installed: "Installed", uninstalled: "Removed", reinstalled: "Reinstalled", updated: "Updated" };
  const shown = $derived(list.filter((c) => (filter === "all" ? updates || c.kind !== "updated" : c.kind === filter)));
  const groups = $derived.by(() => {
    const m = new Map<string, AppChange[]>();
    for (const c of shown) {
      const d = new Date(c.at);
      const k = `${monthName(d.getMonth())} ${d.getFullYear()}`;
      if (!m.has(k)) m.set(k, []);
      m.get(k)!.push(c);
    }
    return [...m];
  });
  const when = (t: number) => `${shortDay(dayStr(new Date(t)))}, ${hm(t)}`;
  const recent = $derived(list.filter((c) => c.kind !== "updated").slice(0, 3));
</script>

{#snippet row(c: AppChange)}
  <div class="row" style="min-height:64px">
    <span class="appicon">{#if c.icon}<img src={c.icon} alt="" />{:else}<Icon name="phone" />{/if}</span>
    <div style="flex:1;min-width:0;padding:8px 0">
      <div style="display:flex;align-items:center;gap:8px;min-width:0"><span class="nm" style="overflow:hidden;text-overflow:ellipsis;white-space:nowrap">{c.label}</span>{#if !c.installed_now}<span class="tag">Uninstalled</span>{/if}</div>
      <div class="sb">{KIND[c.kind] ?? c.kind} · {when(c.at)}{phone ? "" : ` · ${c.device_name}`}</div>
    </div>
    <div style="text-align:right"><div class="val">{c.lifetime > 0 ? dur(c.lifetime) : "–"}</div><div class="muted" style="font-size:12.5px;margin-top:2px">all-time use</div></div>
  </div>
{/snippet}

{#if compact}
  {#if recent.length || app.info?.platform === "android"}
  <div class="card">
    <div class="ch" style="margin-bottom:4px"><h2>App changes</h2>{#if onall}<button type="button" class="lnk" onclick={onall}>See all</button>{/if}</div>
    {#if recent.length}{#each recent as c (c.id)}{@render row(c)}{/each}
    {:else}<p class="hint" style="margin:8px 0 0">Apps you install, remove or reinstall on your phone show up here, with how much you used them.</p>{/if}
  </div>
  {/if}
{:else}
  <div class="grid" style="max-width:820px">
    <div class="card">
      <div style="display:flex;flex-wrap:wrap;gap:10px;align-items:center;margin-bottom:8px">
        <div style="max-width:100%;overflow-x:auto;{phone ? 'flex:1' : ''}"><Seg label="Show" value={filter} onchange={(f) => (filter = f)}
          items={[{ id: "all", name: "All" }, { id: "installed", name: "Installed" }, { id: "uninstalled", name: "Removed" }, { id: "reinstalled", name: phone ? "Back" : "Reinstalled" }]} /></div>
        {#if filter === "all"}
          <button type="button" class="tgwrap" style="display:flex;align-items:center;gap:10px;font-size:14px;color:var(--ink2)" role="switch" aria-checked={updates} onclick={() => (updates = !updates)}>Show updates<span class="tg" class:off={!updates}></span></button>
        {/if}
      </div>
      {#each groups as [month, items] (month)}
        <div class="sec" style="margin:14px 0 2px;font-weight:600;color:var(--ink)">{month}</div>
        {#each items as c (c.id)}{@render row(c)}{/each}
      {/each}
      {#if !shown.length}<p class="hint" style="margin:12px 0 4px">No app changes recorded yet. Chrono compares the apps on your phone each time it imports (when you open it and every 15 minutes).</p>{/if}
    </div>
    <p class="hint" style="margin:0 4px">Removed apps keep their name, icon and history. "Removed" is the time Chrono noticed it, within about 15 minutes.</p>
  </div>
{/if}
