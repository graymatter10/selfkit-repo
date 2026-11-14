<script lang="ts">
  // Weekly report (Mon–Sun): average in words, the week's bars, goals kept
  // per day, where the time went, most used with changes, worth knowing.
  import { api, type Report } from "../lib/api";
  import { addDays, catColor, catName, dur, goalName, weekdayName } from "../lib/fmt";
  import { app, guard, today } from "../lib/state.svelte";
  import CatBreakdown from "../components/CatBreakdown.svelte";
  import Icon from "../components/Icon.svelte";
  import WeekChart from "../components/WeekChart.svelte";
  let { phone }: { phone: boolean } = $props();
  let r = $state<Report | null>(null);
  $effect(() => {
    void app.rev;
    const a = app.week ?? addDays(today(), -7);
    guard(api.report(a)).then((x) => x && (r = x));
  });
  const days = $derived(r ? Math.max(1, r.days_counted) : 1);
  const avg = $derived(r ? r.cur.screen / days : 0);
  const prevAvg = $derived(r ? r.prev.screen / 7 : 0);
  const DAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
  function change(ms: number, prev: number): string {
    // per-day comparison while the week is not over yet
    const p = r ? (prev * Math.max(1, r.cur.days)) / 7 : prev;
    const d = ms - p;
    prev = p;
    if (prev <= 0 || Math.abs(d) < 20 * 60000 || Math.abs(d) < prev * 0.2) return "";
    return `${dur(Math.abs(d))} ${d > 0 ? "more" : "less"}`;
  }
  const worth = $derived.by(() => {
    if (!r) return [];
    const w: [string, string][] = [];
    if (r.longest) w.push(["Longest session", `${dur(r.longest[1])} in ${r.longest[0]}, ${weekdayName(r.longest[2])}`]);
    w.push(["Late nights", r.late_days ? `After 23:00 on ${r.late_days} ${r.late_days === 1 ? "day" : "days"}` : "None this week"]);
    if (r.cur.pickups > 0) w.push(["Phone", `Turned on ${Math.round(r.cur.pickups / days)} times a day`]);
    if (r.cur.notifications > 0) w.push(["Notifications", `${Math.round(r.cur.notifications / days)} a day${r.notif_top ? `, most from ${r.notif_top[0]}` : ""}`]);
    if (r.peak_hour != null) w.push(["Busiest hour", `${r.peak_hour}:00–${r.peak_hour + 1}:00`]);
    return w;
  });
  const top = $derived(r ? [...r.apps, ...r.sites].sort((a, b) => b.ms - a.ms).slice(0, phone ? 5 : 6) : []);
</script>

