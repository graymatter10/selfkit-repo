<script lang="ts">
  // "Last night": duration (estimate or your times), timeline, times, the
  // 1–5 rating, notes and naps, and "This week". Desktop adds the last 14
  // nights and the four stats on the right.
  import type { Night } from "../lib/api";
  import { dayShort, dur, hmd, hmParts, nightRange, nightTitle, plural } from "../lib/fmt";
  import { app, bridge, currentKey, deleteNap, rate, show, shownKey, shownNight, step } from "../lib/state.svelte";
  import Icon from "./Icon.svelte";
  import NightBars from "./NightBars.svelte";
  import Rating from "./Rating.svelte";
  import StatsGrid from "./StatsGrid.svelte";
  import Timeline from "./Timeline.svelte";

  let { desktop }: { desktop: boolean } = $props();
  const night = $derived(shownNight());
  const key = $derived(shownKey());
  const cur = $derived(currentKey());
  const dash = $derived(app.dash!);
  const goal = $derived(dash.goals.goal_min);

  /** 7 nights ending at the night on screen (or the current one). */
  const week = $derived.by(() => {
    const ns = dash.nights;
    let end = ns.findIndex((n) => n.night === key);
    if (end < 0) end = ns.length - 1;
    return ns.slice(Math.max(0, end - 6), end + 1);
  });
  // Same rule as the stats cards (model.rs `stats`): only nights with a main sleep
  // count, naps included in their total; nap-only days made the two averages differ (L4-17).
  const counted = (n: Night) => ["logged", "estimate", "allnighter"].includes(n.source);
  const weekAvg = $derived.by(() => {
    const w = week.filter(counted);
    return w.length ? Math.round(w.reduce((a, n) => a + n.asleep_min + n.nap_min, 0) / w.length) : null;
  });
  const last14 = $derived(dash.nights.slice(-14));
  const avg14 = $derived.by(() => {
    const w = last14.filter(counted);
    return w.length ? Math.round(w.reduce((a, n) => a + n.asleep_min + n.nap_min, 0) / w.length) : null;
  });

  const edit = (focus?: "bed" | "wake" | "note_before" | "note_after") => (app.edit = { kind: "adjust", night: key, focus });
  const addNap = () => (app.edit = { kind: "nap", night: key });
  const found = (n: Night) => n.bed != null && n.wake != null && n.asleep_min > 0;
  const lastUse = (n: Night) => n.estimate.activity.at(-1)?.[1] ?? null;
  const NOTE_HINT = { note_before: "caffeine, mood, on your mind", note_after: "dreams, how you feel" };
</script>

