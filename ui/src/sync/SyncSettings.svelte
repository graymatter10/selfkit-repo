<script lang="ts">
  // Shared "Sync" settings section (design: wave-2 sync mockups). Neutral; the app's
  // accent comes in through CSS variables (see README in this folder):
  // --sync-accent, --sync-accent-soft, --sync-on-accent, --sync-surface, --sync-sunk,
  // --sync-line, --sync-ink, --sync-muted, --sync-danger, --sync-ok, --sync-radius.
  import { onDestroy, onMount } from "svelte";
  import { ago, bottomInset, bridge, companionInstalled, syncApi, type ServerForm, type ServerView, type SyncOutcome, type SyncSettings, type SyncStatus } from "./api";

  let { phone = false, onconflicts }: { phone?: boolean; onconflicts?: () => void } = $props();

  let st = $state<SyncStatus | null>(null);
  let srv = $state<ServerView | null>(null);
  let now = $state(Date.now());
  let busy = $state(false);
  let flash = $state<string | null>(null);
  let sheet = $state<"" | "server" | "encrypt" | "unlock" | "passphrase" | "log" | "enable">("");
  let log = $state<SyncOutcome[]>([]);
  let battery = $state(false);
  // forms
  let form = $state<ServerForm>({ pg_url: "", pg_password: "", s3_endpoint: "", s3_bucket: "", s3_region: "", s3_access_key: "", s3_secret: "" });
  let formMsg = $state<{ ok: boolean; text: string } | null>(null);
  let pass1 = $state(""), pass2 = $state(""), oldPass = $state("");
  let recovery = $state<string | null>(null);
  let confirmGroup = $state("");
  let useRecovery = $state(false);
  let err = $state<string | null>(null);

  async function load() {
    try {
      st = await syncApi.status();
      srv = await syncApi.server();
      battery = !!bridge()?.needsBatteryTip();
    } catch (e) {
      flash = String(e);
    }
    now = Date.now();
  }
  let timer: ReturnType<typeof setInterval>;
  onMount(() => {
    load();
    let n = 0;
    timer = setInterval(() => { now = Date.now(); if (++n % 4 === 0 || st?.running) load(); }, 15000);
  });
  onDestroy(() => clearInterval(timer));

  async function syncNow() {
    busy = true;
    flash = null;
    try {
      const o = await syncApi.now("manual");
      if (!o.ran && o.note) flash = o.note;
    } catch (e) {
      flash = String(e);
    }
    busy = false;
    await load();
  }
  async function saveSettings(patch: Partial<SyncSettings>) {
    if (!st) return;
    const next = { ...$state.snapshot(st.settings), ...patch } as SyncSettings;
    st.settings = await syncApi.saveSettings(next);
    bridge()?.schedule();
    await load();
  }

  // QA-U4: "companion" chosen but no companion app on the phone -> nothing syncs; say so.
  const hasCompanion = companionInstalled();
  const orphan = $derived(!!st && st.owner === "companion" && !hasCompanion);
  const head = $derived.by(() => {
    if (!st) return { title: "", sub: "" };
    if (orphan) return { title: "Not syncing", sub: "The Selfkit companion isn't installed" };
    const [title, ...rest] = (busy ? "Syncing…" : st.headline).split(" · ");
    let sub = rest.join(" · ");
    const last = st.last_ok_at && st.last_ok_at >= (st.last_attempt_at ?? 0) ? st.last_peers : [];
    if (!busy && st.last_transport === "lan" && last.length > 1) sub += ` · also ${last.slice(1).join(", ")}`;
    return { title, sub };
  });
  const kind = $derived(!st ? "" : orphan ? "off" : !st.unlocked ? "lock" : busy || st.running ? "sync" : st.last_error && (st.last_attempt_at ?? 0) > (st.last_ok_at ?? 0) ? "off" : st.last_transport === "lan" ? "wifi" : "cloud");
  const nextIn = $derived(st?.next_auto_at ? Math.max(1, Math.round((st.next_auto_at - now) / 60000)) : null);
  const host = (url: string) => { try { return new URL(url.replace(/^postgres(ql)?:/, "http:")).host; } catch { return url; } };
  const dbName = (url: string) => { try { return new URL(url.replace(/^postgres(ql)?:/, "http:")).pathname.slice(1); } catch { return ""; } };
  const here = (t: number) => now - t < 5 * 60000;
  // "Here" only while sync and LAN are on and the last pass didn't fail; the row kept a
  // green pill next to "Waiting to sync · no connection" (S-1/C72).
  const lanLive = $derived(!!st && st.settings.enabled && st.settings.lan_enabled && !(st.last_error && (st.last_attempt_at ?? 0) > (st.last_ok_at ?? 0)));

  function openServer() {
    form = { pg_url: srv?.pg_url ?? "", pg_password: "", s3_endpoint: srv?.s3_endpoint ?? "", s3_bucket: srv?.s3_bucket ?? "", s3_region: srv?.s3_region ?? "", s3_access_key: srv?.s3_access_key ?? "", s3_secret: "" };
    formMsg = null;
    sheet = "server";
  }
  /** Postgres is required; checked before anything reaches secure storage (L4). */
  const formError = () => (!form.pg_url.trim() ? "Enter the Postgres connection (postgres://…)." : "");
  async function testServer() {
    if (formError()) return void (formMsg = { ok: false, text: formError() });
    // (Save with an empty form still removes a saved server, as before.)
    formMsg = { ok: true, text: "Testing…" };
    try { await syncApi.testServer($state.snapshot(form)); formMsg = { ok: true, text: "Connected. Database ready, storage writable." }; }
    catch (e) { formMsg = { ok: false, text: String(e) }; }
  }
  async function saveServer() {
    if (formError() && !srv?.pg_url) return void (formMsg = { ok: false, text: formError() });
    try { srv = await syncApi.saveServer($state.snapshot(form)); sheet = ""; await load(); }
    catch (e) { formMsg = { ok: false, text: String(e) }; }
  }
  function openEncrypt() { pass1 = pass2 = ""; recovery = null; confirmGroup = ""; err = null; sheet = "encrypt"; }
  async function setupEncryption() {
    err = null;
    if (pass1.length < 12) return void (err = "Use at least 12 characters.");
    if (pass1 !== pass2) return void (err = "The two passphrases differ.");
    try { recovery = await syncApi.setupEncryption(pass1); pass1 = pass2 = ""; } catch (e) { err = String(e); }
  }
  /** Rough strength: length plus character variety (a few words beat short symbol soup). */
  function strength(p: string): { score: number; label: string } {
    if (!p) return { score: 0, label: "" };
    const kinds = [/[a-z]/, /[A-Z]/, /[0-9]/, /[^A-Za-z0-9]/].filter((r) => r.test(p)).length;
    const words = p.trim().split(/\s+/).length;
    let score = p.length >= 20 ? 3 : p.length >= 16 ? 2 : p.length >= 12 ? 1 : 0;
    if (score > 0 && (kinds >= 3 || words >= 4)) score++;
    return { score, label: ["Too short (12 or more)", "Weak", "OK", "Strong", "Very strong"][Math.min(score, 4)] };
  }
  const meter = $derived(strength(pass1));
  const groups = $derived(recovery ? recovery.split("-") : []);
  const lastGroup = $derived(groups.length ? groups[groups.length - 1] : "");
  async function unlock() {
    err = null;
    try { await syncApi.unlock(pass1.trim(), useRecovery); pass1 = ""; sheet = ""; await load(); } catch (e) { err = String(e); }
  }
  async function changePass() {
    err = null;
    if (pass1 !== pass2) return void (err = "The two passphrases differ.");
    try {
      const published = await syncApi.changePassphrase(oldPass, pass1);
      sheet = "";
      flash = published || !st?.server_configured ? "Passphrase changed." : "Passphrase changed. New devices can use it after this device next syncs with the server.";
    } catch (e) { err = String(e); }
  }
  async function enableSync() {
    sheet = "";
    await saveSettings({ enabled: true });
    await syncNow();
  }
  async function openLog() { log = await syncApi.log(); sheet = "log"; }
  const changes = (o: SyncOutcome) => o.report.pulled + o.report.pushed + o.report.photos_up + o.report.photos_down;
  const when = (t: number) => new Date(t).toLocaleString([], { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
  const copy = async (t: string) => { try { await navigator.clipboard.writeText(t); flash = "Copied."; } catch { flash = "Couldn't copy; write it down."; } };
</script>

{#snippet icon(name: string, size = 20)}
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
    {#if name === "wifi"}<path d="M2.5 9a14 14 0 0 1 19 0"/><path d="M5.5 12.5a9.5 9.5 0 0 1 13 0"/><path d="M8.7 16a5 5 0 0 1 6.6 0"/><circle cx="12" cy="19.2" r="1" fill="currentColor"/>
    {:else if name === "cloud"}<path d="M7 18.5h10a4 4 0 0 0 .6-7.95A6 6 0 0 0 6.1 9.6 4.5 4.5 0 0 0 7 18.5z"/>
    {:else if name === "off"}<path d="M7 18.5h10a4 4 0 0 0 1.7-.4M20.6 15A4 4 0 0 0 17.6 10.55 6 6 0 0 0 9.4 6.1M6.1 9.6A4.5 4.5 0 0 0 7 18.5"/><path d="M3 3l18 18"/>
    {:else if name === "sync"}<path d="M20 11a8 8 0 0 0-14.3-4.9L4 8"/><path d="M4 4v4h4"/><path d="M4 13a8 8 0 0 0 14.3 4.9L20 16"/><path d="M20 20v-4h-4"/>
    {:else if name === "lock"}<rect x="4.5" y="10.5" width="15" height="10" rx="2.2"/><path d="M8 10.5V7.5a4 4 0 0 1 8 0v3"/>
    {:else if name === "key"}<circle cx="8" cy="15" r="4.5"/><path d="M11.2 11.8L20 3M16.5 6.5l2.5 2.5M14.5 8.5l2 2"/>
    {:else if name === "pc"}<rect x="3" y="4.5" width="18" height="12" rx="2"/><path d="M8.5 20h7M12 16.5V20"/>
    {:else if name === "phone"}<rect x="6.5" y="2.5" width="11" height="19" rx="2.5"/><path d="M11 18.5h2"/>
    {:else if name === "db"}<ellipse cx="12" cy="5.5" rx="7.5" ry="2.8"/><path d="M4.5 5.5v13c0 1.5 3.4 2.8 7.5 2.8s7.5-1.3 7.5-2.8v-13M4.5 12c0 1.5 3.4 2.8 7.5 2.8s7.5-1.3 7.5-2.8"/>
    {:else if name === "box"}<path d="M3.5 7.5L12 3l8.5 4.5v9L12 21l-8.5-4.5z"/><path d="M3.5 7.5L12 12l8.5-4.5M12 12v9"/>
    {:else if name === "bat"}<rect x="2.5" y="7" width="17" height="10" rx="2.2"/><path d="M22 10.5v3"/><path d="M10.5 9l-2 3h4l-2 3"/>
    {:else if name === "chev"}<path d="M9 6l6 6-6 6"/>
    {:else if name === "check"}<path d="M5 12.5l4.5 4.5L19 7.5"/>
    {:else if name === "x"}<path d="M6 6l12 12M18 6L6 18"/>{/if}
  </svg>
{/snippet}

{#snippet toggle(on: boolean, lab: string, fn: () => void, off = false)}
  <!-- off: sync itself is off, so this option does nothing yet; shown off and disabled (L4-13). -->
  <button type="button" class="k-tg" class:k-on={on && !off} role="switch" aria-checked={on && !off} aria-label={lab} disabled={off} onclick={fn}><span></span></button>
{/snippet}

<div class="sks" class:k-phone={phone}>
  {#if st}
    {#if st.secrets_in_file}
      <!-- C80: no system keyring on this desktop; secrets are in a 0600 file (docs/security.md). -->
      <p class="k-s" style="margin:0 0 10px">No system keyring found — your sync keys are stored in a protected file in the Selfkit data folder. Install a keyring (e.g. GNOME Keyring/KWallet) for extra protection.</p>
    {/if}
    {#if st.test_key}
      <!-- Security (docs/security.md): the built-in test key is public. Anyone with a test build,
           on the same Wi-Fi or with access to the server, can read and change data synced with it. -->
      <div class="k-testkey" role="alert">
        <strong>Test key — not private.</strong>
        Set your own passphrase (Encryption, below) before syncing real data. With the public test key, anyone with a Selfkit test build on the same network or with access to your server can read and change synced data.
      </div>
    {/if}
    <div class="k-cols">
      <div class="k-col">
        <div class="k-card k-status">
          <div class="k-big">
            <div class="k-ic" class:k-spin={kind === "sync"}>{@render icon(kind)}</div>
            <div class="k-grow"><div class="k-h">{head.title}</div>{#if head.sub}<div class="k-when">{head.sub}</div>{/if}</div>
          </div>
          {#if orphan}
            <p class="k-note k-warn">This app was set to let the Selfkit companion sync it, but the companion isn't on this phone.</p>
            <div class="k-acts"><button type="button" class="k-btn k-pri" onclick={() => saveSettings({ owner: "app" })}>Let this app sync</button></div>
          {:else if !st.settings.enabled && st.owner !== "companion"}
            <p class="k-note">Keeps this app's data the same on your devices: directly on the same Wi-Fi, or through your own server.</p>
            <div class="k-acts"><button type="button" class="k-btn k-pri" onclick={() => (st!.test_key ? (sheet = "enable") : enableSync())}>Turn on sync</button></div>
          {:else if st.owner !== "companion"}
            <div class="k-acts">
              <button type="button" class="k-btn k-pri" disabled={busy || !st.unlocked} onclick={syncNow}><span class:k-spin={busy}>{@render icon("sync", 18)}</span> {busy ? "Syncing…" : "Sync now"}</button>
              {#if nextIn && st.settings.auto_sync && st.owner === "app"}<span class="k-next">Next automatic sync in about {nextIn} min</span>
              {:else if st.owner === "agent"}<span class="k-next">The Selfkit agent syncs every app in the background</span>{/if}
            </div>
          {/if}
          {#if st.last_note && !st.last_error}<p class="k-note">{st.last_note}</p>{/if}
          {#if flash}<p class="k-note">{flash}</p>{/if}
        </div>

        {#if st.conflicts > 0}
          <div class="k-notice">
            <span class="k-dot"></span>
            <div class="k-grow"><div class="k-t">{st.conflicts} {st.conflicts === 1 ? "item was" : "items were"} changed on two devices</div><div class="k-s">Nothing is lost. Choose which version to keep.</div></div>
            <button type="button" class="k-btn k-sm" onclick={() => onconflicts?.()}>Review</button>
          </div>
        {/if}

        <section>
          <h3>Local sync</h3>
          <div class="k-card k-list">
            <div class="k-row"><div class="k-grow"><div class="k-t">Sync on the same {phone ? "Wi-Fi" : "network"}</div><div class="k-s">{st.settings.enabled ? "Finds your Selfkit devices on this network" : "Used when sync is on"}</div></div>
              {@render toggle(st.settings.lan_enabled, "Local sync", () => saveSettings({ lan_enabled: !st!.settings.lan_enabled }), !st.settings.enabled)}</div>
            {#each st.peers as p (p.device_id)}
              <div class="k-row"><span class="k-mut">{@render icon(/phone|android|oppo|vivo|pixel|samsung/i.test(p.name) ? "phone" : "pc")}</span>
                <div class="k-grow"><div class="k-t">{p.name}</div><div class="k-s">Seen {ago(now, p.last_seen)}{p.last_sync ? ` · synced ${ago(now, p.last_sync)}` : ""}</div></div>
                {#if here(p.last_seen) && lanLive}<span class="k-pill"><span class="k-okdot"></span>Here</span>{/if}</div>
            {/each}
            {#if !st.peers.length && st.settings.lan_enabled}<div class="k-row"><div class="k-s">No devices seen yet. {st.platform === "android" ? "Your PC needs the Selfkit agent running on the same network." : "Your other devices need Selfkit open (or the agent running) on the same network."}</div></div>{/if}
          </div>
        </section>
      </div>

      <div class="k-col">
        <section class:k-first={!phone}>
          <h3>Server</h3>
          <div class="k-card k-list">
            {#if srv?.configured}
              <div class="k-row"><span class="k-mut">{@render icon("db")}</span><div class="k-grow"><div class="k-t">Database</div><div class="k-s">Postgres · {host(srv.pg_url)}{dbName(srv.pg_url) ? ` / ${dbName(srv.pg_url)}` : ""}</div></div></div>
              <div class="k-row"><span class="k-mut">{@render icon("box")}</span><div class="k-grow"><div class="k-t">Photos and backups</div><div class="k-s">{srv.s3_endpoint ? `S3 · ${host(srv.s3_endpoint)} · ${srv.s3_bucket}` : "Not set up: photos stay on this device"}</div></div></div>
              <button type="button" class="k-row k-link" onclick={openServer}><span class="k-grow">Change server</span>{@render icon("chev", 18)}</button>
            {:else}
              <div class="k-row"><div class="k-grow"><div class="k-t">No server</div><div class="k-s">Optional. Any Postgres and any S3 storage; everything is encrypted before upload.</div></div></div>
              <button type="button" class="k-row k-link" onclick={openServer}><span class="k-grow">Set up a server</span>{@render icon("chev", 18)}</button>
            {/if}
          </div>
        </section>

        <section>
          <h3>Encryption</h3>
          <div class="k-card k-list">
            {#if st.unlocked && !st.test_key}
              <div class="k-row"><span class="k-mut">{@render icon("lock")}</span><div class="k-grow"><div class="k-t">On · unlocked on this device</div><div class="k-s">{st.keys_unpublished ? "New passphrase not on the server yet: new devices need the old one until this device syncs with the server" : "Everything is encrypted before it leaves"}</div></div></div>
              <button type="button" class="k-row k-link" onclick={() => { pass1 = pass2 = oldPass = ""; err = null; sheet = "passphrase"; }}><span class="k-grow">Change passphrase</span>{@render icon("chev", 18)}</button>
            {:else if st.encryption_set_up}
              <div class="k-row"><span class="k-mut">{@render icon("lock")}</span><div class="k-grow"><div class="k-t">Locked on this device</div><div class="k-s">Unlock once with your passphrase or recovery key</div></div></div>
              <button type="button" class="k-row k-link" onclick={() => { pass1 = ""; err = null; useRecovery = false; sheet = "unlock"; }}><span class="k-grow">Unlock</span>{@render icon("chev", 18)}</button>
            {:else}
              <div class="k-row"><span class="k-mut">{@render icon("key")}</span><div class="k-grow"><div class="k-t">Not set up</div><div class="k-s">First device: choose a passphrase and save the recovery key. Other devices: unlock with it.</div></div></div>
              <button type="button" class="k-row k-link" onclick={openEncrypt}><span class="k-grow">Set up encryption</span>{@render icon("chev", 18)}</button>
              <button type="button" class="k-row k-link" onclick={() => { pass1 = ""; err = null; useRecovery = false; sheet = "unlock"; }}><span class="k-grow">Unlock with an existing passphrase</span>{@render icon("chev", 18)}</button>
            {/if}
          </div>
        </section>

        <section>
          <h3>Who syncs this app</h3>
          <div class="k-card k-pad">
            <div class="k-seg" role="radiogroup" aria-label="Who syncs this app">
              {#each st.platform === "android" ? [["app", "This app"], ["companion", "Selfkit companion"]] : [["agent", "Background agent"], ["app", "This app"]] as [id, name]}
                <button type="button" role="radio" aria-checked={st.owner === id} class:k-on={st.owner === id} disabled={id === "companion" && !hasCompanion && st.owner !== id} onclick={() => saveSettings({ owner: id as SyncSettings["owner"] })}>{name}{#if id === "companion" && !hasCompanion}<span class="k-tiny">Not installed</span>{/if}</button>
              {/each}
            </div>
            <p class="k-hint">{st.platform === "android" ? hasCompanion ? "The Selfkit companion can sync every app for you; this app then steps back." : "The Selfkit companion isn't installed, so this app syncs itself." : "On a PC the Selfkit agent syncs every app, even when they are closed."}</p>
            {#if st.owner === "app"}
              <div class="k-row k-flat"><div class="k-grow"><div class="k-t">Automatic</div><div class="k-s">On open and every {st.settings.background_minutes} minutes</div></div>
                {@render toggle(st.settings.auto_sync, "Automatic sync", () => saveSettings({ auto_sync: !st!.settings.auto_sync }), !st.settings.enabled)}</div>
            {/if}
          </div>
        </section>

        {#if battery}
          <section>
            <div class="k-card k-row k-tip"><span class="k-mut">{@render icon("bat")}</span>
              <div class="k-grow"><div class="k-t">Let it sync in the background</div><div class="k-s">Some phones (OPPO, vivo and others) stop the 15-minute sync. Allow background activity for this app.</div>
                <button type="button" class="k-btn k-sm" style="margin-top:10px" onclick={() => bridge()?.openBatterySettings()}>Open battery settings</button></div></div>
          </section>
        {/if}

        <section>
          <h3>Details</h3>
          <div class="k-card k-list">
            <div class="k-row"><div class="k-grow"><div class="k-t">Sync this app</div><div class="k-s">{st.settings.enabled ? (st.settings.auto_sync ? "On: Sync now, on open and in the background" : "On: only when you tap Sync now") : "Off: nothing leaves this device"} <!-- S-3 --></div></div>
              {@render toggle(st.settings.enabled, "Sync this app", () => (st!.settings.enabled ? saveSettings({ enabled: false }) : st!.test_key ? (sheet = "enable") : enableSync()))}</div>
            <button type="button" class="k-row k-link k-plain" onclick={openLog}><div class="k-grow"><div class="k-t">Sync log</div><div class="k-s">Last 20 runs, with errors in plain words</div></div>{@render icon("chev", 18)}</button>
            <div class="k-row"><div class="k-grow"><div class="k-t">This device</div><div class="k-s">{st.device_name}{st.pending && st.server_configured ? ` · ${st.pending} change${st.pending === 1 ? "" : "s"} not on the server yet` : ""}</div></div></div>
          </div>
        </section>
      </div>
    </div>
  {:else}
    <div class="k-card k-status"><div class="k-when">Loading sync status…</div></div>
  {/if}

  {#if sheet}
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <div class="k-scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && recovery === null && (sheet = "")}>
      <div class="k-dlg" role="dialog" aria-modal="true" style={phone && bottomInset() ? `padding-bottom:${24 + bottomInset()}px` : ""}>
        {#if sheet === "server"}
          <div class="k-dh"><h4>Server</h4><button type="button" class="k-x" aria-label="Close" onclick={() => (sheet = "")}>{@render icon("x")}</button></div>
          <p class="k-hint">Any Postgres and any S3 storage. Everything is encrypted before upload, so the server only sees noise.</p>
          <label>Postgres connection<input class="k-mono" bind:value={form.pg_url} placeholder="postgres://user@host:5432/selfkit" autocomplete="off" autocapitalize="off" spellcheck="false" /></label>
          <label>Password<input type="password" bind:value={form.pg_password} placeholder={srv?.has_password ? "Saved (leave empty to keep)" : ""} autocomplete="off" /></label>
          <label>S3 endpoint (optional, for photos)<input class="k-mono" bind:value={form.s3_endpoint} placeholder="https://s3.eu-central-003.backblazeb2.com" autocomplete="off" autocapitalize="off" spellcheck="false" /></label>
          <div class="k-two"><label>Bucket<input bind:value={form.s3_bucket} autocapitalize="off" /></label><label>Region<input bind:value={form.s3_region} placeholder="us-east-1" autocapitalize="off" /></label></div>
          <div class="k-two"><label>Access key<input class="k-mono" bind:value={form.s3_access_key} autocomplete="off" autocapitalize="off" spellcheck="false" /></label>
            <label>Secret key<input type="password" bind:value={form.s3_secret} placeholder={srv?.has_s3_secret ? "Saved" : ""} autocomplete="off" /></label></div>
          {#if formMsg}<p class="k-msg" class:k-bad={!formMsg.ok}>{formMsg.text}</p>{/if}
          <p class="k-hint">Passwords stay in this device's secure storage.</p>
          <div class="k-dacts"><button type="button" class="k-btn" onclick={testServer}>Test</button><button type="button" class="k-btn k-pri" onclick={saveServer}>Save</button></div>
        {:else if sheet === "encrypt"}
          {#if recovery === null}
            <div class="k-dh"><h4>Set up encryption</h4><button type="button" class="k-x" aria-label="Close" onclick={() => (sheet = "")}>{@render icon("x")}</button></div>
            <p class="k-hint">Choose a passphrase you will remember. Your other devices ask for it once.</p>
            <label>Passphrase<input type="password" bind:value={pass1} autocomplete="new-password" /></label>
            {#if pass1}<div class="k-meter" aria-live="polite"><span style="width:{Math.max(1, meter.score) * 25}%" class:k-bad={meter.score < 2}></span></div><p class="k-hint" style="margin-top:6px">{meter.label} · a few unrelated words work well, e.g. "lamp river orbit cactus"</p>{/if}
            <label>Repeat it<input type="password" bind:value={pass2} autocomplete="new-password" /></label>
            {#if err}<p class="k-msg k-bad">{err}</p>{/if}
            <div class="k-dacts"><button type="button" class="k-btn k-pri" onclick={setupEncryption}>Continue</button></div>
          {:else}
            <div class="k-dh"><h4>Your recovery key</h4></div>
            <p class="k-hint">Write it down or print it and keep it safe. It is shown only now. With it you can unlock your data if you forget the passphrase.</p>
            <div class="k-rk">{#each groups as g}<span>{g}</span>{/each}</div>
            <div class="k-dacts k-left"><button type="button" class="k-btn k-sm" onclick={() => copy(recovery!)}>Copy</button></div>
            <label>Type the last group to confirm<input class="k-mono" bind:value={confirmGroup} autocapitalize="characters" spellcheck="false" /></label>
            <div class="k-dacts"><button type="button" class="k-btn k-pri" disabled={confirmGroup.trim().toUpperCase() !== lastGroup} onclick={() => { recovery = null; sheet = ""; load(); }}>I saved it</button></div>
          {/if}
        {:else if sheet === "unlock"}
          <div class="k-dh"><h4>Unlock</h4><button type="button" class="k-x" aria-label="Close" onclick={() => (sheet = "")}>{@render icon("x")}</button></div>
          <div class="k-seg" style="margin-bottom:12px"><button type="button" class:k-on={!useRecovery} onclick={() => (useRecovery = false)}>Passphrase</button><button type="button" class:k-on={useRecovery} onclick={() => (useRecovery = true)}>Recovery key</button></div>
          <label>{useRecovery ? "Recovery key" : "Passphrase"}<input type={useRecovery ? "text" : "password"} class:k-mono={useRecovery} bind:value={pass1} autocapitalize="off" spellcheck="false" /></label>
          {#if err}<p class="k-msg k-bad">{err}</p>{/if}
          <p class="k-hint">Needs the key file from your first device: it comes from your server's storage, so set up the server first.</p>
          <div class="k-dacts"><button type="button" class="k-btn k-pri" onclick={unlock}>Unlock</button></div>
        {:else if sheet === "passphrase"}
          <div class="k-dh"><h4>Change passphrase</h4><button type="button" class="k-x" aria-label="Close" onclick={() => (sheet = "")}>{@render icon("x")}</button></div>
          <label>Current passphrase<input type="password" bind:value={oldPass} /></label>
          <label>New passphrase<input type="password" bind:value={pass1} autocomplete="new-password" /></label>
          <label>Repeat it<input type="password" bind:value={pass2} autocomplete="new-password" /></label>
          {#if err}<p class="k-msg k-bad">{err}</p>{/if}
          <div class="k-dacts"><button type="button" class="k-btn k-pri" onclick={changePass}>Change</button></div>
        {:else if sheet === "enable"}
          <div class="k-dh"><h4>Turn on sync with the test key?</h4><button type="button" class="k-x" aria-label="Close" onclick={() => (sheet = "")}>{@render icon("x")}</button></div>
          <p class="k-hint">This build uses a public test key. Anyone with a Selfkit test build on this Wi-Fi, or with access to your server, could read and change this app's data. Set your own passphrase first if this is real data.</p>
          <div class="k-dacts"><button type="button" class="k-btn" onclick={openEncrypt}>Set a passphrase</button><button type="button" class="k-btn k-pri" onclick={enableSync}>Turn on anyway</button></div>
        {:else if sheet === "log"}
          <div class="k-dh"><h4>Sync log</h4><button type="button" class="k-x" aria-label="Close" onclick={() => (sheet = "")}>{@render icon("x")}</button></div>
          <div class="k-log">
            {#each log as o}
              <div class="k-lrow"><div class="k-t">{when(o.started_at)} · {o.transport === "lan" ? `locally${o.peers.length ? " with " + o.peers.join(", ") : ""}` : o.transport === "server" ? "via server" : o.transport === "agent" ? "by the agent" : "not run"}</div>
                <div class="k-s" class:k-bad={!o.ok}>{o.ok ? `${changes(o)} change${changes(o) === 1 ? "" : "s"}${o.report.conflicts ? ` · ${o.report.conflicts} to review` : ""}${o.note ? ` · ${o.note}` : ""}` : o.error}</div></div>
            {:else}<p class="k-hint">No runs yet.</p>{/each}
          </div>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .sks {
    --a: var(--sync-accent, var(--accent, #3e6bb3));
    --a-soft: var(--sync-accent-soft, color-mix(in srgb, var(--a) 14%, transparent));
    --on-a: var(--sync-on-accent, #fff);
    --surf: var(--sync-surface, var(--surface, #fff));
    --sunk: var(--sync-sunk, color-mix(in srgb, var(--ink-c) 6%, transparent));
    --line-c: var(--sync-line, var(--line, rgba(127, 127, 127, 0.22)));
    --ink-c: var(--sync-ink, var(--ink, currentColor));
    --mut: var(--sync-muted, var(--muted, rgba(127, 127, 127, 0.95)));
    --bad: var(--sync-danger, var(--danger, #c2412d));
    --ok: var(--sync-ok, #3f8a5a);
    --r: var(--sync-radius, 16px);
    color: var(--ink-c);
    font-size: 14px;
    line-height: 1.4;
  }
  .k-cols { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: 24px; align-items: start; }
  .k-phone .k-cols { grid-template-columns: minmax(0, 1fr); gap: 0; }
  .k-card { background: var(--surf); border: 1px solid var(--line-c); border-radius: var(--r); }
  section { margin-top: 22px; }
  section.k-first { margin-top: 0; }
  h3 { font-size: 12px; font-weight: 600; color: var(--mut); letter-spacing: 0.04em; text-transform: uppercase; margin: 0 4px 8px; }
  .k-row { display: flex; align-items: center; gap: 12px; padding: 12px 16px; min-height: 52px; width: 100%; text-align: left; color: inherit; background: none; border: 0; font: inherit; box-sizing: border-box; }
  .k-list > .k-row + .k-row { border-top: 1px solid var(--line-c); }
  .k-row.k-flat { padding: 12px 0 0; min-height: 44px; }
  .k-row.k-link { color: var(--a); font-weight: 600; cursor: pointer; }
  .k-row.k-link.k-plain { color: inherit; font-weight: 400; }
  .k-row.k-link :global(svg) { color: var(--mut); }
  .k-t { font-weight: 550; }
  .k-s { color: var(--mut); font-size: 12.5px; margin-top: 1px; }
  .k-s.k-bad, .k-msg.k-bad { color: var(--bad); }
  .k-grow { flex: 1; min-width: 0; }
  .k-mut { color: var(--mut); display: flex; }
  .k-pad { padding: 14px 16px; }
  .k-status { padding: 18px; }
  .k-big { display: flex; gap: 14px; align-items: flex-start; }
  .k-ic { width: 44px; height: 44px; border-radius: 14px; display: grid; place-items: center; background: var(--a-soft); color: var(--a); flex: none; }
  .k-h { font-size: 16.5px; font-weight: 650; letter-spacing: -0.01em; }
  .k-when { color: var(--mut); margin-top: 2px; font-size: 13px; }
  .k-acts { display: flex; align-items: center; gap: 12px; margin-top: 16px; flex-wrap: wrap; }
  .k-next { color: var(--mut); font-size: 12.5px; flex: 1; min-width: 140px; }
  .k-note { margin: 12px 0 0; font-size: 13px; color: var(--mut); }
  .k-testkey { margin: 0 0 14px; padding: 12px 14px; border-radius: 12px; border: 1.5px solid var(--bad); color: var(--bad); font-size: 13.5px; line-height: 1.45; }
  .k-testkey strong { display: block; font-size: 15px; margin-bottom: 2px; }
  .k-note.k-warn { padding: 10px 12px; border-radius: 12px; background: var(--sunk); color: var(--ink-c); }
  .k-btn { height: 44px; padding: 0 16px; border-radius: 12px; font: inherit; font-weight: 600; border: 1px solid var(--line-c); background: var(--surf); color: var(--ink-c); display: inline-flex; align-items: center; justify-content: center; gap: 8px; white-space: nowrap; cursor: pointer; }
  .k-btn.k-pri { background: var(--a); color: var(--on-a); border-color: transparent; }
  .k-btn:disabled { opacity: 0.55; cursor: default; }
  .k-btn.k-sm { padding: 0 14px; font-size: 13.5px; }
  .k-notice { display: flex; gap: 12px; align-items: center; padding: 12px 14px; border-radius: var(--r); background: var(--sunk); margin-top: 12px; }
  .k-dot { width: 8px; height: 8px; border-radius: 4px; background: var(--a); flex: none; }
  .k-okdot { width: 7px; height: 7px; border-radius: 4px; background: var(--ok); }
  .k-pill { display: inline-flex; align-items: center; gap: 6px; height: 24px; padding: 0 9px; border-radius: 12px; background: var(--sunk); color: var(--mut); font-size: 12px; font-weight: 550; }
  .k-tg { width: 46px; height: 44px; flex: none; display: grid; place-items: center; background: none; border: 0; cursor: pointer; padding: 0; }
  .k-tg span { width: 42px; height: 26px; border-radius: 13px; background: var(--line-c); position: relative; transition: background 0.15s; }
  .k-tg span::after { content: ""; position: absolute; top: 3px; left: 3px; width: 20px; height: 20px; border-radius: 10px; background: #fff; box-shadow: 0 1px 2px rgba(0, 0, 0, 0.2); transition: transform 0.15s; }
  .k-tg.k-on span { background: var(--a); }
  .k-tg:disabled { opacity: 0.4; cursor: default; }
  .k-tg.k-on span::after { transform: translateX(16px); }
  .k-seg { display: flex; background: var(--sunk); border-radius: 12px; padding: 3px; gap: 3px; }
  .k-seg button { flex: 1; min-height: 38px; border-radius: 9px; font: inherit; font-weight: 550; color: var(--mut); background: none; border: 0; cursor: pointer; font-size: 13.5px; }
  .k-seg button.k-on { background: var(--surf); color: var(--ink-c); box-shadow: 0 1px 3px rgba(0, 0, 0, 0.12); }
  .k-seg button:disabled { cursor: default; opacity: 0.55; }
  .k-seg .k-tiny { display: block; font-size: 11px; font-weight: 500; }
  .k-hint { color: var(--mut); font-size: 12.5px; margin: 10px 2px 0; }
  .k-tip { align-items: flex-start; background: var(--sunk); }
  .k-spin :global(svg) { animation: sks-spin 1s linear infinite; }
  .k-ic.k-spin :global(svg) { animation: sks-spin 1s linear infinite; }
  @keyframes sks-spin { to { transform: rotate(360deg); } }
  .k-scrim { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.4); display: grid; place-items: center; z-index: 50; padding: 16px; }
  .k-dlg { width: min(540px, 100%); max-height: calc(100% - 32px); overflow: auto; background: var(--surf); border: 1px solid var(--line-c); border-radius: 18px; padding: 20px; box-sizing: border-box; }
  .k-phone .k-scrim { place-items: end stretch; padding: 0; }
  .k-phone .k-dlg { width: 100%; border-radius: 22px 22px 0 0; max-height: 92%; padding: 18px 16px 24px; }
  .k-dh { display: flex; align-items: center; }
  .k-dh h4 { flex: 1; margin: 0; font-size: 18px; font-weight: 650; }
  .k-x { width: 44px; height: 44px; display: grid; place-items: center; background: none; border: 0; color: var(--mut); cursor: pointer; margin-right: -10px; }
  label { display: block; font-size: 12.5px; color: var(--mut); font-weight: 550; margin-top: 14px; }
  label input { display: block; width: 100%; box-sizing: border-box; margin-top: 6px; height: 44px; border: 1px solid var(--line-c); border-radius: 12px; background: var(--surf); padding: 0 14px; color: var(--ink-c); font: inherit; font-size: 14px; }
  .k-mono, label input.k-mono { font-family: ui-monospace, "JetBrains Mono", "DejaVu Sans Mono", monospace; font-size: 12.5px; }
  .k-two { display: flex; gap: 8px; }
  .k-two label { flex: 1; min-width: 0; }
  .k-msg { margin: 14px 2px 0; font-size: 13px; color: var(--ok); }
  .k-dacts { display: flex; gap: 8px; justify-content: flex-end; margin-top: 18px; }
  .k-dacts.k-left { justify-content: flex-start; margin-top: 10px; }
  .k-phone .k-dacts .k-btn { flex: 1; }
  .k-rk { display: grid; grid-template-columns: repeat(4, 1fr); gap: 8px; margin-top: 14px; }
  .k-rk span { font-family: ui-monospace, "DejaVu Sans Mono", monospace; font-size: 15px; letter-spacing: 0.06em; text-align: center; background: var(--sunk); border-radius: 8px; padding: 7px 0; }
  .k-log { margin-top: 10px; }
  .k-meter { height: 6px; border-radius: 3px; background: var(--sunk); margin-top: 8px; overflow: hidden; }
  .k-meter span { display: block; height: 100%; background: var(--ok); transition: width 0.15s; }
  .k-meter span.k-bad { background: var(--bad); }
  .k-lrow { padding: 10px 0; border-top: 1px solid var(--line-c); }
  .k-lrow:first-child { border-top: 0; }
</style>
