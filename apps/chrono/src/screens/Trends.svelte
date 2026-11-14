<script lang="ts">
  // Trends: day / week / month / year / all time. Led by the daily average in
  // words, then bars (tap for details), totals, peak times, daily habits.
  import { api, type Period, type Total, type TrendsView } from "../lib/api";
  import { catColor, catName, dayStr, devIcon, dur, monthName, parseDay, shortDay } from "../lib/fmt";
  import { app, guard, today } from "../lib/state.svelte";
  import BarChart from "../components/BarChart.svelte";
  import Heatmap from "../components/Heatmap.svelte";
  import Icon from "../components/Icon.svelte";
  import Seg from "../components/Seg.svelte";
  import VList from "../components/VList.svelte";
  import AppChanges from "./AppChanges.svelte";
  let { phone, scroller, initialTab = "apps", onchanges }: { phone: boolean; scroller: HTMLElement | null; initialTab?: Tab; onchanges: () => void } = $props();
  let withGone = $state(true);
  type Tab = "apps" | "sites" | "devices" | "cats";
  let period = $state<Period>((app.demo.period as Period) || "month");
  let anchor = $state<string | undefined>(undefined);
  let v = $state<TrendsView | null>(null);
  let sel = $state(-1);
  let heatSel = $state<[number, number] | null>(null);
  let tab = $state<Tab>("apps");
  let all = $state(false);
  $effect(() => { tab = initialTab; });
  $effect(() => {
    void app.rev;
    const p = period, a = anchor;
    guard(api.trends(p, a)).then((r) => {
      if (!r) return;
      v = r;
      const t = today();
      const i = r.buckets.findIndex((b, k) => b.from <= t && (k === r.buckets.length - 1 || r.buckets[k + 1].from > t));
      const lastData = r.buckets.map((b) => b.ms > 0).lastIndexOf(true);
      sel = i >= 0 && r.buckets[i].ms > 0 ? i : lastData;
    });
  });
  function setPeriod(p: Period) { period = p; anchor = undefined; all = false; heatSel = null; }
  const PERIODS: { id: Period; name: string }[] = [{ id: "day", name: "Day" }, { id: "week", name: "Week" }, { id: "month", name: "Month" }, { id: "year", name: "Year" }, { id: "lifetime", name: "All" }];
  const unit = $derived(!v ? "" : v.period === "day" ? "hour" : v.period === "week" || v.period === "month" ? "day" : v.buckets[0]?.label.match(/^\d{4}$/) ? "year" : "month");
  const future = $derived(v ? v.buckets.filter((b) => b.from > today()).length : 0);
  const shown = $derived(v ? v.buckets.slice(0, v.buckets.length - future) : []);
  const prevName = $derived.by(() => {
    if (!v) return "";
    const d = parseDay(v.from);
    return { day: "the day before", week: "last week", month: monthName((d.getMonth() + 11) % 12), year: String(d.getFullYear() - 1), lifetime: "" }[v.period];
  });
  const prevShort = $derived(v?.period === "month" ? prevName.slice(0, 3) : prevName);
  const avg = $derived(v ? (v.period === "day" ? v.summary.screen : v.summary.avg) : 0);
  const prevAvg = $derived(v ? (v.period === "day" ? v.prev.screen : v.prev.avg) : 0);
  const showLabel = (i: number) => {
    if (!v) return false;
    const n = v.buckets.length;
    if (v.period === "day") return i % (phone ? 6 : 3) === 0;
    if (v.period === "month") return [0, 7, 14, 21, 28].includes(i);
    if (v.period === "week") return true;
    if (v.period === "year") return phone ? i % 2 === 0 : true;
    const step = Math.ceil(n / (phone ? 5 : 10));
    return i % step === 0;
  };
  function bucketName(i: number): string {
    if (!v) return "";
    const b = v.buckets[i];
    if (!b) return "";
    if (v.period === "day") return `${b.label}:00–${Number(b.label) + 1}:00`;
    if (unit === "day") return b.from === today() ? `Today, ${shortDay(b.from)}` : shortDay(b.from);
    if (unit === "month") return `${monthName(parseDay(b.from).getMonth())} ${parseDay(b.from).getFullYear()}`;
    return b.label;
  }
  // totals list
  const anyGone = $derived(!!v?.apps.some((t) => t.uninstalled));
  const list = $derived.by((): { key: string; name: string; sub: string; ms: number; cat?: string; icon?: string; prev: number; gone?: boolean }[] => {
    if (!v) return [];
    if (tab === "apps" || tab === "sites")
      return (tab === "apps" ? v.apps : v.sites)
        .filter((t: Total) => withGone || !t.uninstalled)
        .map((t: Total) => ({ key: t.kind + t.name, name: t.name, sub: "", ms: t.ms, cat: t.cat, prev: t.prev, gone: !!t.uninstalled }));
    if (tab === "devices") return v.devices.map((d) => ({ key: d.id, name: d.name, sub: d.web && d.platforms.length > 1 ? "incl. browser" : "", ms: d.ms, icon: devIcon(d.platforms), prev: -1 }));
    return Object.entries(v.summary.cats).sort((a, b) => b[1] - a[1]).map(([c, ms]) => ({ key: c, name: catName(c), sub: "", ms, cat: c, prev: v!.prev.cats[c] ?? 0 }));
  });
  function delta(ms: number, prev: number): string {
    if (!v?.has_prev || prev <= 0 || !prevShort) return "";
    // compare per day: the current period may not be over yet
    const p = prev * (Math.max(1, v.summary.days) / Math.max(1, v.prev.days));
    const d = Math.round(((ms - p) / p) * 100);
    return Math.abs(d) >= 10 ? `${Math.abs(d)}% ${d > 0 ? "more" : "less"} than ${prevShort}` : "";
  }
  // peak times sentence
  const peak = $derived.by(() => {
    if (!v) return "";
    const h = v.heatmap;
    const avgRows = (rows: number[]) => Array.from({ length: 24 }, (_, k) => rows.reduce((a, r) => a + (h[r]?.[k] ?? 0), 0) / rows.length);
    const wk = avgRows([0, 1, 2, 3, 4]), we = avgRows([5, 6]);
    const top = (a: number[]) => { const m = Math.max(...a); return m < 60000 ? -1 : a.indexOf(m); };
    const a = top(wk), b = top(we);
    if (a < 0 && b < 0) return "";
    const part = (x: number) => (x < 12 ? "mornings" : x < 17 ? "afternoons" : "evenings");
    const s1 = a >= 0 ? `weekdays around ${a}–${a + 1}` : "";
    const s2 = b >= 0 ? `weekend ${part(b)} around ${b}–${b + 1}` : "";
    return [s1, s2].filter(Boolean).join("</b>, and on <b>");
  });
  const DAYS = ["Mondays", "Tuesdays", "Wednesdays", "Thursdays", "Fridays", "Saturdays", "Sundays"];
  const habits = $derived.by(() => {
    if (!v) return [];
    const n = Math.max(1, v.summary.active_days), s = v.summary;
    const r: [string, string][] = [];
    if (s.pickups > 0) r.push(["Phone screen turned on", `${Math.round(s.pickups / n)} times`], ["Phone unlocked", `${Math.round(s.unlocks / n)} times`]);
    if (s.notifications > 0) r.push(["Notifications", `${Math.round(s.notifications / n)}`]);
    if (s.focus != null) r.push(["Focus (Work + Study)", `${s.focus}%`]);
    if (s.longest) r.push(["Longest session", `${dur(s.longest[1])} · ${s.longest[0]}, ${shortDay(dayStr(new Date(s.longest[2])))}`]);
    return r;
  });
