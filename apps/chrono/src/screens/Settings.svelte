<script lang="ts">
  // Settings: tracking + pause, this phone (Android), goals and limits,
  // alerts, weekly report, never-track lists, categories, devices and
  // sources, appearance. Everything shared lives in `chrono_settings`.
  import { api, type CategoryRule, type ChronoSettings, type Goal, type Total } from "../lib/api";
  import { CATS, ago, catColor, catName, dur, goalName, ruleWhen } from "../lib/fmt";
  import { app, guard, isPaused, pause, pauseLabel, refreshAndroid, resume, saveSettings, setTheme } from "../lib/state.svelte";
  import Icon from "../components/Icon.svelte";
  import Seg from "../components/Seg.svelte";
  import { isPermissionGranted, requestPermission } from "@tauri-apps/plugin-notification";
  import { syncApi } from "../../../../ui/src/sync/api";
  import LockSettings from "../../../../ui/src/lock/LockSettings.svelte"; // App lock section
  let { phone, onsources, onsync }: { phone: boolean; onsources: () => void; onsync: () => void } = $props();
  let syncLine = $state("");
  $effect(() => { void app.rev; syncApi.status().then((st) => (syncLine = st.conflicts ? `${st.headline.split(" · ")[0]} · ${st.conflicts} to review` : st.headline)).catch(() => (syncLine = "")); });
  let names = $state<Total[]>([]);
  let q = $state("");
  let addApp = $state(""), addSite = $state("");
  let adding = $state<"" | "app" | "site">("");
  let edit = $state<Goal | null>(null);
  let editNew = $state(false);
  let showAllCats = $state(false);
  $effect(() => { void app.rev; guard(api.names()).then((n) => n && (names = n)); });
  const s = $derived(app.settings);
  const clone = (): ChronoSettings => JSON.parse(JSON.stringify($state.snapshot(app.settings)));

  /** Android 13+: alerts need the notification permission; ask when it matters. */
  let notifyBlocked = $state(false);
  async function askNotify() {
    try {
      if (!(await isPermissionGranted())) notifyBlocked = (await requestPermission()) !== "granted";
      else notifyBlocked = false;
    } catch (e) {
      console.warn("notification permission", e);
    }
  }
  // Alerts / Weekly report are on by default: say when Android blocks them (P23).
  if (/Android/i.test(navigator.userAgent)) isPermissionGranted().then((g) => (notifyBlocked = !g)).catch(() => {});
  function toggle(field: "alerts" | "weekly_report") {
    const n = clone();
    n[field] = !n[field];
    if (n[field]) askNotify();
    saveSettings(n);
  }
  function toggleGoal(id: string) {
    const n = clone();
    const g = n.goals.find((g) => g.id === id);
    if (g) g.enabled = !g.enabled;
    saveSettings(n);
  }
  function addExclusion(kind: "app" | "site") {
    let v = (kind === "app" ? addApp : addSite).trim().replace(/^https?:\/\//, "").replace(/\/.*$/, "");
    // Sites stored like the agent keeps them: lowercase, no www., no port (L3-10).
    if (kind === "site") v = v.toLowerCase().replace(/:\d+$/, "").replace(/\.$/, "").replace(/^www\./, "");
    if (!v) return void (adding = "");
    const n = clone();
    const list = kind === "app" ? n.exclude_apps : n.exclude_domains;
    if (!list.some((x) => x.toLowerCase() === v.toLowerCase())) list.push(kind === "site" ? v.toLowerCase() : v);
    saveSettings(n);
    addApp = addSite = "";
    adding = "";
  }
  function removeExclusion(kind: "app" | "site", v: string) {
    const before = clone(), n = clone();
    if (kind === "app") n.exclude_apps = n.exclude_apps.filter((x) => x !== v);
    else n.exclude_domains = n.exclude_domains.filter((x) => x !== v);
    saveSettings(n, `Tracking ${v} again`, before);
  }
  function setCat(name: string, cat: string) {
    const n = clone();
    n.categories[name] = cat;
    saveSettings(n);
  }
  let goalsEl: HTMLElement | undefined = $state();
  // From Today's goals card: scroll to this section, and open the form when there are none (P22).
  $effect(() => {
    if (!app.goalsFocus || !goalsEl || !s) return;
    app.goalsFocus = false;
    setTimeout(() => goalsEl?.scrollIntoView({ block: "start" }), 60);
    if (!s.goals.length) newGoal();
  });
  function newGoal() {
    edit = { id: `g-${Date.now().toString(36)}`, kind: "limit", scope: "category", target: "entertainment", minutes: 60, enabled: true };
    editNew = true;
  }
  function saveGoal() {
    if (!edit) return;
    const g = $state.snapshot(edit) as Goal;
    if (g.scope === "total") g.target = "";
    if ((g.scope === "app" || g.scope === "domain") && !g.target.trim()) return;
    g.minutes = Math.max(1, Math.round(g.minutes));
    const n = clone();
    const i = n.goals.findIndex((x) => x.id === g.id);
    if (i >= 0) n.goals[i] = g;
    else n.goals.push(g);
    saveSettings(n);
    edit = null;
    if (n.alerts) askNotify();
  }
  function deleteGoal() {
    if (!edit) return;
    const before = clone(), n = clone(), id = edit.id, name = goalName(edit);
    n.goals = n.goals.filter((g) => g.id !== id);
    edit = null;
    saveSettings(n, `Removed ${name}`, before);
  }
  const catRows = $derived.by(() => {
    const ql = q.trim().toLowerCase();
    const r = names.filter((t) => !ql || t.name.toLowerCase().includes(ql));
    return showAllCats || ql ? r.slice(0, 200) : r.slice(0, 6);
  });
  // Time rules (category_rules) shown under the name they apply to.
  const normDom = (d: string) => d.trim().toLowerCase().replace(/^www\./, "");
  function ruleFits(r: CategoryRule, t: Total): boolean {
    if (r.kind === "domain") return t.kind === "w" && (normDom(t.name) === r.target || normDom(t.name).endsWith("." + r.target));
    const x = r.target.toLowerCase();
    return t.kind === "a" && (t.name.toLowerCase() === x || (t.package ?? "").toLowerCase() === x);
  }
  const rules = $derived(s?.category_rules ?? []);
  const rulesOf = (t: Total) => rules.filter((r) => ruleFits(r, t));
  /** Rules whose app/site isn't in the list (not used lately, or filtered out by the search). */
  const looseRules = $derived(rules.filter((r) => !catRows.some((t) => ruleFits(r, t)) && (!q.trim() || r.target.toLowerCase().includes(q.trim().toLowerCase()))));
  function editRule(r: CategoryRule, base?: string) {
    app.rule = { rule: JSON.parse(JSON.stringify(r)), isNew: false, base };
  }
  function addRule() {
    app.rule = { rule: { id: `r-${Date.now().toString(36)}`, target: "", kind: "domain", category: "work", days: [1, 2, 3, 4, 5], start_min: 540, end_min: 1020, enabled: true }, isNew: true };
  }
  const goalDesc = (g: Goal) => `${g.kind === "limit" ? "Limit" : "Goal"} · ${dur(g.minutes * 60000)} a day`;
  const appNames = $derived(names.filter((t) => t.kind === "a").map((t) => t.name));
  const siteNames = $derived(names.filter((t) => t.kind === "w").map((t) => t.name));
  const PRESETS = [15, 30, 45, 60, 90, 120, 180, 480];
</script>

{#snippet tg(on: boolean, label: string, fn: () => void)}
  <button type="button" class="tgwrap" role="switch" aria-checked={on} aria-label={label} onclick={fn}><span class="tg" class:off={!on}></span></button>
{/snippet}

{#snippet track()}
  <div class="card">
    <div class="ch"><h2>Tracking</h2></div>
    <div style="display:flex;align-items:center;gap:10px;margin-bottom:16px">
      <span class="live" class:off={isPaused()}></span><span style="font-weight:600;flex:1">{isPaused() ? pauseLabel() : "Recording on all devices"}</span>
      {#if isPaused()}<button type="button" class="btn pri" style="height:40px" onclick={resume}><Icon name="play" size={15} /> Resume</button>{/if}
    </div>
    <div class="sec">{isPaused() ? "Change the pause" : "Pause for"}</div>
    <div style="display:grid;grid-template-columns:{phone ? '1fr 1fr' : 'repeat(4,auto)'};justify-content:start;gap:8px">
      <button type="button" class="btn" onclick={() => pause(15)}>15 min</button>
      <button type="button" class="btn" onclick={() => pause(60)}>1 hour</button>
      <button type="button" class="btn" onclick={() => pause("tomorrow")}>Until tomorrow</button>
      <button type="button" class="btn" onclick={() => pause(null)}>Until I resume</button>
    </div>
    <div style="display:flex;gap:10px;margin-top:18px;padding-top:14px;border-top:1px solid var(--line2)">
      <span class="muted" style="display:flex;margin-top:1px"><Icon name="shield" size={17} /></span>
      <p class="hint" style="margin:0">Chrono saves app names and website domains only — never window titles, page contents or private tabs. Time away from the screen is not counted.</p>
    </div>
  </div>
{/snippet}

{#snippet androidCard()}
  {#if app.android.supported}
    <div class="card">
      <div class="ch" style="margin-bottom:2px"><h2>This phone</h2></div>
      <div class="row" style="min-height:60px"><div style="flex:1"><div class="nm">Usage access</div><div class="sb">App time, opens, screen-on and unlocks</div></div>
        {#if app.android.usage}<span class="ok"><Icon name="check" size={16} stroke={2.4} /> Allowed</span>{:else}<button type="button" class="btn" onclick={() => (app.setup = 1)}>Allow</button>{/if}</div>
      <div class="row" style="min-height:60px"><div style="flex:1"><div class="nm">Notification counts</div><div class="sb">How many per app, never what they say</div></div>
        {#if app.android.notifications}<button type="button" class="ok" style="min-height:44px" onclick={() => api.androidOpen("notifications")}><Icon name="check" size={16} stroke={2.4} /> On</button>{:else}<button type="button" class="btn" onclick={() => (app.setup = 2)}>Turn on</button>{/if}</div>
      <div class="row" style="min-height:60px"><div style="flex:1"><div class="nm">Last import</div><div class="sb">When you open Chrono and every 15 min in the background</div></div>
        <button type="button" class="muted num" style="font-size:14px;min-height:44px" onclick={refreshAndroid}>{ago(app.android.last_import, app.now)}</button></div>
    </div>
  {/if}
{/snippet}

{#snippet goalForm(g: Goal)}
  <div class="sel fade" style="display:block;margin:6px 0 10px;padding:14px">
    <div style="display:flex;margin-bottom:10px"><Seg label="Type" value={g.kind} onchange={(k) => (g.kind = k)} items={[{ id: "limit", name: "Limit (stay under)" }, { id: "goal", name: "Goal (reach)" }]} style="flex:1" /></div>
    <div class="sec">For</div>
    <div class="field" style="margin-bottom:10px">
      <select bind:value={g.scope} aria-label="Applies to" onchange={() => (g.target = g.scope === "category" ? "entertainment" : "")}>
        <option value="total">All screen time</option><option value="category">A category</option><option value="app">An app</option><option value="domain">A website</option>
      </select>
    </div>
    {#if g.scope === "category"}
      <div class="field" style="margin-bottom:10px"><select bind:value={g.target} aria-label="Category">{#each CATS as c}<option value={c}>{catName(c)}</option>{/each}</select></div>
    {:else if g.scope !== "total"}
      <div class="field" style="margin-bottom:10px"><input bind:value={g.target} list={g.scope === "app" ? "chrono-apps" : "chrono-sites"} placeholder={g.scope === "app" ? "App name, e.g. Instagram" : "Website, e.g. youtube.com"} aria-label="Name" /></div>
    {/if}
    <div class="sec">Minutes a day · {dur(g.minutes * 60000)}</div>
    <div style="display:flex;flex-wrap:wrap;gap:6px;margin-bottom:12px">
      {#each PRESETS as m}<button type="button" class="chip" style={g.minutes === m ? "background:var(--accent-soft);color:var(--accent-ink);font-weight:600" : ""} onclick={() => (g.minutes = m)}>{dur(m * 60000)}</button>{/each}
      <span class="field" style="width:110px;height:36px"><input type="number" min="1" max="1440" bind:value={g.minutes} aria-label="Minutes" />min</span>
    </div>
    <div style="display:flex;gap:8px">
      {#if !editNew}<button type="button" class="btn ghost danger" style="padding:0 8px" onclick={deleteGoal}>Delete</button>{/if}
      <span style="flex:1"></span>
      <button type="button" class="btn" onclick={() => (edit = null)}>Cancel</button>
      <button type="button" class="btn pri" onclick={saveGoal}>Save</button>
    </div>
  </div>
{/snippet}

{#snippet goalsCard()}
  {#if s}
    <div class="card">
      <div class="ch" style="margin-bottom:2px" bind:this={goalsEl}><h2>Goals and limits</h2><button type="button" class="lnk" onclick={newGoal}><Icon name="plus" size={14} stroke={2.2} /> New</button></div>
      {#if edit && editNew}{@render goalForm(edit)}{/if}
      {#each s.goals as g (g.id)}
        {#if edit && !editNew && edit.id === g.id}{@render goalForm(edit)}
        {:else}
          <div class="row" style="min-height:56px">
            <button type="button" style="flex:1;min-height:52px;text-align:left" onclick={() => ((edit = JSON.parse(JSON.stringify(g))), (editNew = false))}><div class="nm">{goalName(g)}</div><div class="sb">{goalDesc(g)}</div></button>
            {@render tg(g.enabled, `${goalName(g)} on`, () => toggleGoal(g.id))}
          </div>
        {/if}
      {/each}
      {#if !s.goals.length && !edit}<p class="hint" style="margin:8px 0 6px">No goals yet. Add a daily limit or a goal.</p>{/if}
      <div class="row" style="min-height:60px"><div style="flex:1"><div class="nm">Alerts</div><div class="sb">{s.alerts && notifyBlocked ? "Notifications are off for Chrono in Android settings, so nothing will show" : "Notify me when a limit is reached or a goal is met"}</div></div>{@render tg(s.alerts, "Alerts", () => toggle("alerts"))}</div>
      <div class="row" style="min-height:60px"><div style="flex:1"><div class="nm">Weekly report</div><div class="sb">{s.weekly_report && notifyBlocked ? "Notifications are off for Chrono in Android settings" : "Ready every Monday morning"}</div></div>{@render tg(s.weekly_report, "Weekly report", () => toggle("weekly_report"))}</div>
    </div>
  {/if}
{/snippet}

{#snippet exclCard()}
  {#if s}
    <div class="card">
      <div class="ch"><h2>Never track</h2></div>
      <div class="sec">Apps</div>
      <div style="display:flex;flex-wrap:wrap;gap:8px;margin-bottom:14px">
        {#each s.exclude_apps as a (a)}<span class="chip">{a}<button type="button" aria-label="Track {a} again" style="display:grid;place-items:center;width:28px;height:36px;margin-right:-6px" onclick={() => removeExclusion("app", a)}><Icon name="close" size={15} /></button></span>{/each}
        {#if adding === "app"}
          <form class="field" style="height:36px;width:220px" onsubmit={(e) => (e.preventDefault(), addExclusion("app"))}><!-- svelte-ignore a11y_autofocus --><input autofocus bind:value={addApp} list="chrono-apps" placeholder="App name" aria-label="App name" onblur={() => addExclusion("app")} /></form>
        {:else}<button type="button" class="chip add" onclick={() => (adding = "app")}><Icon name="plus" size={15} /> Add app</button>{/if}
      </div>
      <div class="sec">Websites, including their subdomains</div>
      <div style="display:flex;flex-wrap:wrap;gap:8px">
        {#each s.exclude_domains as d (d)}<span class="chip">{d}<button type="button" aria-label="Track {d} again" style="display:grid;place-items:center;width:28px;height:36px;margin-right:-6px" onclick={() => removeExclusion("site", d)}><Icon name="close" size={15} /></button></span>{/each}
        {#if adding === "site"}
          <form class="field" style="height:36px;width:220px" onsubmit={(e) => (e.preventDefault(), addExclusion("site"))}><!-- svelte-ignore a11y_autofocus --><input autofocus bind:value={addSite} list="chrono-sites" placeholder="example.com" aria-label="Website" onblur={() => addExclusion("site")} /></form>
        {:else}<button type="button" class="chip add" onclick={() => (adding = "site")}><Icon name="plus" size={15} /> Add website</button>{/if}
      </div>
      <p class="hint" style="margin:14px 0 0">Past time stays in your history but is hidden everywhere.</p>
    </div>
  {/if}
{/snippet}

{#snippet ruleLine(r: CategoryRule, base: string | undefined, label: string)}
  <button type="button" class="rulesub" class:off={!r.enabled} onclick={() => editRule(r, base)}>
    <span class="ln"></span><Icon name="clock" size={15} /><i class="sw" style="width:8px;height:8px;background:{catColor(r.category)}"></i>
    <span style="flex:1;min-width:0">{label ? `${label} · ` : ""}{catName(r.category)} · {ruleWhen(r)}</span>{#if !r.enabled}<span class="tag">Off</span>{/if}
  </button>
{/snippet}

{#snippet catsCard()}
  <div class="card">
    <div class="ch"><h2>Categories</h2><span class="r">used for Focus</span></div>
    <label class="field" style="margin-bottom:6px"><Icon name="search" size={17} /><input bind:value={q} placeholder="Find an app or website" aria-label="Find an app or website" /></label>
    {#each catRows as t (t.kind + t.name)}
      <div class="row"><span class="nm" style="flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">{t.name} <span class="muted" style="font-size:13px">{t.kind === "w" ? "website" : ""}</span></span>
        <i class="sw" style="background:{catColor(t.cat)}"></i>
        <select class="chip" value={t.cat} aria-label="Category of {t.name}" onchange={(e) => setCat(t.name, (e.target as HTMLSelectElement).value)}>{#each CATS as c}<option value={c}>{catName(c)}</option>{/each}</select>
      </div>
      {#each rulesOf(t) as r (r.id)}{@render ruleLine(r, t.cat, r.target !== t.name ? r.target : "")}{/each}
    {/each}
    {#each looseRules as r (r.id)}{@render ruleLine(r, undefined, r.target)}{/each}
    {#if !q && names.length > 6}<button type="button" class="row" style="min-height:48px;justify-content:center;color:var(--accent-ink);font-weight:600;font-size:14px" onclick={() => (showAllCats = !showAllCats)}>{showAllCats ? "Show fewer" : `Show all ${names.length}`}</button>{/if}
    {#if !names.length}<p class="hint" style="margin:8px 0 0">Apps and websites appear here once they have been used.</p>{/if}
    <button type="button" class="row" style="min-height:48px;color:var(--accent-ink);font-weight:600;font-size:14px;gap:6px" onclick={addRule}><Icon name="plus" size={15} stroke={2.2} /> Add time rule</button>
    <p class="hint" style="margin:6px 0 0">A time rule changes an app's or website's category on certain days and hours. It counts in totals, Focus, goals and alerts.</p>
  </div>
{/snippet}

{#snippet sourcesCard()}
  <div class="card" style="padding-top:6px;padding-bottom:6px">
    <button type="button" class="row" style="min-height:52px" onclick={onsources}><div style="flex:1"><div class="nm" style="font-weight:600;font-size:14px">Devices and sources</div><div class="sb">Where Chrono gets its data · rename devices</div></div><span class="muted"><Icon name="right" /></span></button>
  </div>
{/snippet}

{#snippet syncCard()}
  <div class="card" style="padding-top:6px;padding-bottom:6px">
    <button type="button" class="row" style="min-height:52px" onclick={onsync}><div style="flex:1;min-width:0"><div class="nm" style="font-weight:600;font-size:14px">Sync</div><div class="sb">{syncLine || "Your devices, server and encryption"}</div></div><span class="muted"><Icon name="right" /></span></button>
  </div>
{/snippet}

{#snippet themeCard()}
  <div class="card"><div class="ch"><h2>Appearance</h2></div>
    <Seg label="Appearance" value={app.theme} onchange={setTheme} items={[{ id: "system", name: "System" }, { id: "light", name: "Light" }, { id: "dark", name: "Dark" }]} />
    <div class="hint" style="margin-top:10px">Shared by all Selfkit apps.</div></div>
{/snippet}

<datalist id="chrono-apps">{#each appNames as n}<option value={n}></option>{/each}</datalist>
<datalist id="chrono-sites">{#each siteNames as n}<option value={n}></option>{/each}</datalist>
{#if phone}
  <div class="grid">{@render syncCard()}<LockSettings {phone} appName="Chrono" />{@render track()}{@render androidCard()}{@render goalsCard()}{@render exclCard()}{@render catsCard()}{@render sourcesCard()}{@render themeCard()}</div>
{:else}
  <div class="grid" style="grid-template-columns:minmax(0,1fr) minmax(0,1fr)"><div class="grid">{@render track()}{@render exclCard()}{@render catsCard()}</div><div class="grid">{@render syncCard()}<LockSettings {phone} appName="Chrono" />{@render goalsCard()}{@render androidCard()}{@render sourcesCard()}{@render themeCard()}</div></div>
{/if}
