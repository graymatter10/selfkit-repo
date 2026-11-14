<script lang="ts">
  // Today (and any past day): calm overview. Screen time + one plain
  // sentence + day strip + first/last/longest, the note, goals, most used,
  // categories with Focus, phone counters, this week, devices.
  import { api, type DayView } from "../lib/api";
  import { devIcon, dur, hm } from "../lib/fmt";
  import { app, guard, today } from "../lib/state.svelte";
  import CatBreakdown from "../components/CatBreakdown.svelte";
  import DayRibbon from "../components/DayRibbon.svelte";
  import GoalRows from "../components/GoalRows.svelte";
  import Icon from "../components/Icon.svelte";
  import NoteCard from "../components/NoteCard.svelte";
  import TopRows from "../components/TopRows.svelte";
  let { phone, onseeall, onsettings }: { phone: boolean; onseeall: () => void; onsettings: () => void } = $props();
  let v = $state<DayView | null>(null);
  $effect(() => {
    void app.rev;
    const d = app.day ?? undefined;
    guard(api.day(d)).then((r) => r && (v = r));
  });
  const past = $derived(!!v && !v.today);
  const mins = $derived(v ? Math.round(v.screen / 60000) : 0);
  const sentence = $derived.by(() => {
    if (!v) return null;
    const avg = v.week_avg, diff = v.screen - avg;
    if (avg <= 0) return { lead: v.screen > 0 ? "Your first days with Chrono" : "Nothing recorded yet", rest: "" };
    const usual = Math.abs(diff) <= avg * 0.15;
    if (v.today && diff < 0 && !usual) return { lead: "So far today", rest: `Your daily average is ${dur(avg)}.` };
    const lead = usual ? "About a usual day for you" : diff < 0 ? "A quieter day" : "A busier day";
    return { lead, amount: `${dur(Math.abs(diff))} ${diff >= 0 ? "more" : "less"}`, avg: dur(avg) };
  });
  const phoneRows = $derived(v ? [["Screen turned on", `${v.pickups} times`], ["Unlocked", `${v.unlocks} times`], ["Notifications", `${v.notifications}`]] : []);
  const weekDiff = $derived(v ? v.week_so_far - v.last_week_same : 0);
</script>

{#snippet hero()}
  {#if v}
    <div class="card">
      <div class="ch"><h2>{past ? "Screen time" : "Screen time today"}</h2><span class="r">all devices</span></div>
      <div class="big num">{Math.floor(mins / 60)}<small>h</small>{String(mins % 60).padStart(2, "0")}<small>min</small></div>
      {#if sentence}
        <div class="say">{sentence.lead}{#if sentence.amount}{" — "}<b>{sentence.amount}</b> than your daily average ({sentence.avg}).{:else}.{sentence.rest ? " " + sentence.rest : ""}{/if}</div>
      {/if}
      <div style="margin-top:18px"><DayRibbon strip={v.strip} from={v.day_start} to={v.day_end} /></div>
      <div class="facts">
        <div><div class="v num">{hm(v.first)}</div><div class="l">First use</div></div>
        <div><div class="v num">{hm(v.last)}</div><div class="l">Last use</div></div>
        <div><div class="v num">{v.longest ? dur(v.longest[1]) : "–"}</div><div class="l" style="overflow:hidden;text-overflow:ellipsis;white-space:nowrap">Longest{v.longest ? ` · ${v.longest[0]}` : ""}</div></div>
      </div>
    </div>
  {/if}
{/snippet}

{#snippet goals()}
  {#if v}
    <div class="card">
      <div class="ch" style="margin-bottom:4px"><h2>Goals and limits</h2><button type="button" class="lnk" onclick={onsettings}>{v.goals.length ? "Edit" : "Add"}</button></div>
      {#if v.goals.length}<GoalRows goals={v.goals} {past} />
      {:else}<p class="hint" style="margin:10px 0 0">Set a daily limit (for example Entertainment, 1 h 30) or a goal (Study, 45 min). Chrono tells you when you reach it.</p>{/if}
    </div>
  {/if}
{/snippet}

{#snippet top()}
  {#if v}
    <div class="card">
      <div class="ch" style="margin-bottom:6px"><h2>Most used</h2>{#if v.top.length > (phone ? 4 : 6)}<button type="button" class="lnk" onclick={onseeall}>See all</button>{/if}</div>
      {#if v.top.length}<TopRows items={v.top.slice(0, phone ? 4 : 6)} devices={v.devices} {phone} />
      {:else}<p class="hint" style="margin:0">Nothing recorded {past ? "on this day" : "yet today"}.</p>{/if}
    </div>
  {/if}
{/snippet}

{#snippet cats()}
  {#if v && v.screen > 0}
    <div class="card"><div class="ch"><h2>Where the time went</h2></div><CatBreakdown cats={v.cats} focus={v.focus} /></div>
  {/if}
{/snippet}

{#snippet phoneCard()}
  {#if v?.phone}
    <div class="card">
      <div class="ch" style="margin-bottom:2px"><h2>Phone</h2><span class="r">{v.phone}</span></div>
      {#each phoneRows as [a, b]}<div class="row" style="min-height:44px"><span style="flex:1">{a}</span><span class="val">{b}</span></div>{/each}
    </div>
  {/if}
{/snippet}

{#snippet week()}
  {#if v && !past && v.week_so_far > 0}
    <div class="card">
      <div class="ch"><h2>This week so far</h2></div>
      <div class="mid num">{dur(v.week_so_far)}</div>
      {#if v.last_week_same > 0}<div class="say" style="margin-top:6px"><b>{dur(Math.abs(weekDiff))} {weekDiff > 0 ? "more" : "less"}</b> than the same days last week ({dur(v.last_week_same)}).</div>{/if}
    </div>
  {/if}
{/snippet}

{#snippet devices()}
  {#if v && v.devices.length}
    <div class="card">
      <div class="ch" style="margin-bottom:2px"><h2>Devices</h2></div>
      {#each v.devices as d (d.id)}
        <div class="row" style="min-height:46px"><span class="muted" style="display:flex"><Icon name={devIcon(d.platforms)} /></span><span style="flex:1">{d.name} {#if d.web && d.platforms.length > 1}<span class="muted" style="font-size:13px">incl. browser</span>{/if}</span><span class="val">{dur(d.ms)}</span></div>
      {/each}
    </div>
  {/if}
{/snippet}

{#if v}
  {@const note = { day: v.day, note: v.note, isToday: v.day === today() }}
  {#if phone}
    <div class="grid">{@render hero()}<NoteCard day={note.day} note={note.note} {phone} isToday={note.isToday} />{@render goals()}{@render top()}{@render cats()}{@render phoneCard()}{@render week()}{@render devices()}</div>
  {:else}
    <div class="grid" style="grid-template-columns:minmax(0,1.5fr) minmax(0,1fr)">
      <div class="grid">{@render hero()}<NoteCard day={note.day} note={note.note} {phone} isToday={note.isToday} />{@render top()}</div>
      <div class="grid">{@render goals()}{@render cats()}{@render phoneCard()}{@render week()}{@render devices()}</div>
    </div>
  {/if}
{/if}