</script>

{#snippet periodBar()}
  <Seg label="Period" items={PERIODS} value={period} onchange={setPeriod} style={phone ? "flex:1" : ""} />
{/snippet}

{#snippet chartCard()}
  {#if v}
    <div class="card">
      <div class="ch"><h2>Screen time per {unit}</h2><span class="r">{v.title}</span></div>
      <div style="display:flex;align-items:flex-end;gap:12px;flex-wrap:wrap"><div class="mid num">{dur(avg)}</div><div class="muted" style="font-size:14px;padding-bottom:3px">{v.period === "day" ? "in total" : "a day on average"}</div></div>
      <div class="say" style="margin-top:6px">
        {#if v.has_prev && prevAvg > 0 && prevName}<b>{dur(Math.abs(avg - prevAvg))}{v.period === "day" ? "" : " a day"} {avg <= prevAvg ? "less" : "more"}</b> than {prevName}.{/if}
        {#if v.period !== "day"} {dur(v.summary.screen)} in total{v.next_anchor ? "" : " so far"}.{/if}
        {#if v.summary.screen === 0}Nothing recorded in this period.{/if}
      </div>
      <div style="margin-top:20px">
        <BarChart values={shown.map((b) => b.ms)} labels={v.buckets.map((b) => b.label)} selected={sel} onselect={(i) => (sel = i)} avg={v.period === "day" ? 0 : unit === "day" ? v.summary.avg : 0} {phone} {future} {showLabel} />
      </div>
      {#if sel >= 0 && v.buckets[sel]}
        <div class="sel" style="margin-top:14px"><span class="sw" style="background:var(--accent)"></span><span style="font-weight:600">{bucketName(sel)}</span><span class="num" style="margin-left:auto">{dur(v.buckets[sel].ms)}</span></div>
      {/if}
    </div>
  {/if}
{/snippet}

{#snippet heatCard()}
  {#if v}
    <div class="card">
      <div class="ch"><h2>When you use screens</h2><span class="r">typical week</span></div>
      {#if peak}<div class="say" style="margin:0 0 14px">Busiest on <b>{@html peak}</b>.</div>{/if}
      <Heatmap data={v.heatmap} {phone} selected={heatSel} onselect={(c) => (heatSel = c)} />
      {#if heatSel}
        <div class="sel fade" style="margin-top:12px"><span style="font-weight:600">{DAYS[heatSel[0]]} {String(heatSel[1]).padStart(2, "0")}:00–{String(heatSel[1] + 1).padStart(2, "0")}:00</span><span class="num" style="margin-left:auto">{dur(v.heatmap[heatSel[0]][heatSel[1]])} on average</span></div>
      {/if}
      <div class="legend" style="margin-top:12px;align-items:center;gap:8px">
        {#each ["None", "Some", "A lot", "Most"] as t, i}<span style="margin-right:8px"><i class="sw" style="background:var(--h{i});box-shadow:inset 0 0 0 1px var(--line)"></i>{t}</span>{/each}
      </div>
      <div class="hint" style="margin-top:8px">Average per hour, {shortDay(v.heat_from)} – {shortDay(v.heat_to)}.</div>
    </div>
  {/if}
{/snippet}

{#snippet listRow(it: (typeof list)[number])}
  <div class="row" style="height:100%">
    {#if it.icon}<span class="muted" style="display:flex"><Icon name={it.icon} /></span>{/if}
    <div style="flex:1;min-width:0">
      <div class="nm" style="display:flex;align-items:center;gap:8px;min-width:0"><span style="overflow:hidden;text-overflow:ellipsis;white-space:nowrap">{it.name}</span>{#if it.gone}<span class="tag">Uninstalled</span>{/if}{#if it.sub}<span class="muted" style="font-size:13px">{it.sub}</span>{/if}</div>
      {#if it.cat}
        {@const d = delta(it.ms, it.prev)}
        <div class="sb" style="display:flex;align-items:center;gap:6px;white-space:nowrap;overflow:hidden">{#if tab !== "cats"}<i class="sw" style="background:{catColor(it.cat)}"></i>{catName(it.cat)}{/if}{#if d}{tab !== "cats" ? "\u00a0· " : ""}<span style="color:var(--ink2)">{d}</span>{/if}{#if tab === "cats" && !d}&nbsp;{/if}</div>
      {/if}
    </div>
    <span class="val">{dur(it.ms)}</span>
  </div>
{/snippet}

{#snippet listCard()}
  {#if v}
    <div class="card">
      <div class="ch" style="margin-bottom:12px"><h2>Totals</h2><span class="r">{v.title}</span></div>
      <div style="margin-bottom:6px;display:flex"><Seg label="Totals" value={tab} onchange={(t) => ((tab = t), (all = false))} style={phone ? "flex:1" : ""}
        items={[{ id: "apps", name: "Apps" }, { id: "sites", name: "Websites" }, { id: "devices", name: "Devices" }, { id: "cats", name: "Categories" }]} /></div>
      {#if tab === "apps" && anyGone}
        <button type="button" class="tgwrap" style="display:flex;justify-content:space-between;align-items:center;width:100%;font-size:14px;color:var(--ink2)" role="switch" aria-checked={withGone} onclick={() => (withGone = !withGone)}>Include uninstalled apps<span class="tg" class:off={!withGone}></span></button>
      {/if}
      {#if list.length}
        {#if all}<VList items={list} rowH={62} {scroller} row={listRow} />
        {:else}{#each list.slice(0, phone ? 5 : 6) as it (it.key)}<div style="height:62px">{@render listRow(it)}</div>{/each}{/if}
        {#if list.length > (phone ? 5 : 6)}
          <button type="button" class="row" style="min-height:48px;color:var(--accent-ink);font-weight:600;font-size:14px;justify-content:center" onclick={() => (all = !all)}>{all ? "Show fewer" : `Show all ${list.length}`}</button>
        {/if}
      {:else}<p class="hint" style="margin:8px 0 0">Nothing here for this period.</p>{/if}
    </div>
  {/if}
{/snippet}

{#snippet habitsCard()}
  {#if habits.length}
    <div class="card">
      <div class="ch" style="margin-bottom:2px"><h2>Daily habits</h2><span class="r">average per day</span></div>
      {#each habits as [a, b]}<div class="row" style="min-height:46px"><span style="flex:1">{a}</span><span class="val" style="white-space:normal;text-align:right">{b}</span></div>{/each}
    </div>
  {/if}
{/snippet}

{#snippet nav()}
  {#if v && period !== "lifetime"}
    <div class="daynav">
      <button type="button" class="ib box" aria-label="Previous" onclick={() => v && (anchor = v.prev_anchor)}><Icon name="left" /></button>
      <button type="button" class="ib box" aria-label="Next" disabled={!v.next_anchor} onclick={() => v?.next_anchor && (anchor = v.next_anchor)}><Icon name="right" /></button>
    </div>
  {/if}
{/snippet}

{#if phone}
  <div style="margin-bottom:10px;display:flex">{@render periodBar()}</div>
  {#if v && period !== "lifetime"}
    <div style="display:flex;align-items:center;gap:6px;margin-bottom:12px"><span style="font-weight:600;font-size:15px;flex:1">{v.title}</span>{@render nav()}</div>
  {/if}
  <div class="grid">{@render chartCard()}{@render listCard()}<AppChanges {phone} compact onall={onchanges} />{@render heatCard()}{@render habitsCard()}</div>
{:else}
  <div style="display:flex;align-items:center;gap:14px;margin-bottom:18px">{@render periodBar()}<span style="margin-left:8px">{@render nav()}</span><span style="font-weight:600;font-size:15px">{v?.title ?? ""}</span></div>
  <div class="grid" style="grid-template-columns:minmax(0,1.3fr) minmax(0,1fr)"><div class="grid">{@render chartCard()}{@render heatCard()}</div><div class="grid">{@render listCard()}<AppChanges {phone} compact onall={onchanges} />{@render habitsCard()}</div></div>
{/if}
