<script lang="ts">
  // Devices and sources: where Chrono gets its data, per device; rename
  // devices; time-away threshold; browser extension and phone permissions.
  import { api, type Source } from "../lib/api";
  import { ago, devIcon, dur } from "../lib/fmt";
  import { app, guard, saveSettings } from "../lib/state.svelte";
  import Icon from "../components/Icon.svelte";
  let { phone }: { phone: boolean } = $props();
  let list = $state<Source[]>([]);
  let renaming = $state(""), newName = $state("");
  let howTo = $state(""), addHelp = $state(false);
  $effect(() => { void app.rev; guard(api.sources()).then((s) => s && (list = s)); });
  const fresh = (t: number) => app.now - t < 15 * 60000;
  function osName(s: Source) {
    if (s.platforms.includes("android")) return "Android";
    if (s.platforms.includes("windows")) return "Windows";
    if (s.platforms.includes("linux")) return "Linux";
    return "Browser only";
  }
  function rename(s: Source) {
    const n = JSON.parse(JSON.stringify($state.snapshot(app.settings)));
    const v = newName.trim();
    if (v) n.device_names[s.id] = v;
    else delete n.device_names[s.id];
    saveSettings(n);
    renaming = "";
  }
  function setIdle(e: Event) {
    const n = JSON.parse(JSON.stringify($state.snapshot(app.settings)));
    n.idle_minutes = Number((e.target as HTMLSelectElement).value);
    saveSettings(n);
  }
  const idle = $derived(app.settings?.idle_minutes ?? 2);
</script>

