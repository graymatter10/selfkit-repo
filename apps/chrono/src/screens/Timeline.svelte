<script lang="ts">
  // Combined timeline for one day: lanes per device/browser + session list
  // (virtualized; short checks under a minute folded away).
  import { api, type SessionRow, type TimelineView } from "../lib/api";
  import { catColor, catName, dur, durShort, hm } from "../lib/fmt";
  import { app, guard } from "../lib/state.svelte";
  import Icon from "../components/Icon.svelte";
  import LaneChart from "../components/LaneChart.svelte";
  import Seg from "../components/Seg.svelte";
  import VList from "../components/VList.svelte";
  let { phone, scroller }: { phone: boolean; scroller: HTMLElement | null } = $props();
  let v = $state<TimelineView | null>(null);
  let sel = $state<{ lane: number; i: number } | null>(null);
  let filter = $state("all");
  let showShort = $state(false);
  $effect(() => {
    void app.rev;
    const d = app.day ?? undefined;
    guard(api.timeline(d)).then((r) => r && (v = r));
  });
  $effect(() => { void app.day; sel = null; showShort = false; });
  const selSeg = $derived(v && sel ? { seg: v.lanes[sel.lane]?.segs[sel.i], lane: v.lanes[sel.lane] } : null);
  const devs = $derived.by(() => {
    const m = new Map<string, string>();
    for (const s of v?.sessions ?? []) m.set(s.device_id, s.device_name);
    return [...m];
  });
  const filtered = $derived((v?.sessions ?? []).filter((s) => filter === "all" || s.device_id === filter));
  const long = $derived(filtered.filter((s) => s.end - s.start >= 60000));
  const shortN = $derived(filtered.length - long.length);
  const rows = $derived(showShort ? filtered : long);
  const rowH = $derived(phone ? 62 : 58);
  const CATS = ["work", "study", "social", "entertainment", "other"];
</script>

{#snippet sessionRow(s: SessionRow)}
  <div class="row" style="height:100%;align-items:center">
    <span class="num muted" style="width:{phone ? 50 : 104}px;font-size:13.5px;white-space:nowrap;line-height:1.35">{#if phone}{hm(s.start)}<br />{hm(s.end)}{:else}{hm(s.start)}–{hm(s.end)}{/if}</span>
    <div style="flex:1;min-width:0">
      <div class="nm" style="display:flex;align-items:center;gap:8px;min-width:0"><span style="overflow:hidden;text-overflow:ellipsis;white-space:nowrap">{s.name}</span>{#if s.uninstalled}<span class="tag">Uninstalled</span>{/if}</div>
      <div class="sb" style="display:flex;align-items:center;gap:6px;white-space:nowrap;overflow:hidden"><i class="sw" style="background:{catColor(s.cat)}"></i>{catName(s.cat)} · {s.device}</div>
    </div>
    <span class="val">{durShort(s.end - s.start)}</span>
  </div>
{/snippet}

{#if v}
  <div class="grid">
    <div class="card">
      <div class="ch"><h2>All devices</h2><span class="r">{phone ? "Tap a block for details" : "Click a block for details · idle time not counted"}</span></div>
      {#if v.lanes.length}
        <LaneChart lanes={v.lanes} from={v.from} to={v.to} {phone} selected={sel} onselect={(s) => (sel = s)} />
        <div class="legend" style="margin-top:14px">{#each CATS as c}<span><i class="sw" style="background:{catColor(c)}"></i>{catName(c)}</span>{/each}</div>
        {#if selSeg?.seg}
          {@const [a, b, name, cat] = selSeg.seg}
          <div class="sel fade" style="margin-top:16px;align-items:flex-start">
            <i class="sw" style="background:{catColor(cat)};margin-top:4px"></i>
            <div style="flex:1;min-width:0"><div style="font-weight:600;overflow:hidden;text-overflow:ellipsis">{name}</div><div class="muted" style="font-size:13px;margin-top:2px">{catName(cat)} · {selSeg.lane.name}</div></div>
            <div class="num" style="text-align:right"><div>{durShort(b - a)}</div><div class="muted" style="font-size:13px;margin-top:2px">{hm(a)}–{hm(b)}</div></div>
            <button type="button" class="ib" style="width:32px;height:32px;margin:-6px -6px 0 0" aria-label="Clear selection" onclick={() => (sel = null)}><Icon name="close" size={16} /></button>
          </div>
        {/if}
      {:else}
        <p class="hint" style="margin:0">Nothing recorded on this day.</p>
      {/if}
    </div>
    {#if v.sessions.length}
      <div class="card">
        <div class="ch" style="margin-bottom:10px"><h2>Sessions</h2><span class="r">{rows.length} · newest first</span></div>
        {#if devs.length > 1}
          <div style="margin-bottom:6px;overflow-x:auto"><Seg label="Device" value={filter} onchange={(f) => (filter = f)} items={[{ id: "all", name: "All" }, ...devs.map(([id, name]) => ({ id, name }))]} /></div>
        {/if}
        <VList items={rows} {rowH} {scroller} row={sessionRow} />
        {#if shortN > 0}
          <div class="row" style="min-height:48px"><span class="muted" style="flex:1;font-size:14px">{shortN} short {shortN === 1 ? "check" : "checks"} under 1 minute</span><button type="button" class="lnk" style="color:var(--accent-ink);font-weight:600;font-size:14px;min-height:44px" onclick={() => (showShort = !showShort)}>{showShort ? "Hide" : "Show"}</button></div>
        {/if}
      </div>
    {/if}
  </div>
{/if}
