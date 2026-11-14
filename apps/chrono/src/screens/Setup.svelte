<script lang="ts">
  // Android setup, 2 steps: 1 Usage access (needed for phone time),
  // 2 notification counts (optional, off unless turned on).
  import { api } from "../lib/api";
  import { app, guard, refreshAndroid } from "../lib/state.svelte";
  import Icon from "../components/Icon.svelte";
  let waiting = $state(false);
  async function done() {
    const p = (await guard(api.prefs())) ?? {};
    await guard(api.savePrefs({ ...p, setup_done: true }));
    app.setup = 0;
  }
  async function openUsage() {
    waiting = true;
    await guard(api.androidOpen("usage"));
  }
  async function openNotif() {
    waiting = true;
    await guard(api.androidOpen("notifications"));
  }
  // Coming back from Android settings: move on when the permission is granted.
  $effect(() => {
    const f = async () => {
      if (document.visibilityState !== "visible" || !waiting) return;
      waiting = false;
      await refreshAndroid();
      if (app.setup === 1 && app.android.usage) app.setup = app.android.notifications ? (await done(), 0) : 2;
      else if (app.setup === 2 && app.android.notifications) await done();
    };
    document.addEventListener("visibilitychange", f);
    return () => document.removeEventListener("visibilitychange", f);
  });
  const back = () => (app.setup === 2 ? (app.setup = app.android.usage ? 0 : 1) : done());
  $effect(() => {
    const prev = app.closeGuard;
    app.closeGuard = () => (back(), true);
    return () => (app.closeGuard = prev);
  });
</script>

{#snippet li(t: string, ok: boolean)}
  <div style="display:flex;gap:12px;align-items:flex-start;padding:7px 0">
    <span style="display:flex;color:{ok ? 'var(--accent-ink)' : 'var(--muted)'};margin-top:1px"><Icon name={ok ? "check" : "no"} size={18} stroke={2} /></span>
    <span style="font-size:15px;line-height:1.4;color:var(--ink2)">{t}</span>
  </div>
{/snippet}

<div class="page fade" style="z-index:65">
  <div class="appbar">
    <button type="button" class="ib" aria-label="Back" onclick={back}><Icon name="back" size={22} /></button>
    <span class="t">Set up this phone</span>
    <span class="muted num" style="margin-left:auto;margin-right:12px;font-size:14px">Step {app.setup} of 2</span>
  </div>
  <div style="padding:12px 20px 0;flex:1;display:flex;flex-direction:column;overflow-y:auto">
    {#if app.setup === 1}
      <div style="width:56px;height:56px;border-radius:16px;background:var(--accent-soft);color:var(--accent-ink);display:grid;place-items:center"><Icon name="chart" size={30} /></div>
      <h1 style="font-size:25px;font-weight:650;letter-spacing:-0.02em;margin:18px 0 8px;line-height:1.2">Let Chrono see how you use this phone</h1>
      <div class="say" style="margin:0 0 18px;font-size:15px">Android calls this <b>Usage access</b>. Chrono needs it to count phone time.</div>
      <div class="sec" style="margin:0 0 2px;font-weight:600;color:var(--ink)">Chrono will see</div>
      {@render li("Which apps were open, and for how long", true)}
      {@render li("How often the screen turned on and was unlocked", true)}
      <div class="sec" style="margin:12px 0 2px;font-weight:600;color:var(--ink)">Chrono never sees</div>
      {@render li("Messages, photos or anything you type", false)}
      {@render li("What is on the screen or inside apps", false)}
      {@render li("Your location", false)}
      <div style="display:flex;gap:10px;margin-top:14px;padding:12px 14px;border-radius:12px;background:var(--raised);box-shadow:inset 0 0 0 1px var(--line)"><span class="muted" style="display:flex;margin-top:1px"><Icon name="lock" size={17} /></span><span class="hint" style="font-size:13.5px">Everything stays encrypted on your devices before it is synced.</span></div>
      <div style="flex:1;min-height:16px"></div>
      <div class="hint" style="text-align:center;margin-bottom:12px;font-size:13.5px">Next: find <b style="color:var(--ink2)">Chrono</b> in the list and turn on <b style="color:var(--ink2)">Permit usage access</b>.</div>
    {:else}
      <div style="width:56px;height:56px;border-radius:16px;background:var(--track);color:var(--ink2);display:grid;place-items:center"><Icon name="bell" size={28} /></div>
      <div class="muted" style="font-size:13px;font-weight:600;margin-top:18px;letter-spacing:.02em">OPTIONAL · OFF UNLESS YOU TURN IT ON</div>
      <h1 style="font-size:25px;font-weight:650;letter-spacing:-0.02em;margin:6px 0 8px;line-height:1.2">Count notifications too?</h1>
      <div class="say" style="margin:0 0 18px;font-size:15px">See which apps interrupt you most, for example “WhatsApp: 46 today”.</div>
      {@render li("Chrono saves only a number per app", true)}
      {@render li("It never saves who sent it or what it says", false)}
      {@render li("It never opens, hides or answers notifications", false)}
      <div style="display:flex;gap:10px;margin-top:16px;padding:12px 14px;border-radius:12px;background:var(--raised);box-shadow:inset 0 0 0 1px var(--line)"><span class="muted" style="display:flex;margin-top:1px"><Icon name="shield" size={17} /></span><span class="hint" style="font-size:13.5px">Android will warn that the app <i>could</i> read notifications. That is how the permission is worded; Chrono only counts them. You can turn it off any time in Settings.</span></div>
      <div style="flex:1;min-height:16px"></div>
    {/if}
  </div>
  <div style="padding:0 20px calc(18px + env(safe-area-inset-bottom, 8px));display:grid;gap:8px">
    {#if app.setup === 1}
      <button type="button" class="btn pri" style="height:50px;font-size:15.5px" onclick={openUsage}>Open Android settings <Icon name="ext" size={17} /></button>
      <button type="button" class="btn ghost" style="height:48px" onclick={done}>Not now</button>
    {:else}
      <button type="button" class="btn pri" style="height:50px;font-size:15.5px" onclick={openNotif}>Turn on notification counts</button>
      <button type="button" class="btn ghost" style="height:48px" onclick={done}>Skip</button>
    {/if}
  </div>
</div>
