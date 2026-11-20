<script lang="ts">
  // Settings: goals, wind-down reminder (off until turned on), day start,
  // appearance, data sources. Every row says what it is for.
  import { clock, dur, tdisp, toMin } from "../lib/fmt";
  import { app, bridge, importUsage, refresh, saveGoals, savePrefs, saveSettings } from "../lib/state.svelte";
  import Icon from "./Icon.svelte";
  import Seg from "./Seg.svelte";
  import TimeSheet from "./TimeSheet.svelte";

  import { syncApi } from "../../../../ui/src/sync/api";
  import LockSettings from "../../../../ui/src/lock/LockSettings.svelte"; // App lock section
  let { desktop }: { desktop: boolean } = $props();
  let syncLine = $state("");
  $effect(() => { syncApi.status().then((st) => (syncLine = st.conflicts ? `${st.headline.split(" · ")[0]} · ${st.conflicts} to review` : st.headline)).catch(() => (syncLine = "")); });
  const goals = $derived(app.dash?.goals ?? { goal_min: 480, bedtime: "23:00", winddown_min: 30 });
  let bedDial = $state(app.demo.dial === "goal");
  let shortcut = $state("");
  let shortcutErr = $state("");
  $effect(() => {
    shortcut = app.settings?.shortcuts?.rate_sleep ?? "";
  });
  const winddownAt = $derived(tdisp(clock(toMin(goals.bedtime) - goals.winddown_min)));

  async function setGoal(by: number) {
    const g = Math.min(840, Math.max(180, goals.goal_min + by));
    await saveGoals({ ...goals, goal_min: g });
  }
  async function setBedtime(t: string) {
    bedDial = false;
    if (t !== goals.bedtime) await saveGoals({ ...goals, bedtime: t });
  }
  async function setShortcut() {
    if (!app.settings) return;
    const s = { ...app.settings, shortcuts: { ...app.settings.shortcuts, rate_sleep: shortcut.trim() } };
    const clash = Object.entries(s.shortcuts).find(([k, v]) => k !== "rate_sleep" && v && v.toLowerCase().replace(/\s/g, "") === shortcut.trim().toLowerCase().replace(/\s/g, ""));
    if (clash) return void (shortcutErr = `Already used for ${clash[0].replace("_", " ")}`);
    shortcutErr = "";
    await saveSettings(s);
  }
  const DAY_STARTS = [0, 60, 120, 180, 240, 300, 360];
</script>

