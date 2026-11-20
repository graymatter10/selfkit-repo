<script lang="ts">
  // Websites (mockup 2026-11-21): every domain with time in the chosen day /
  // week / month, its browsers and devices, and its category (time rules
  // shown as their own share). Tap a site → detail: by day and hour, category,
  // time rules, where, daily limit, never track. Phone: detail is a full page;
  // desktop: right pane. Data: `websites` / `site` commands (stats.rs).
  import { api, type ChronoSettings, type SiteDetail, type SiteRow, type WebsitesView } from "../lib/api";
  import { CATS, browserName, catColor, catName, dur, ruleWhen } from "../lib/fmt";
  import { app, guard, saveSettings, today } from "../lib/state.svelte";
  import Icon from "../components/Icon.svelte";
  import Seg from "../components/Seg.svelte";
  let { phone, onsources }: { phone: boolean; onsources: () => void } = $props();

  let v = $state<WebsitesView | null>(null);
  let sd = $state<SiteDetail | null>(null);
  let q = $state("");
  let showAll = $state(false);
  let pickDay = $state<number | null>(null);
  let limitOpen = $state(false);

  $effect(() => {
    void app.rev;
    const p = app.web.period, a = app.web.anchor ?? undefined;
    guard(api.websites(p, a)).then((r) => {
      if (!r) return;
      v = r;
      app.web.title = r.title;
      app.web.prev = r.prev_anchor;
      app.web.next = r.next_anchor;
      if (!phone && !app.site && r.sites.length) app.site = r.sites[0].name;
    });
  });
  $effect(() => {
    void app.rev;
    const name = app.site, p = app.web.period, a = app.web.anchor ?? undefined;
    pickDay = null;
    limitOpen = false;
    if (!name) return void (sd = null);
    guard(api.site(name, p, a)).then((r) => r && app.site === name && (sd = r));
  });

  const ql = $derived(q.trim().toLowerCase());
  const filtered = $derived(v ? v.sites.filter((s) => !ql || s.name.includes(ql)) : []);
  const limit = $derived(phone ? 8 : 12);
  const rows = $derived(showAll || ql ? filtered.slice(0, 400) : filtered.slice(0, limit));
  const max = $derived(Math.max(1, ...(v?.sites.map((s) => s.ms) ?? [1])));
  const periodWord = $derived(app.web.period === "day" ? "day" : app.web.period === "week" ? "week" : "month");
  const prevWord = $derived(app.web.period === "day" ? "the day before" : app.web.period === "week" ? "last week" : "last month");

  function change(cur: number, prev: number): string {
    if (!prev) return "";
    const d = cur - prev;
    if (Math.abs(d) < 60000 * 5) return `, about the same as ${prevWord}`;
    return `, ${dur(Math.abs(d))} ${d < 0 ? "less" : "more"} than ${prevWord}`;
  }
  const topCat = $derived.by(() => {
    if (!v) return "";
    const e = Object.entries(v.cats).sort((a, b) => b[1] - a[1])[0];
    return e && e[1] > 0 ? e[0] : "";
  });
  const catParts = $derived(v ? CATS.map((c) => [c, v!.cats[c] ?? 0] as const).filter(([, ms]) => ms > 0) : []);
  const pctOf = (ms: number, tot: number) => (tot > 0 ? Math.round((ms / tot) * 100) : 0);
  const whereLine = (s: SiteRow) => `${s.browsers.map(browserName).join(", ")}${s.devices.length ? " · " + s.devices.join(", ") : ""}`;
  const ruled = (s: { rules: Record<string, number> }) => Object.entries(s.rules ?? {}).filter(([, ms]) => ms > 0);
  const total = (s: { ms: number }) => s.ms;

  // ---- detail actions ----------------------------------------------------
  const clone = (): ChronoSettings => JSON.parse(JSON.stringify($state.snapshot(app.settings)));
  function setCat(c: string) {
    if (!sd || !app.settings) return;
    const n = clone();
    n.categories[sd.name] = c;
    saveSettings(n);
  }
  function newRule() {
    if (!sd) return;
    app.rule = {
      rule: { id: `r-${Date.now().toString(36)}`, target: sd.name, kind: "domain", category: sd.cat === "work" ? "entertainment" : "work", days: [1, 2, 3, 4, 5], start_min: 540, end_min: 1020, enabled: true },
      isNew: true,
      base: sd.cat,
    };
  }
  function editRule(id: string) {
    const r = sd?.rules.find((x) => x.id === id);
    if (r && sd) app.rule = { rule: JSON.parse(JSON.stringify(r)), isNew: false, base: sd.cat };
  }
  function toggleRule(id: string) {
    const n = clone();
    const r = n.category_rules?.find((x) => x.id === id);
    if (r) r.enabled = !r.enabled;
    saveSettings(n);
  }
  const goal = $derived(sd?.goals.find((g) => g.kind === "limit") ?? null);
  function setLimit(m: number | null) {
    if (!sd) return;
    const before = clone(), n = clone(), name = sd.name;
    const i = n.goals.findIndex((g) => g.id === goal?.id);
    if (m == null) {
      if (i >= 0) n.goals.splice(i, 1);
      saveSettings(n, `Removed the limit for ${name}`, before);
    } else {
      if (i >= 0) n.goals[i] = { ...n.goals[i], minutes: m, enabled: true };
      else n.goals.push({ id: `g-${Date.now().toString(36)}`, kind: "limit", scope: "domain", target: name, minutes: m, enabled: true, created: today() });
      saveSettings(n);
    }
    limitOpen = false;
  }
  function neverTrack() {
    if (!sd) return;
    const before = clone(), n = clone(), name = sd.name;
    if (!n.exclude_domains.includes(name)) n.exclude_domains.push(name);
    app.site = null;
    saveSettings(n, `${name} won't be tracked`, before);
  }
  const dayMax = $derived(Math.max(1, ...(sd?.days.map((d) => Object.values(d.cats).reduce((a, b) => a + b, 0)) ?? [1])));
  const hourMax = $derived(Math.max(1, ...(sd?.hours ?? [1])));
  const busiest = $derived.by(() => {
    if (!sd || !sd.ms) return null;
    const h = sd.hours.indexOf(Math.max(...sd.hours));
    return `${h}–${h + 1}`;
  });
  const dayOrder = (cats: Record<string, number>) => CATS.filter((c) => cats[c] > 0).concat(Object.keys(cats).filter((c) => !(CATS as readonly string[]).includes(c) && cats[c] > 0));
  const showLabel = (i: number, n: number) => n <= 7 || i === 0 || (i + 1) % 5 === 0;
  const PRESETS = [15, 30, 60, 90, 120];