{#snippet hero(n: Night)}
  <div class="flex items-center justify-between">
    <button type="button" class="press grid h-11 w-11 place-items-center rounded-[12px] text-ink2" aria-label="Night before" title="Night before (←)" onclick={() => step(-1)}><Icon name="left" /></button>
    <div class="flex flex-col items-center gap-0.5 text-center">
      <h1 class="m-0 text-[15px] font-[650]">{nightTitle(key, cur)}</h1>
      <div class="text-[13px] text-muted">{nightRange(key)}</div>
    </div>
    <button type="button" class="press grid h-11 w-11 place-items-center rounded-[12px] text-ink2 disabled:opacity-30" aria-label="Next night" title="Next night (→)" disabled={key >= cur} onclick={() => step(1)}><Icon name="right" /></button>
  </div>
  <div class="mt-3.5 mb-4 flex flex-col items-center">
    {#if found(n)}
      {@const [h, m] = hmParts(n.asleep_min)}
      <span class="num text-[64px] leading-none font-[650] tracking-[-0.045em]" aria-label="{dur(n.asleep_min)} asleep">{h}<span class="mr-[0.26em] ml-[0.02em] text-[0.5em] font-semibold tracking-normal text-ink2">h</span>{m}<span class="ml-[0.02em] text-[0.5em] font-semibold tracking-normal text-ink2">m</span></span>
    {:else}
      <span class="text-[40px] leading-[1.1] font-[650] tracking-[-0.035em]">{n.source === "allnighter" ? "No sleep" : n.source === "ongoing" ? "Not over yet" : "No data"}</span>
    {/if}
    <div class="mt-3">
      <span class="inline-flex h-7 items-center gap-1.5 rounded-full bg-accent-soft px-[11px] text-[13px] font-semibold text-accent">
        {#if n.source === "logged"}<Icon name="edit" size={14} stroke={2.2} />Times set by you
        {:else if n.source === "estimate" && n.estimate.source !== "devices"}<Icon name="check" size={14} stroke={2.2} />From {n.estimate.source.replace("_", " ")}
        {:else if n.source === "estimate"}<Icon name="phone" size={14} stroke={2.2} />Estimated from screen use
        {:else if n.source === "allnighter"}<Icon name="phone" size={14} stroke={2.2} />Screens were on all night
        {:else if n.source === "ongoing"}<Icon name="moon" size={14} stroke={2.2} />{lastUse(n) == null ? "Sleep well" : dash.now - lastUse(n)! < 3 * 60_000 ? "You're still up" : `Last screen use ${hmd(lastUse(n), n.tz_offset)}`}
        {:else}<Icon name="phone" size={14} stroke={2.2} />No screen use recorded{/if}
      </span>
    </div>
  </div>
{/snippet}

{#snippet timeCell(label: string, v: number | null, est: number | null, tz: number)}
  <div class="flex min-w-0 flex-1 flex-col gap-1">
    <span class="text-[12.5px] text-muted">{label}</span>
    <span class="num text-[19px] font-[650] tracking-[-0.01em]">{hmd(v, tz)}</span>
    {#if est != null && v != null && est !== v}<span class="num text-[12px] text-muted">was {hmd(est, tz)}</span>{/if}
  </div>
{/snippet}

{#snippet timelineCard(n: Night)}
  <div class="card px-4 pt-4 pb-1">
    {#if app.usageAccess === false}
      <div class="mb-3 flex items-center gap-3 rounded-[12px] bg-accent-soft p-3">
        <span class="flex-1 text-[14px] leading-snug text-ink">Allow Usage access so Dusk can estimate your sleep from your last and first phone use.</span>
        <button type="button" class="press h-11 shrink-0 rounded-[12px] bg-accent px-4 text-[15px] font-semibold text-on-accent" onclick={() => bridge()?.openAccessSettings()}>Allow</button>
      </div>
    {/if}
    <Timeline night={n} />
    <div class="mt-4 flex">
      {@render timeCell("Bedtime", n.bed, n.source === "logged" ? n.estimate.bed : null, n.tz_offset)}
      {@render timeCell("Woke up", n.wake, n.source === "logged" ? n.estimate.wake : null, n.tz_offset)}
      <div class="flex min-w-0 flex-1 flex-col gap-1">
        <span class="text-[12.5px] text-muted">Woke briefly</span>
        {#if n.source === "estimate" && n.estimate.interruptions > 0}
          <span class="num text-[19px] font-[650] tracking-[-0.01em]">{plural(n.estimate.interruptions, "time")} <span class="text-[13px] font-medium whitespace-nowrap text-muted">· {n.estimate.awake_min < 1 ? "< 1" : n.estimate.awake_min} min</span></span>
        {:else}
          <span class="text-[19px] font-[650] tracking-[-0.01em]">{n.source === "estimate" ? "No" : "–"}</span>
        {/if}
      </div>
    </div>
    <div class="-mx-4 mt-3.5 flex items-center border-t border-sep px-1.5">
      <button type="button" class="press inline-flex h-11 items-center gap-1.5 px-2.5 text-[15px] font-semibold text-accent" onclick={() => edit()}><Icon name="edit" size={16} stroke={2.2} />{found(n) ? "Adjust times" : "Enter times"}</button>
      <span class="flex-1"></span>
      {#if desktop}<span class="pr-2.5 text-[12.5px] text-muted">Press A</span>
      {:else}<button type="button" class="press inline-flex h-11 items-center gap-1.5 px-2.5 text-[15px] font-semibold text-accent" onclick={addNap}><Icon name="plus" size={16} stroke={2.4} />Add nap</button>{/if}
    </div>
  </div>
{/snippet}

{#snippet noteRow(n: Night, f: "note_before" | "note_after", stacked: boolean)}
  {@const text = n.log?.[f] ?? ""}
  {@const label = f === "note_before" ? "Before bed" : "After waking"}
  <button type="button" class="hl flex min-h-[52px] w-full items-center gap-3 px-4 py-1.5 text-left {stacked ? 'items-start py-3' : ''}" onclick={() => edit(f)} aria-label="{label} note{text ? ': ' + text : ', add one'}">
    <span class="flex w-5 shrink-0 text-ink2 {stacked ? 'mt-px' : ''}"><Icon name={f === "note_before" ? "moon" : "sun"} size={18} /></span>
    {#if stacked}
      <span class="flex min-w-0 flex-1 flex-col gap-1">
        <span class="text-[13.5px] font-[550] text-muted">{label}</span>
        {#if text}<span class="text-[15px] leading-[1.35] break-words">{text}</span>
        {:else}<span class="inline-flex items-center gap-1.5 text-[14px] font-semibold text-accent"><Icon name="plus" size={14} stroke={2.4} />Add a note</span>{/if}
      </span>
      {#if text}<span class="mt-0.5 text-muted"><Icon name="edit" size={15} stroke={2} /></span>{/if}
    {:else}
      <span class="w-[92px] shrink-0 text-[13.5px] font-[550] text-muted">{label}</span>
      {#if text}<span class="min-w-0 flex-1 truncate text-[15px]">{text}</span><span class="text-muted"><Icon name="edit" size={15} stroke={2} /></span>
      {:else}<span class="inline-flex min-w-0 flex-1 items-center gap-1.5 truncate text-[14px] font-semibold text-accent"><Icon name="plus" size={14} stroke={2.4} />Add a note <span class="truncate font-normal text-muted">· {NOTE_HINT[f]}</span></span>{/if}
    {/if}
  </button>
{/snippet}

{#snippet napRows(n: Night)}
  {#each n.naps as nap (nap.id)}
    <div class="flex min-h-[52px] items-center gap-3 px-4 py-1.5">
      <span class="flex w-5 shrink-0 text-ink2"><Icon name="nap" size={18} /></span>
      <span class="{desktop ? 'w-[92px]' : 'w-10'} shrink-0 text-[13.5px] font-[550] text-muted">Nap</span>
      <span class="num min-w-0 flex-1 text-[15px] [overflow-wrap:anywhere]">{hmd(nap.bed, nap.tz_offset)}–{hmd(nap.wake, nap.tz_offset)} <span class="whitespace-nowrap text-muted">· {dur(((nap.wake ?? 0) - (nap.bed ?? 0)) / 60000)}</span></span>
      <button type="button" class="press -mr-2 grid h-11 w-11 place-items-center rounded-[12px] text-muted" aria-label="Remove nap" onclick={() => deleteNap(nap.id, n.night)}><Icon name="trash" size={18} /></button>
    </div>
    <div class="ml-12 h-px bg-sep"></div>
  {/each}
{/snippet}

{#snippet weekCard()}
  <div class="mx-0.5 mt-[26px] mb-2.5 flex items-baseline"><h2 class="h2 m-0 flex-1">This week</h2><span class="num text-[13px] text-muted">{weekAvg != null ? `Average ${dur(weekAvg)}` : ""}</span></div>
  <div class="card p-4">
    <NightBars nights={week} {goal} height={96} labels="weekday" highlight={key} onpick={show} />
    <div class="legend mt-2.5"><span><i style="background:var(--accent2)"></i>Night</span><span><i style="background:var(--faint)"></i>Nap</span><span><i style="background:var(--accent)"></i>{key === cur ? "Last night" : dayShort(key)}</span></div>
  </div>
{/snippet}

{#if night}
  {#if desktop}
    <div class="flex items-start gap-9">
      <div class="w-[460px] shrink-0 pb-8">
        {@render hero(night)}
        {@render timelineCard(night)}
        <h2 class="h2 mx-0.5 mt-6 mb-2.5">How did you sleep?</h2>
        <Rating value={night.quality} onrate={(q) => rate(key, q)} />
        <div class="card mt-3 overflow-hidden">
          <div class="flex min-h-[52px] items-center gap-3 px-4 py-1.5">
            <span class="flex w-5 shrink-0 text-ink2"><Icon name="nap" size={18} /></span>
            <span class="w-[92px] shrink-0 text-[13.5px] font-[550] text-muted">Naps</span>
            <span class="min-w-0 flex-1 text-[14.5px] text-muted">{night.naps.length ? plural(night.naps.length, "nap") + " · " + dur(night.nap_min) : `None on ${dayShort(key)}`}</span>
            <button type="button" class="press -mr-2.5 inline-flex h-11 items-center gap-1.5 px-2.5 text-[15px] font-semibold text-accent" onclick={addNap}><Icon name="plus" size={15} stroke={2.4} />Add nap</button>
          </div>
          <div class="ml-12 h-px bg-sep"></div>
          {@render napRows(night)}
          {@render noteRow(night, "note_before", false)}
          <div class="ml-12 h-px bg-sep"></div>
          {@render noteRow(night, "note_after", false)}
        </div>
        {@render weekCard()}
      </div>
      {#if !app.edit}
      <div class="flex min-w-0 flex-1 flex-col gap-3 pt-1.5 pb-8">
        <div class="card px-[18px] pt-[18px] pb-3.5">
          <div class="mb-3.5 flex items-baseline"><h2 class="h2 m-0 flex-1">Last 14 nights</h2><span class="num text-[13px] text-muted">{avg14 != null ? `Average ${dur(avg14)}` : ""}</span></div>
          <NightBars nights={last14} {goal} height={228} labels="day" dots highlight={key} onpick={show} />
          <div class="mt-3 flex items-center"><div class="legend flex-1"><span><i style="background:var(--accent2)"></i>Night</span><span><i style="background:var(--faint)"></i>Nap</span><span><i style="width:4px;height:4px;border-radius:2px;background:var(--muted);vertical-align:2px"></i>Has notes</span></div><span class="text-[12px] text-muted">Click a bar to open that night</span></div>
        </div>
        <StatsGrid {dash} />
      </div>
      {/if}
    </div>
  {:else}
    {@render hero(night)}
    {@render timelineCard(night)}
    <h2 class="h2 mx-0.5 mt-6 mb-2.5">How did you sleep?</h2>
    <Rating value={night.quality} onrate={(q) => rate(key, q)} />
    <h2 class="h2 mx-0.5 mt-6 mb-2.5">Notes</h2>
    <div class="card overflow-hidden">
      {@render napRows(night)}
      {@render noteRow(night, "note_before", true)}
      <div class="ml-12 h-px bg-sep"></div>
      {@render noteRow(night, "note_after", true)}
    </div>
    {@render weekCard()}
  {/if}
{:else}
  <p class="pt-20 text-center text-muted">{app.error || "Loading…"}</p>
{/if}
