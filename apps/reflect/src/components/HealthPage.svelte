<script lang="ts">
  // Agent + sync health from the desktop agent's health.json. Red only for errors.
  import { agent, app, healthLevel } from "../lib/state.svelte";
  import { ago, bytes } from "../lib/fmt";
  import Icon from "./Icon.svelte";
  let { desktop }: { desktop: boolean } = $props();
  const h = $derived(app.health);
  const d = $derived(h?.doc ?? {});
  const lvl = $derived(healthLevel(h));
  const now = Date.now();
  const week = $derived((d.errors ?? []).filter((x: any) => now - x.at < 7 * 86_400_000));
  const pct = $derived(d.sync?.server_db_bytes != null && d.sync?.server_db_limit_bytes ? Math.round((d.sync.server_db_bytes / d.sync.server_db_limit_bytes) * 100) : null);
  const APPC: Record<string, string> = { chrono: "var(--chrono)", hisab: "var(--hisab)", agenda: "var(--agenda)", dusk: "var(--dusk)", reflect: "var(--accent)", settings: "var(--faint)" };
  const NAME: Record<string, string> = { chrono: "Chrono", hisab: "Hisab", agenda: "Agenda", dusk: "Dusk", reflect: "Reflect", settings: "Settings" };
  const apps = $derived(Object.entries((d.storage?.apps ?? {}) as Record<string, number>).filter(([, b]) => b > 0).sort((a, b) => b[1] - a[1]));
  const other = $derived((d.storage?.files_bytes ?? 0) + (d.storage?.backups_bytes ?? 0) + (d.storage?.logs_bytes ?? 0));
  const days = (s: number) => (s >= 86400 ? `${Math.floor(s / 86400)} ${Math.floor(s / 86400) === 1 ? "day" : "days"}` : s >= 3600 ? `${Math.floor(s / 3600)} h` : `${Math.max(1, Math.floor(s / 60))} min`);
  const status = $derived.by(() => {
    if (!h?.found) return null;
    if (!h.running) return { err: true, t: "The agent isn't running", p: `Last heard from ${ago(d.written_at, now)}. Tracking, sync and backups are stopped on this PC until it starts again.` };
    const bad: string[] = [];
    if (d.tracking?.error) bad.push("tracking");
    if (d.sync?.last_error) bad.push("sync");
    if (d.backup?.last_error) bad.push("backups");
    if (d.lan?.error) bad.push("local network sync");
    if (bad.length) return { err: true, t: "Something needs a look", p: `There's a problem with ${bad.join(" and ")}. The details are below.` };
    const parts = [d.tracking?.on && !d.tracking?.paused ? "Tracking is on" : "Tracking is paused", d.sync?.configured ? `data synced ${ago(d.sync?.last_ok_at, now)}` : "sync isn't set up", d.backup?.last_ok_at ? `the last backup is from ${ago(d.backup.last_ok_at, now)}` : "no backup yet"];
    const s = parts.join(", ").replace(/, ([^,]*)$/, " and $1") + ".";
    const gaps = !d.sync?.configured || !d.backup?.target;
    return { err: false, t: gaps ? "Tracking is running" : "Everything is running", p: s + (d.sync?.server_db_warn ? " One thing to look at: server space." : "") };
  });
</script>