{#snippet lrow(label: string, sub: string)}
  <span class="flex min-w-0 flex-1 flex-col gap-0.5">{label}<small class="text-[13px] text-muted">{sub}</small></span>
{/snippet}

{#snippet colA()}
  <div>
    <h2 class="mx-1 mt-2 mb-2 text-[13.5px] font-[650] text-ink2">Goals</h2>
    <div class="card">
      <div class="flex min-h-[60px] items-center gap-3 px-4 py-2 text-[15.5px]">
        {@render lrow("Sleep goal", "Used for sleep debt")}
        <span class="flex items-center gap-2">
          <button type="button" class="press grid h-11 w-11 place-items-center rounded-[12px] bg-fill text-[18px] font-semibold text-ink2" aria-label="15 minutes less" onclick={() => setGoal(-15)}>−</button>
          <span class="num w-[66px] text-center font-[650]">{dur(goals.goal_min)}</span>
          <button type="button" class="press grid h-11 w-11 place-items-center rounded-[12px] bg-fill text-[18px] font-semibold text-ink2" aria-label="15 minutes more" onclick={() => setGoal(15)}>+</button>
        </span>
      </div>
      <div class="h-px bg-sep"></div>
      <div class="flex min-h-[60px] items-center gap-3 px-4 py-2 text-[15.5px]">
        <span class="flex min-w-0 flex-1 flex-col gap-0.5">Bedtime goal<small class="text-[13px] text-muted">In bed by this time</small></span>
        <button type="button" id="bedgoal" aria-haspopup="dialog" aria-label="Bedtime goal {tdisp(goals.bedtime)}. Pick on the clock" onclick={() => (bedDial = true)}
          class="press num h-11 min-w-24 rounded-[12px] px-3 text-center text-[17px] font-semibold {bedDial ? 'bg-card shadow-[inset_0_0_0_2px_var(--accent)]' : 'bg-fill'}">{tdisp(goals.bedtime)}</button>
      </div>
    </div>

    <h2 class="mx-1 mt-6 mb-2 text-[13.5px] font-[650] text-ink2">Wind-down reminder</h2>
    <div class="card">
      <label class="flex min-h-[60px] items-center gap-3 px-4 py-2 text-[15.5px]">
        {@render lrow("Remind me to wind down", app.prefs.winddown_on ? `Every day at ${winddownAt}` : `Off · would remind you at ${winddownAt}`)}
        <input type="checkbox" class="sr" checked={app.prefs.winddown_on} onchange={(e) => savePrefs({ ...app.prefs, winddown_on: (e.currentTarget as HTMLInputElement).checked })} />
        <span aria-hidden="true" class="relative inline-block h-7 w-12 shrink-0 rounded-full transition-colors duration-150 {app.prefs.winddown_on ? 'bg-accent' : 'bg-faint'}">
          <span class="absolute top-1 h-5 w-5 rounded-full transition-transform duration-150 {app.prefs.winddown_on ? 'translate-x-6 bg-on-accent' : 'translate-x-1 bg-white'}"></span>
        </span>
      </label>
      <div class="h-px bg-sep"></div>
      <div class="px-4 pt-3 pb-4">
        <div class="mb-2 text-[13px] text-muted">How long before bedtime</div>
        <Seg label="How long before bedtime" items={[{ id: 15, name: "15 min" }, { id: 30, name: "30 min" }, { id: 45, name: "45 min" }, { id: 60, name: "1 hour" }]}
          value={goals.winddown_min} onchange={(v) => saveGoals({ ...goals, winddown_min: v })} />
      </div>
    </div>
  </div>
{/snippet}

{#snippet colB()}
  <div>
    <h2 class="mx-1 mt-2 mb-2 text-[13.5px] font-[650] text-ink2">General</h2>
    <div class="card">
      {#if desktop}
        <div class="flex min-h-[60px] items-center gap-3 px-4 py-2 text-[15.5px]">
          <label for="sc" class="flex min-w-0 flex-1 flex-col gap-0.5">Rate-sleep shortcut<small class="text-[13px] {shortcutErr ? 'text-danger' : 'text-muted'}">{shortcutErr || "Works from anywhere · empty = off"}</small></label>
          <input id="sc" bind:value={shortcut} onblur={setShortcut} onkeydown={(e) => e.key === "Enter" && (e.currentTarget as HTMLInputElement).blur()} placeholder="Off"
            class="num h-9 w-[170px] rounded-[8px] bg-fill px-2.5 text-center text-[13px] font-semibold outline-none focus:bg-card focus:shadow-[inset_0_0_0_2px_var(--accent)]" />
        </div>
        <div class="h-px bg-sep"></div>
      {/if}
      <div class="flex min-h-[60px] items-center gap-3 px-4 py-2 text-[15.5px]">
        <label for="ds" class="flex min-w-0 flex-1 flex-col gap-0.5">New day starts at<small class="text-[13px] text-muted">Sleep before this counts as last night</small></label>
        <select id="ds" class="num h-11 rounded-[12px] bg-fill px-3 text-[15px] outline-none" value={app.settings?.day_start_min ?? 0}
          onchange={(e) => app.settings && saveSettings({ ...app.settings, day_start_min: Number((e.currentTarget as HTMLSelectElement).value) })}>
          {#each DAY_STARTS as m}<option value={m}>{clock(m)}</option>{/each}
        </select>
      </div>
      <div class="h-px bg-sep"></div>
      <div class="px-4 pt-3 pb-4">
        <div class="mb-2.5 text-[15.5px]">Appearance</div>
        <Seg label="Appearance" items={[{ id: "system", name: "System" }, { id: "light", name: "Light" }, { id: "dark", name: "Dark" }]} value={app.prefs.theme} onchange={(v) => savePrefs({ ...app.prefs, theme: v })} />
      </div>
    </div>

    <h2 class="mx-1 mt-6 mb-2 text-[13.5px] font-[650] text-ink2">Data</h2>
    <div class="card">
      <button type="button" class="press flex min-h-[60px] w-full items-center gap-3 px-4 py-2 text-left text-[15.5px]" onclick={() => (app.syncOpen = true)}>
        {@render lrow("Sync", syncLine || "Your devices, server and encryption")}<span class="flex text-muted"><Icon name="right" size={18} /></span></button>
      <div class="h-px bg-sep"></div>
      {#if app.sources?.mobile}
        <div class="flex min-h-[60px] items-center gap-3 px-4 py-2 text-[15.5px]">
          {@render lrow("Usage access", "Used to estimate your sleep")}
          {#if app.usageAccess}<span class="inline-flex items-center gap-1 font-semibold text-accent"><Icon name="check" size={17} stroke={2.4} />Allowed</span>
          {:else}<button type="button" class="press h-11 rounded-[12px] bg-accent px-4 text-[15px] font-semibold text-on-accent" onclick={() => bridge()?.openAccessSettings()}>Allow</button>{/if}
        </div>
        <div class="h-px bg-sep"></div>
        <div class="flex min-h-[60px] items-center gap-3 px-4 py-2 text-[15.5px]">
          {@render lrow("Tasks and spending", "Agenda, Hisab · for Connections")}
          {#if app.sources.agenda && app.sources.hisab}<span class="inline-flex items-center gap-1 font-semibold text-accent"><Icon name="check" size={17} stroke={2.4} />Found</span>
          {:else}<span class="text-[14px] text-muted">{app.sources.agenda ? "Agenda only" : app.sources.hisab ? "Hisab only" : "Not found"}</span>{/if}
        </div>
      {:else}
        <div class="flex min-h-[60px] items-center gap-3 px-4 py-2 text-[15.5px]">
          {@render lrow("Activity from Chrono", app.sources?.phone_nights ? `Used to estimate your sleep · plus ${app.sources.phone_nights} nights from your phone` : "Used to estimate your sleep")}
          {#if app.sources?.chrono}<span class="inline-flex items-center gap-1 font-semibold text-accent"><Icon name="check" size={17} stroke={2.4} />Found</span>{:else}<span class="text-[14px] text-muted">Not found</span>{/if}
        </div>
        <div class="h-px bg-sep"></div>
        <div class="flex min-h-[60px] items-center gap-3 px-4 py-2 text-[15.5px]">
          {@render lrow("Tasks and spending", "From Agenda and Hisab, for Connections")}
          {#if app.sources?.agenda && app.sources?.hisab}<span class="inline-flex items-center gap-1 font-semibold text-accent"><Icon name="check" size={17} stroke={2.4} />Found</span>
          {:else}<span class="text-[14px] text-muted">{app.sources?.agenda ? "Agenda only" : app.sources?.hisab ? "Hisab only" : "Not found"}</span>{/if}
        </div>
      {/if}
      <div class="h-px bg-sep"></div>
      <div class="flex min-h-[60px] items-center gap-3 px-4 py-2 text-[15.5px]">
        {@render lrow("Health Connect", "Watch and ring data")}
        <span class="text-[14px] text-muted">Coming later</span>
      </div>
    </div>
    {#if app.sources?.mobile && app.usageAccess}
      <button type="button" class="press mt-3 h-11 w-full text-[15px] font-semibold text-accent" onclick={async () => (await importUsage(), refresh())}>Read phone use again</button>
    {/if}

    <h2 class="mx-1 mt-6 mb-2 text-[13.5px] font-[650] text-ink2">App lock</h2>
    <LockSettings phone={!desktop} appName="Dusk" heading={false} />
  </div>
{/snippet}

{#if desktop}
  <div class="max-w-[1000px]">
    <h1 class="m-0 mb-2 text-[26px] font-bold tracking-[-0.03em]">Settings</h1>
    <div class="grid grid-cols-2 items-start gap-6">{@render colA()}{@render colB()}</div>
  </div>
{:else}
  <div class="pb-6">{@render colA()}<div class="h-4"></div>{@render colB()}</div>
{/if}
{#if bedDial}
  <TimeSheet title="Bedtime goal" value={goals.bedtime} {desktop} ondone={setBedtime} oncancel={() => (bedDial = false)} />
{/if}