</script>

{#snippet catBar(parts: (readonly [string, number])[], tot: number)}
  <div class="stack" role="img" aria-label={parts.map(([c, ms]) => `${catName(c)} ${pctOf(ms, tot)}%`).join(", ")}>
    {#each parts as [c, ms]}<i style="width:{pctOf(ms, tot)}%;background:{catColor(c)}"></i>{/each}
  </div>
{/snippet}

{#snippet summary(w: WebsitesView)}
  <div class="card">
    <div class="mid">{dur(w.total)} <span class="muted" style="font-size:16px;letter-spacing:0">on {w.sites.length} {w.sites.length === 1 ? "website" : "websites"}</span></div>
    <div class="say">
      {#if w.period === "day"}In the browser this day{change(w.total, w.prev_total)}.
      {:else}About <b>{dur(w.total / w.days)} a day</b> in the browser{change(w.total, w.prev_total)}.{/if}
      {#if topCat} Most of it was {catName(topCat)}.{/if}
    </div>
    {#if catParts.length}
      <div style="margin-top:16px">{@render catBar(catParts, w.total)}</div>
      <div class="legend" style="margin-top:12px">{#each catParts as [c, ms]}<span><i class="sw" style="background:{catColor(c)}"></i>{catName(c)} {pctOf(ms, w.total)}%</span>{/each}</div>
    {/if}
  </div>
{/snippet}

{#snippet siteRow(s: SiteRow)}
  {@const sel = !phone && app.site === s.name}
  {@const rs = ruled(s)}
  {@const base = s.ms - rs.reduce((a, [, ms]) => a + ms, 0)}
  <button type="button" class="row" class:sel style="padding:8px {sel ? '8px' : '0'};border-radius:{sel ? '10px' : '0'};min-height:72px;align-items:flex-start" aria-current={sel ? "true" : undefined} onclick={() => (app.site = s.name)}>
    <div style="flex:1;min-width:0">
      <div style="display:flex;align-items:baseline;gap:8px;min-width:0"><span class="nm" style="flex:1;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">{s.name}</span><span class="val">{dur(total(s))}</span></div>
      <div class="bar" style="height:5px;display:flex;gap:2px;margin-top:7px;background:none">
        <i style="width:{(base / max) * 100}%;background:{catColor(s.cat)}"></i>
        {#each rs as [c, ms]}<i style="width:{(ms / max) * 100}%;background:{catColor(c)}"></i>{/each}
        <span style="flex:1;background:var(--track);border-radius:6px"></span>
      </div>
      <div class="muted" style="font-size:12.5px;margin-top:6px;display:flex;flex-wrap:wrap;gap:4px 12px">
        <span style="display:inline-flex;align-items:center;gap:5px"><i class="sw" style="width:8px;height:8px;background:{catColor(s.cat)}"></i>{catName(s.cat)}</span>
        {#each rs as [c, ms]}<span style="display:inline-flex;align-items:center;gap:5px"><i class="sw" style="width:8px;height:8px;background:{catColor(c)}"></i>{catName(c)} {dur(ms)} by time rule</span>{/each}
        <span>{whereLine(s)}</span>
      </div>
    </div>
  </button>
{/snippet}

{#snippet list(w: WebsitesView)}
  <div class="card" style="padding-top:10px;padding-bottom:6px">
    <label class="field" style="margin:4px 0 6px"><Icon name="search" size={17} /><input bind:value={q} placeholder="Find a website" aria-label="Find a website" /></label>
    {#each rows as s (s.name)}{@render siteRow(s)}{/each}
    {#if !rows.length}<p class="hint" style="margin:10px 0 12px">{ql ? `No website matching “${q.trim()}” this ${periodWord}.` : `No website time this ${periodWord}.`}</p>{/if}
    {#if !ql && filtered.length > limit}<button type="button" class="row" style="min-height:48px;justify-content:center;color:var(--accent-ink);font-weight:600;font-size:14px" onclick={() => (showAll = !showAll)}>{showAll ? "Show fewer" : `Show all ${filtered.length}`}</button>{/if}
  </div>
{/snippet}

{#snippet empty()}
  <div class="card empty">
    <div class="gl"><Icon name="globe" size={30} stroke={1.6} /></div>
    <h2 style="font-weight:650;font-size:17px;margin:0">No website time yet</h2>
    <p class="say" style="max-width:320px;margin:10px auto 0">Chrono counts websites with the Selfkit extension for Firefox and Chrome on your computer. It saves the site name only, like youtube.com — never the pages you open or private windows.</p>
    <button type="button" class="btn" style="margin-top:18px" onclick={onsources}>How to add the extension</button>
  </div>
{/snippet}

{#snippet detail(d: SiteDetail)}
  {@const ruledMs = Object.entries(d.cats).filter(([c]) => c !== d.cat).reduce((a, [, ms]) => a + ms, 0)}
  <div class="card">
    <div style="display:flex;align-items:center;gap:12px;margin-bottom:14px">
      <div class="sitemark" aria-hidden="true">{d.name.replace(/^(www|m)\./, "").charAt(0)}</div>
      <div style="min-width:0"><div style="font-weight:650;font-size:17px;overflow:hidden;text-overflow:ellipsis">{d.name}</div><div class="muted" style="font-size:13px">{d.title}</div></div>
    </div>
    <div class="mid">{dur(d.ms)} <span class="muted" style="font-size:16px;letter-spacing:0">{app.web.period === "day" ? "this day" : `this ${periodWord}`}</span></div>
    <div class="say">
      {#if !d.ms}No time on this site in this {periodWord}.
      {:else if app.web.period !== "day"}About <b>{dur(d.ms / d.days_counted)} a day</b>{change(d.ms, d.prev)}.
      {:else}{change(d.ms, d.prev).replace(/^, /, "").replace(/^./, (c) => c.toUpperCase()) || "In the browser this day"}.{/if}
      {#if ruledMs > 0} <b>{dur(ruledMs)}</b> of it counted as {Object.keys(d.cats).filter((c) => c !== d.cat).map(catName).join(" and ")} because of your time rule.{/if}
    </div>
    {#if d.days.length > 1 && d.ms}
      <div class="sec" style="margin-top:18px">By day</div>
      <div class="vb">
        {#each d.days as day, i}
          {@const t = Object.values(day.cats).reduce((a, b) => a + b, 0)}
          <button type="button" class="c" aria-label="{day.label}: {dur(t)}" style={pickDay === i ? "outline:2px solid var(--accent);outline-offset:2px;border-radius:4px" : ""} onclick={() => (pickDay = pickDay === i ? null : i)}>
            {#each dayOrder(day.cats) as c}<i style="height:{(day.cats[c] / dayMax) * 100}%;background:{catColor(c)}"></i>{/each}
            {#if !t}<i style="height:2px;background:var(--track)"></i>{/if}
          </button>
        {/each}
      </div>
      <div class="vbl">{#each d.days as day, i}<span>{showLabel(i, d.days.length) ? day.label : ""}</span>{/each}</div>
      {#if pickDay != null && d.days[pickDay]}
        {@const day = d.days[pickDay]}
        <p class="say" style="margin:8px 0 0;font-size:13.5px">{day.label}{d.days.length > 7 ? "" : ` ${Number(day.day.slice(8))}`}: {dur(Object.values(day.cats).reduce((a, b) => a + b, 0))}{#each dayOrder(day.cats) as c} · {catName(c)} {dur(day.cats[c])}{/each}</p>
      {/if}
      <div class="legend" style="margin-top:12px">{#each dayOrder(d.cats) as c}<span><i class="sw" style="background:{catColor(c)}"></i>{catName(c)} {dur(d.cats[c])}</span>{/each}</div>
    {/if}
    {#if d.ms}
      <div class="sec" style="margin-top:18px">By hour{busiest ? ` · busiest around ${busiest}` : ""}</div>
      <div class="hb" role="img" aria-label="Time by hour of the day">{#each d.hours as h}<i style="height:{Math.max(3, (h / hourMax) * 100)}%;{h ? '' : 'background:var(--track)'}"></i>{/each}</div>
      <div class="hbl"><span>0</span><span>6</span><span>12</span><span>18</span><span>24</span></div>
    {/if}
  </div>
  <div class="card">
    <div class="ch" style="margin-bottom:8px"><h2>Category</h2>{#if !d.cat_set}<span class="r">Chrono's guess</span>{/if}</div>
    <div class="catpick" role="radiogroup" aria-label="Category of {d.name}">
      {#each CATS as c}<button type="button" role="radio" aria-checked={d.cat === c} class="cp" class:on={d.cat === c} onclick={() => setCat(c)}><i class="sw" style="background:{catColor(c)}"></i>{catName(c)}</button>{/each}
    </div>
    <div class="sec" style="margin-top:18px">Time rules</div>
    {#each d.rules as r (r.id)}
      <div class="row" style="min-height:56px;border-top:0">
        <i class="sw" style="background:{catColor(r.category)}"></i>
        <button type="button" style="flex:1;text-align:left;min-height:52px;min-width:0" onclick={() => editRule(r.id)}><div class="nm">{catName(r.category)}{r.target !== d.name ? ` · ${r.target}` : ""}</div><div class="sb">{ruleWhen(r)}</div></button>
        <button type="button" class="tgwrap" role="switch" aria-checked={r.enabled} aria-label="Time rule {catName(r.category)} on" onclick={() => toggleRule(r.id)}><span class="tg" class:off={!r.enabled}></span></button>
      </div>
    {/each}
    <button type="button" class="chip add" style="margin-top:6px;height:44px" onclick={newRule}><Icon name="plus" size={15} /> Add a time rule</button>
    <p class="hint" style="margin:12px 0 0">{d.rules.length ? `At other times ${d.name} counts as ${catName(d.cat)}.` : "A time rule counts this site as another category on certain days and hours, e.g. Work on weekdays 09:00 – 17:00."}</p>
  </div>
  {#if d.browsers.length}
    <div class="card">
      <div class="ch" style="margin-bottom:2px"><h2>Where</h2></div>
      {#each d.browsers as [b, ms]}<div class="row"><span class="muted"><Icon name="globe" /></span><span class="nm" style="flex:1">{browserName(b)}</span><span class="val">{dur(ms)}</span></div>{/each}
      {#each d.devices as [n, ms]}<div class="row"><span class="muted"><Icon name="pc" /></span><span class="nm" style="flex:1">{n}</span><span class="val">{dur(ms)}</span></div>{/each}
    </div>
  {/if}
  <div class="card" style="padding-top:6px;padding-bottom:6px">
    <button type="button" class="row" style="min-height:56px" aria-expanded={limitOpen} onclick={() => (limitOpen = !limitOpen)}>
      <span class="muted"><Icon name="limit" size={19} /></span>
      <div style="flex:1"><div class="nm">{goal ? `Daily limit · ${dur(goal.minutes * 60000)}` : "Set a daily limit"}</div><div class="sb">{goal ? (goal.enabled ? "Alert when you reach it" : "Off · turn on in Settings") : "Get an alert when you reach it"}</div></div>
      <span class="muted"><Icon name={limitOpen ? "down" : "right"} /></span>
    </button>
    {#if limitOpen}
      <div class="fade" style="display:flex;flex-wrap:wrap;gap:6px;padding:4px 0 12px">
        {#each PRESETS as m}<button type="button" class="chip" style="height:44px;{goal?.minutes === m ? 'background:var(--accent-soft);color:var(--accent-ink);font-weight:600' : ''}" onclick={() => setLimit(m)}>{dur(m * 60000)}</button>{/each}
        {#if goal}<button type="button" class="btn ghost danger" style="height:44px;padding:0 8px" onclick={() => setLimit(null)}>Remove</button>{/if}
      </div>
    {/if}
    <button type="button" class="row" style="min-height:56px" onclick={neverTrack}>
      <span class="muted"><Icon name="no" size={19} /></span>
      <div style="flex:1"><div class="nm">Never track this site</div><div class="sb">Past time is hidden, nothing new is saved</div></div>
      <span class="muted"><Icon name="right" /></span>
    </button>
  </div>
{/snippet}

{#if v}
  {#if phone}
    <div class="grid">
      <Seg label="Period" value={app.web.period} onchange={(p) => ((app.web.period = p), (app.web.anchor = null))} items={[{ id: "day", name: "Today" }, { id: "week", name: "Week" }, { id: "month", name: "Month" }]} />
      {#if !v.has_any}{@render empty()}{:else}{@render summary(v)}{@render list(v)}{/if}
    </div>
    {#if app.site}
      <div class="page rise" style="z-index:42">
        <div class="appbar"><button type="button" class="ib" aria-label="Back" onclick={() => (app.site = null)}><Icon name="back" size={22} /></button><span class="t" style="flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">{app.site}</span></div>
        <div class="content" style="flex:1;overflow-y:auto;padding:8px 16px 24px"><div class="grid">{#if sd}{@render detail(sd)}{/if}</div></div>
      </div>
    {/if}
  {:else if !v.has_any}
    <div style="max-width:560px;margin:24px auto 0">{@render empty()}</div>
  {:else}
    <div style="display:grid;grid-template-columns:minmax(0,1fr) minmax(360px,440px);gap:20px;align-items:start">
      <div class="grid">{@render summary(v)}{@render list(v)}</div>
      <div class="grid">{#if sd}{@render detail(sd)}{:else}<div class="card"><p class="hint" style="margin:0">Choose a website to see when you used it.</p></div>{/if}</div>
    </div>
  {/if}
{/if}

<style>
  .hb { display: flex; align-items: flex-end; gap: 2px; height: 64px; }
  .hb i { flex: 1; background: var(--accent-bar); border-radius: 2px 2px 0 0; min-height: 2px; }
  .hbl { display: flex; justify-content: space-between; font-size: 12px; color: var(--muted); margin-top: 6px; }
</style>