{#snippet row(icon: string, k: string, sub: string, v: string, cls = "")}
  <div class="lrow"><span class="ic"><Icon name={icon} size={18} /></span><div class="lbl"><b>{k}</b><small>{sub}</small></div>{#if v}<span class="v {cls}">{v}</span>{/if}</div>
{/snippet}
{#snippet statusCard()}
  {#if status}
    <div class="card flex items-start gap-3.5 p-[18px]">
      <span class="grid h-10 w-10 shrink-0 place-items-center rounded-[12px] {status.err ? 'bg-[color-mix(in_srgb,var(--danger)_10%,transparent)] text-danger' : 'bg-accent-soft text-accent'}"><Icon name={status.err ? "alert" : "check"} size={22} stroke={2.3} /></span>
      <div><h3 class="m-0 text-[17px] font-[680] tracking-[-0.018em]">{status.t}</h3><p class="mt-1 text-[13.5px] leading-[1.45] text-ink2">{status.p}</p></div>
    </div>
  {/if}
{/snippet}
{#snippet mainCard()}
  <div class="card">
    {@render row("eye", "Tracking", d.tracking?.error ? d.tracking.error : d.tracking?.paused ? `Paused${d.tracking.paused_until ? " until " + new Date(d.tracking.paused_until).toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" }) : ""}` : `Recording on this PC${d.tracking?.current_app ? ` · now: ${d.tracking.current_app}` : ""}`,
      d.tracking?.error ? "Problem" : d.tracking?.paused ? "Paused" : d.tracking?.on ? "On" : "Off", d.tracking?.error ? "!text-danger" : d.tracking?.on && !d.tracking?.paused ? "!text-accent" : "")}
    {@render row("sync", "Last sync", !d.sync?.configured ? "No server set up yet" : d.sync?.last_error ? `Failed: ${d.sync.last_error}` : `To your server over the internet · ${d.sync?.pending_records ? `${d.sync.pending_records} changes waiting` : "nothing waiting"}`,
      d.sync?.configured ? ago(d.sync?.last_ok_at, now) : "Off", d.sync?.last_error ? "!text-danger" : "")}
    {@render row("wifi", "Local network", d.lan?.error ? d.lan.error : !d.lan?.enabled ? "Off" : d.lan?.peers ? "Syncs directly with devices on the same Wi-Fi" : "No other device found right now",
      d.lan?.enabled ? `${d.lan?.peers ?? 0} ${d.lan?.peers === 1 ? "device" : "devices"}` : "", d.lan?.error ? "!text-danger" : "")}
    {@render row("cloud", "Last backup", d.backup?.last_error ? `Failed: ${d.backup.last_error}` : !d.backup?.enabled ? "Backups are off" : `${d.backup?.target === "s3" ? "To your S3 storage" : d.backup?.target === "local" ? "On this PC" : "No target"}${d.backup?.last_weekly ? ` · weekly copy ${d.backup.last_weekly}` : ""}`,
      d.backup?.enabled ? ago(d.backup?.last_ok_at, now) : "Off", d.backup?.last_error ? "!text-danger" : "")}
    {@render row("cpu", "Agent", `Running ${days(d.agent?.uptime_secs ?? 0)} · ${Math.round((d.agent?.rss_kib ?? 0) / 1024)} MB memory · version ${d.version ?? "?"}`, "")}
  </div>
  <div class="mt-3 flex gap-2"><button type="button" class="btn press flex-1" disabled={!h?.running} onclick={() => agent("sync_now")}><Icon name="sync" size={17} stroke={2} />Sync now</button>
    <button type="button" class="btn press flex-1" disabled={!h?.running} onclick={() => agent("backup_now")}><Icon name="cloud" size={17} stroke={2} />Back up now</button></div>
{/snippet}
{#snippet errorsCard()}
  <div class="card">
    <div class="ctitle px-4 pt-4 pb-1">Problems this week<span class="ml-auto text-[12.5px] font-medium text-muted">{week.length ? `${week.length}` : "None"}</span></div>
    {#each week.slice(0, 6) as x, i}
      <div class="flex items-start gap-3 px-4 py-3 {i ? 'border-t border-sep' : ''}">
        <span class="mt-1.5 h-2 w-2 shrink-0 rounded-full {x.level === 'ERROR' ? 'bg-danger' : 'bg-faint'}"></span>
        <div class="min-w-0"><div class="text-[14px] leading-[1.4] break-words">{x.message}</div><div class="mt-0.5 text-[12.5px] text-muted">{ago(x.at, now)} · {String(x.source).replace("selfkit_agent::", "")}{x.level === "WARN" ? " · warning" : ""}</div></div>
      </div>
    {:else}<p class="px-4 pb-4 text-[13.5px] text-muted">Nothing went wrong in the last 7 days.</p>{/each}
  </div>
{/snippet}
{#snippet storageCard()}
  <div class="card p-4"><div class="ctitle">Storage</div>
    {#if pct != null}
      <div class="mt-3.5 flex items-baseline text-[14px]"><span class="flex-1 text-ink2">Server database</span><span class="num font-[650]">{bytes(d.sync.server_db_bytes)}</span><span class="num ml-1.5 text-muted">of {bytes(d.sync.server_db_limit_bytes)}</span></div>
      <div class="relative mt-2 h-2 overflow-hidden rounded-[4px] bg-fill"><i class="block h-full rounded-[4px] {d.sync.server_db_warn ? 'bg-accent' : 'bg-bar'}" style="width:{Math.min(100, pct)}%"></i><s class="absolute -top-0.5 -bottom-0.5 w-0.5 bg-muted opacity-60" style="left:80%"></s></div>
      {#if d.sync.server_db_warn}<div class="mt-3 rounded-[12px] bg-accent-soft px-3.5 py-3 text-[13.5px] leading-[1.45] text-ink2"><b class="font-[650] text-accent">{pct}% full.</b> Sync keeps working until it's full. Free up room on the server or move to a bigger one soon.</div>{/if}
    {/if}
    <div class="mt-[18px] flex items-baseline text-[14px]"><span class="flex-1 text-ink2">On this PC</span><span class="num font-[650]">{bytes(d.storage?.total_bytes ?? 0)}</span></div>
    <div class="mt-2.5 flex h-2.5 gap-0.5 overflow-hidden rounded-[5px]">{#each apps as [a, b], i}<i class="block h-full bg-bar" style="flex:{b};opacity:{1 - i * 0.13}"></i>{/each}{#if other}<i class="block h-full bg-fill" style="flex:{other}"></i>{/if}</div>
    <div class="mt-2.5 flex flex-wrap gap-x-3.5 gap-y-1.5">
      {#each apps as [a, b]}<span class="inline-flex items-center gap-[5px] text-[12.5px] text-muted"><i class="inline-block h-1.5 w-1.5 rounded-full" style="background:{APPC[a] ?? 'var(--faint)'}"></i>{NAME[a] ?? a} <b class="num font-semibold text-ink2">{bytes(b)}</b></span>{/each}
      {#if other}<span class="text-[12.5px] text-muted">Photos, backups and logs <b class="num font-semibold text-ink2">{bytes(other)}</b></span>{/if}
    </div>
  </div>
{/snippet}
{#snippet noAgent()}
  <div class="card flex items-start gap-3.5 p-[18px]">
    <span class="grid h-10 w-10 shrink-0 place-items-center rounded-[12px] bg-fill text-ink2"><Icon name="cpu" size={22} /></span>
    {#if h?.mobile}
      <div><h3 class="m-0 text-[17px] font-[680] tracking-[-0.018em]">The agent runs on your PCs</h3><p class="mt-1 text-[13.5px] leading-[1.45] text-ink2">Tracking, sync and backups are handled by the background agent on your computers. Open Reflect on a PC to see how it's doing.</p></div>
    {:else}
      <div><h3 class="m-0 text-[17px] font-[680] tracking-[-0.018em]">No agent on this PC</h3><p class="mt-1 text-[13.5px] leading-[1.45] text-ink2">The background agent tracks screen time, syncs your apps and makes backups. Install it with <code class="rounded bg-fill px-1 text-[12.5px]">selfkit-agent install-service --enable</code>.</p></div>
    {/if}
  </div>
  {#if h?.local_dbs.length}
    <div class="card mt-3 p-4"><div class="ctitle">Stored on this {h.mobile ? "phone" : "PC"}</div>
      <div class="mt-2">{#each h.local_dbs as [a, b]}<div class="flex min-h-10 items-center gap-2 text-[14px]"><i class="h-1.5 w-1.5 rounded-full" style="background:{APPC[a] ?? 'var(--faint)'}"></i><span class="flex-1">{NAME[a] ?? a}</span><span class="num text-ink2">{bytes(b)}</span></div>{/each}</div></div>
  {/if}
{/snippet}

{#if !h}<p class="pt-16 text-center text-muted">Loading…</p>
{:else if desktop}
  <div class="max-w-[1020px]">
    <div class="mb-5"><h1 class="dtitle">Health</h1><p class="mt-1.5 text-[14px] text-muted">How tracking, sync and backups are doing on this PC. Nothing here leaves your devices.</p></div>
    {#if !h.found}<div class="max-w-[560px]">{@render noAgent()}</div>
    {:else}<div class="grid grid-cols-2 items-start gap-3.5"><div class="flex flex-col gap-3">{@render statusCard()}<div>{@render mainCard()}</div></div><div class="flex flex-col gap-3">{@render errorsCard()}{@render storageCard()}</div></div>{/if}
  </div>
{:else}
  {#if !h.found}{@render noAgent()}
  {:else}<div class="flex flex-col gap-3">{@render statusCard()}<div>{@render mainCard()}</div>{#if lvl === "warn"}{@render storageCard()}{@render errorsCard()}{:else}{@render errorsCard()}{@render storageCard()}{/if}</div>{/if}
{/if}
