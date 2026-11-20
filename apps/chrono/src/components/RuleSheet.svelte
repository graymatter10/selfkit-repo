<script lang="ts">
  // Time rule form (app.rule): "on these days from–to, <target> counts as
  // <category>". Bottom sheet on phones, dialog on desktop. Opened from a
  // website's detail and from Settings → Categories. Saves into
  // chrono_settings.category_rules (resolution: src-tauri/src/categories.rs).
  import type { CategoryRule, ChronoSettings } from "../lib/api";
  import { CATS, catColor, catName, clock, ruleDays } from "../lib/fmt";
  import { app, saveSettings } from "../lib/state.svelte";
  import Icon from "./Icon.svelte";
  import Seg from "./Seg.svelte";
  // Name suggestions come from Settings' datalists (#chrono-apps / #chrono-sites).
  let { phone }: { phone: boolean } = $props();

  const r = $derived(app.rule?.rule);
  const DAYS = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
  const toTime = (m: number) => clock(m);
  const fromTime = (v: string) => {
    const [h, m] = v.split(":").map(Number);
    return Number.isFinite(h) && Number.isFinite(m) ? h * 60 + m : 0;
  };
  function toggleDay(d: number) {
    if (!r) return;
    r.days = r.days.includes(d) ? r.days.filter((x) => x !== d) : [...r.days, d].sort();
  }
  const canSave = $derived(!!r && r.target.trim().length > 0 && r.days.length > 0);
  const base = $derived(app.rule?.base ?? "");
  const sentence = $derived.by(() => {
    if (!r) return "";
    const who = r.target.trim() || (r.kind === "domain" ? "this website" : "this app");
    const days = ruleDays(r.days);
    const on = days === "Every day" ? "Every day" : days === "Weekends" ? "On weekends" : `On ${days}`;
    const when = r.start_min === r.end_min ? (r.start_min === 0 ? "all day" : `for 24 hours from ${clock(r.start_min)}`)
      : `from ${clock(r.start_min)} to ${clock(r.end_min)}${r.end_min < r.start_min ? " the next day" : ""}`;
    return `${on} ${when}, ${who} counts as ${catName(r.category)}.`;
  });
  const clone = (): ChronoSettings => JSON.parse(JSON.stringify($state.snapshot(app.settings)));
  function close() { app.rule = null; }
  function save() {
    if (!r || !canSave || !app.settings) return;
    const rule = $state.snapshot(r) as CategoryRule;
    rule.target = rule.kind === "domain" ? rule.target.trim().toLowerCase().replace(/^https?:\/\//, "").replace(/\/.*$/, "").replace(/:\d+$/, "").replace(/^www\./, "") : rule.target.trim();
    const n = clone();
    n.category_rules ??= [];
    const i = n.category_rules.findIndex((x) => x.id === rule.id);
    if (i >= 0) n.category_rules[i] = rule;
    else n.category_rules.push(rule);
    saveSettings(n);
    close();
  }
  function remove() {
    if (!r || !app.settings) return;
    const before = clone(), n = clone(), id = r.id;
    n.category_rules = (n.category_rules ?? []).filter((x) => x.id !== id);
    close();
    saveSettings(n, "Time rule removed", before);
  }
  function key(e: KeyboardEvent) {
    if (app.rule && e.key === "Escape") { e.stopPropagation(); close(); }
  }
</script>

<svelte:window onkeydown={key} />

{#snippet body(rule: CategoryRule)}
  <div style="display:flex;align-items:center;margin-bottom:6px">
    <div style="flex:1;min-width:0">
      <h2 style="margin:0;font-weight:650;font-size:17px">Time rule</h2>
      {#if !app.rule?.isNew || rule.target}<div class="muted" style="font-size:13px;margin-top:2px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">for {rule.target}</div>{/if}
    </div>
    <button type="button" class="ib" aria-label="Close" onclick={close}><Icon name="close" /></button>
  </div>
  {#if app.rule?.isNew && !app.rule.base}
    <div style="display:flex;margin:8px 0 8px"><Seg label="Rule for" value={rule.kind} onchange={(k) => (rule.kind = k)} items={[{ id: "domain", name: "A website" }, { id: "app", name: "An app" }]} style="flex:1" /></div>
    <div class="field"><input bind:value={rule.target} list={rule.kind === "app" ? "chrono-apps" : "chrono-sites"} placeholder={rule.kind === "app" ? "App name, e.g. Telegram" : "Website, e.g. youtube.com"} aria-label="Name" /></div>
    {#if rule.kind === "domain"}<p class="hint" style="margin:6px 0 0">Also covers its subdomains.</p>{/if}
  {/if}
  <div class="sec" style="margin-top:14px">Count it as</div>
  <div class="catpick" role="radiogroup" aria-label="Category">
    {#each CATS as c}<button type="button" role="radio" aria-checked={rule.category === c} class="cp" class:on={rule.category === c} onclick={() => (rule.category = c)}><i class="sw" style="background:{catColor(c)}"></i>{catName(c)}</button>{/each}
  </div>
  <div class="sec" style="margin-top:16px;display:flex;align-items:center">On these days
    <span style="margin-left:auto;display:flex;gap:4px">
      <button type="button" class="lnk2" onclick={() => (rule.days = [1, 2, 3, 4, 5])}>Weekdays</button>
      <button type="button" class="lnk2" onclick={() => (rule.days = [1, 2, 3, 4, 5, 6, 7])}>Every day</button>
    </span>
  </div>
  <div class="daychips">
    {#each DAYS as d, i}<button type="button" class="dc" class:on={rule.days.includes(i + 1)} aria-pressed={rule.days.includes(i + 1)} aria-label={d} onclick={() => toggleDay(i + 1)}>{d.slice(0, phone ? 1 : 3)}</button>{/each}
  </div>
  <div style="display:grid;grid-template-columns:minmax(0,1fr) minmax(0,1fr);gap:10px;margin-top:16px">
    <label><div class="sec">From</div><span class="field"><input type="time" value={toTime(rule.start_min)} aria-label="From" onchange={(e) => (rule.start_min = fromTime((e.target as HTMLInputElement).value))} /></span></label>
    <label><div class="sec">To</div><span class="field"><input type="time" value={toTime(rule.end_min)} aria-label="To" onchange={(e) => (rule.end_min = fromTime((e.target as HTMLInputElement).value))} /></span></label>
  </div>
  <p class="say" style="margin:16px 0 0;font-size:14px">{sentence}{#if base && base !== rule.category} At other times it stays {catName(base)}.{/if}</p>
  <p class="hint" style="margin:6px 0 0">{rule.days.length ? "Ending earlier than it starts runs into the next day, e.g. 22:00 – 02:00." : "Pick at least one day."}</p>
  <div style="display:flex;gap:8px;margin-top:18px">
    {#if !app.rule?.isNew}<button type="button" class="btn ghost danger" style="padding:0 8px" onclick={remove}>Delete</button>{/if}
    <span style="flex:1"></span>
    <button type="button" class="btn" onclick={close}>Cancel</button>
    <button type="button" class="btn pri" disabled={!canSave} onclick={save}>Save rule</button>
  </div>
{/snippet}

{#if r}
  {#if phone}
    <button type="button" class="scrim fade" style="z-index:70" aria-label="Close" onclick={close}></button>
    <div class="sheet rise" role="dialog" aria-modal="true" aria-label="Time rule">
      <div class="grab"></div>
      {@render body(r)}
    </div>
  {:else}
    <div class="scrim fade" style="z-index:70">
      <button type="button" aria-label="Close" style="position:absolute;inset:0;cursor:default" onclick={close}></button>
      <div class="dialog" role="dialog" aria-modal="true" aria-label="Time rule" style="width:520px;position:relative">{@render body(r)}</div>
    </div>
  {/if}
{/if}