{#snippet st(ok: boolean, t: string)}
  <span class={ok ? "ok" : "muted"} style="font-size:14px;font-weight:600;display:inline-flex;align-items:center;gap:6px;white-space:nowrap">{#if ok}<Icon name="check" size={16} stroke={2.4} />{/if}{t}</span>
{/snippet}

{#snippet device(s: Source)}
  {@const isPhone = s.platforms.includes("android")}
  {@const pc = s.platforms.includes("linux") || s.platforms.includes("windows")}
  <div class="card">
    <div style="display:flex;gap:14px;align-items:flex-start">
      <span style="width:40px;height:40px;border-radius:11px;background:var(--track);display:grid;place-items:center;color:var(--ink2);flex:none"><Icon name={devIcon(s.platforms)} size={20} /></span>
      <div style="flex:1;min-width:0">
        {#if renaming === s.id}
          <form class="field" style="height:40px" onsubmit={(e) => (e.preventDefault(), rename(s))}><!-- svelte-ignore a11y_autofocus --><input autofocus bind:value={newName} aria-label="Device name" onblur={() => rename(s)} /></form>
        {:else}
          <button type="button" style="font-weight:600;font-size:15.5px;min-height:28px;display:inline-flex;align-items:center;gap:6px" title="Rename" onclick={() => ((renaming = s.id), (newName = s.name))}>{s.name}<span class="muted"><Icon name="pen" size={14} /></span></button>
        {/if}
        <div class="muted" style="font-size:13px;margin-top:2px">{s.this_device ? "This device · " : ""}{osName(s)}{s.ms_30d > 0 ? ` · ${dur(s.ms_30d)} in 30 days` : ""}</div>
      </div>
      {#if s.this_device && fresh(s.last_seen)}{@render st(true, "Recording")}{:else}<span class="muted num" style="font-size:13.5px;white-space:nowrap">latest data {ago(s.last_seen, app.now)}</span>{/if}
    </div>
    <div style="margin-top:12px">
      {#if pc}
        <div class="row" style="min-height:54px"><div style="flex:1"><div class="nm" style="font-size:14.5px">Active app</div><div class="sb">{s.platforms.includes("linux") ? "From the window manager. No add-ons needed." : "Recorded while this PC is on."}</div></div>{@render st(fresh(s.last_seen), fresh(s.last_seen) ? "Working" : ago(s.last_seen, app.now))}</div>
        {#if s.this_device}
          <div class="row" style="min-height:54px"><div style="flex:1"><div class="nm" style="font-size:14.5px">Time away</div><div class="sb">Not counted after {idle} min without keyboard or mouse</div></div>
            <select class="chip" value={String(idle)} aria-label="Time away threshold" onchange={setIdle}>{#each [1, 2, 3, 5, 10] as m}<option value={String(m)}>{m} min</option>{/each}</select></div>
        {/if}
        {#each ["firefox", "chrome"] as b}
          {@const on = s.browsers.includes(b)}
          <div class="row" style="min-height:54px"><div style="flex:1"><div class="nm" style="font-size:14.5px">{b === "firefox" ? "Firefox" : "Chrome"}</div><div class="sb">{on ? "Website domains from the active tab" : "Add the Selfkit extension to count websites"}</div></div>
            {#if on}{@render st(true, "Connected")}{:else}<button type="button" class="btn" style="height:40px" onclick={() => (howTo = howTo === s.id + b ? "" : s.id + b)}>How to add</button>{/if}</div>
          {#if howTo === s.id + b}
            <p class="hint fade" style="margin:0 0 12px">Load the Selfkit extension from the <b>extension</b> folder of Selfkit in {b === "firefox" ? "Firefox (about:debugging → This Firefox → Load temporary add-on)" : "Chrome (chrome://extensions → Developer mode → Load unpacked)"}, then run <b>selfkit-agent install-native-host</b> once. Private windows are never counted.</p>
          {/if}
        {/each}
      {/if}
      {#if isPhone || s.phone_counters}
        {@const here = s.this_device && app.android.supported}
        <div class="row" style="min-height:54px"><div style="flex:1"><div class="nm" style="font-size:14.5px">Usage access</div><div class="sb">App time, opens, screen-on and unlocks</div></div>
          {#if here && !app.android.usage}<button type="button" class="btn" style="height:40px" onclick={() => (app.setup = 1)}>Allow</button>{:else}{@render st(true, here ? "Allowed" : "Data received")}{/if}</div>
        <div class="row" style="min-height:54px"><div style="flex:1"><div class="nm" style="font-size:14.5px">Notification counts</div><div class="sb">How many per app, never what they say</div></div>
          {#if here}{#if app.android.notifications}{@render st(true, "On")}{:else}<button type="button" class="btn" style="height:40px" onclick={() => (app.setup = 2)}>Turn on</button>{/if}
          {:else}<span class="muted" style="font-size:13.5px">set on the phone</span>{/if}</div>
      {/if}
    </div>
  </div>
{/snippet}

{#snippet addCard()}
  <div class="card">
    <div style="display:flex;align-items:center;gap:14px">
      <div style="flex:1"><div style="font-weight:600;font-size:15px">Add a device</div><div class="hint" style="margin-top:2px">Install Selfkit on it and sign in with the same server.</div></div>
      <button type="button" class="btn" onclick={() => (addHelp = !addHelp)}><Icon name="plus" size={16} /> Add</button>
    </div>
    {#if addHelp}
      <p class="hint fade" style="margin:12px 0 0">On a PC, install the Selfkit agent (it records the active app and syncs every app). On Android, install Chrono and allow Usage access. Use the same server and recovery key; the new device shows up here after its first sync.</p>
    {/if}
  </div>
{/snippet}

{#if phone}
  <div class="grid">{#each list as s (s.id)}{@render device(s)}{/each}{@render addCard()}</div>
{:else}
  {@const left = list.filter((_, i) => i % 2 === 0)}
  {@const right = list.filter((_, i) => i % 2 === 1)}
  <div class="grid" style="grid-template-columns:minmax(0,1fr) minmax(0,1fr)">
    <div class="grid">{#each left as s (s.id)}{@render device(s)}{/each}{#if !list.length}{@render addCard()}{/if}</div>
    <div class="grid">{#each right as s (s.id)}{@render device(s)}{/each}{#if list.length}{@render addCard()}{/if}</div>
  </div>
{/if}