{#snippet sum()}
  {#if r}
    <div class="card">
      <div class="ch"><h2>Screen time</h2><span class="r">{r.complete ? "all devices" : "week so far"}</span></div>
      <div style="display:flex;align-items:flex-end;gap:12px;flex-wrap:wrap"><div class="mid num">{dur(avg)}</div><div class="muted" style="font-size:14px;padding-bottom:3px">a day on average</div></div>
      <div class="say" style="margin-top:6px">
        {#if r.prev.screen > 0}<b>{dur(Math.abs(avg - prevAvg))} {avg <= prevAvg ? "less" : "more"}</b> a day than the week before.{/if}
        {dur(r.cur.screen)} in total.
      </div>
      <div style="margin-top:8px"><WeekChart values={r.days.map((d) => d.ms)} labels={DAYS} {phone} /></div>
      {#if r.busiest && r.quietest && r.busiest[0] !== r.quietest[0]}
        <div class="hint" style="margin-top:6px">Busiest day {weekdayName(r.busiest[0])}, quietest {weekdayName(r.quietest[0])}.</div>
      {/if}
    </div>
  {/if}
{/snippet}

{#snippet goals()}
  {#if r && r.goals.length}
    <div class="card">
      <div class="ch" style="margin-bottom:4px"><h2>Goals and limits</h2><span class="r">days kept</span></div>
      {#each r.goals as g (g.goal.id)}
        <div class="gr" style="padding:12px 0;border-top:1px solid var(--line2)">
          <div style="display:flex;align-items:baseline;margin-bottom:10px;gap:8px"><span style="font-size:15px">{goalName(g.goal)} {g.goal.kind === "limit" ? "limit" : "goal"}</span><span class="muted" style="font-size:13px">{dur(g.goal.minutes * 60000)} a day</span><span class="num" style="margin-left:auto;font-weight:600;font-size:14.5px;white-space:nowrap">{g.days_ok} of {g.days}</span></div>
          <div style="display:flex;gap:{phone ? 6 : 8}px">
            {#each g.kept as k, i}
              <div style="display:flex;flex-direction:column;align-items:center;gap:4px;width:{phone ? 30 : 32}px" title="{DAYS[i]}: {k == null ? 'not yet' : k ? 'kept' : 'not kept'}">
                <i style="width:18px;height:18px;border-radius:50%;display:grid;place-items:center;color:var(--on-accent);{k ? 'background:var(--accent)' : k === false ? 'box-shadow:inset 0 0 0 2px var(--faint)' : 'box-shadow:inset 0 0 0 2px var(--line2)'}">{#if k}<Icon name="check" size={12} stroke={3} />{/if}</i>
                <span class="muted" style="font-size:12px">{DAYS[i][0]}</span>
              </div>
            {/each}
          </div>
        </div>
      {/each}
    </div>
  {/if}
{/snippet}

{#snippet cats()}
  {#if r && r.cur.screen > 0}
    <div class="card"><div class="ch"><h2>Where the time went</h2></div>
      <CatBreakdown cats={r.cur.cats} focus={r.cur.focus} compact
        focusNote={r.prev.focus != null && r.cur.focus != null ? (r.cur.focus === r.prev.focus ? "same as the week before" : `${r.cur.focus > r.prev.focus ? "up" : "down"} from ${r.prev.focus}% the week before`) : "of your time was Work or Study"} />
    </div>
  {/if}
{/snippet}

{#snippet most()}
  {#if top.length}
    <div class="card">
      <div class="ch" style="margin-bottom:6px"><h2>Most used</h2></div>
      {#each top as t (t.kind + t.name)}
        {@const d = change(t.ms, t.prev)}
        <div class="row"><div style="flex:1;min-width:0;padding:8px 0"><div class="nm" style="display:flex;align-items:center;gap:8px">{t.name}{#if t.uninstalled}<span class="tag">Uninstalled</span>{/if}</div><div class="sb" style="display:flex;align-items:center;gap:6px"><i class="sw" style="background:{catColor(t.cat)}"></i>{catName(t.cat)}{#if d}{"\u00a0· "}<span style="color:var(--ink2)">{d}</span>{/if}</div></div><span class="val">{dur(t.ms)}</span></div>
      {/each}
    </div>
  {/if}
{/snippet}

{#snippet notes()}
  {#if worth.length && r && r.cur.screen > 0}
    <div class="card">
      <div class="ch" style="margin-bottom:2px"><h2>Worth knowing</h2></div>
      {#each worth as [a, b]}<div class="row" style="min-height:50px"><span style="width:{phone ? 120 : 150}px;flex:none" class="muted">{a}</span><span style="flex:1">{b}</span></div>{/each}
    </div>
  {/if}
{/snippet}

{#if r}
  {#if phone}
    <div class="grid">{@render sum()}{@render goals()}{@render cats()}{@render most()}{@render notes()}</div>
  {:else}
    <div class="grid" style="grid-template-columns:minmax(0,1.2fr) minmax(0,1fr)"><div class="grid">{@render sum()}{@render most()}</div><div class="grid">{@render goals()}{@render cats()}{@render notes()}</div></div>
  {/if}
{/if}
