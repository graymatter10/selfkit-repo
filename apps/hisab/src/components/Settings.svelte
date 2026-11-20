<script lang="ts">
  // Settings: theme, main currency, exchange rates (fetch + hand edit),
  // category ranking, loan reminders, CSV import/export, desktop shortcut.
  import { showToast } from "@selfkit/ui";
  import { getVersion } from "@tauri-apps/api/app";
  import { api, type Settings } from "../lib/api";
  import { isAndroid } from "../lib/native";
  import { fetchRates } from "../lib/rates";
  import { app, go, main, openSheet, refresh, setPrefs } from "../lib/state.svelte";
  import Ic from "./Ic.svelte";
  import Seg from "./Seg.svelte";
  import StaleOffer from "./StaleOffer.svelte";
  import { syncApi } from "../../../../ui/src/sync/api";
  import LockSettings from "../../../../ui/src/lock/LockSettings.svelte"; // App lock section
  let { desktop = false }: { desktop?: boolean } = $props();
  let syncLine = $state("");
  // Version from the app bundle (tauri.conf.json), not a hard-coded string.
  let version = $state("");
  getVersion().then((v) => (version = v)).catch(() => {});
  syncApi.status().then((st) => (syncLine = st.conflicts ? `${st.conflicts} to review` : st.headline.split(" · ")[0])).catch(() => {});
  let settings = $state<Settings | null>(null);
  api.settingsGet().then((s) => (settings = s));
  let fetching = $state(false);
  let recording = $state(false);
  let newCur = $state("");

  async function saveSettings(s: Settings) {
    try {
      await api.settingsSet(s);
      settings = s;
    } catch (e) {
      showToast(String(e));
    }
  }
  // Main currency: the global default everywhere. Unused accounts that still
  // had the old default switch with it (in Rust); used ones are offered by
  // StaleOffer (from the snapshot, so it shows without re-picking).
  async function setMain(c: string) {
    try {
      const ch = await api.mainCurrencySet(c);
      if (settings) settings = { ...settings, main_currency: ch.new };
      await refresh();
    } catch (e) {
      showToast(String(e));
    }
  }
  async function refetch() {
    fetching = true;
    const ok = await fetchRates();
    fetching = false;
    showToast(ok ? "Rates updated" : "Offline: keeping the saved rates");
  }
  // Currencies the user actually uses, plus any hand-edited ones.
  const used = $derived.by(() => {
    const s = new Set<string>();
    for (const a of app.snap?.accounts ?? []) s.add(a.currency);
    for (const l of app.snap?.loans ?? []) s.add(l.currency);
    for (const g of app.snap?.goals ?? []) s.add(g.currency);
    for (const t of app.snap?.recent ?? []) s.add(t.currency);
    for (const c of Object.keys(app.snap?.rates.manual ?? {})) s.add(c);
    for (const c of app.snap?.missing_rates ?? []) s.add(c);
    if (newCur) s.add(newCur);
    s.delete(main());
    return [...s].sort();
  });
  const rates = $derived(app.snap?.rates);
  // Rate shown as "1 MAIN = x CUR" (stored per 1 base = USD).
  function rateOf(c: string) {
    const r = rates;
    if (!r) return undefined;
    const get = (x: string) => (x === r.base ? 1 : r.manual[x] ?? r.rates[x]);
    const a = get(main()), b = get(c);
    return a && b ? b / a : undefined;
  }
  // A row shows whichever direction gives a number ≥ 1 ("1 USD = 124.42 BDT" with main BDT,
  // not "1 BDT = 0.00803758 USD"); a value typed into an inverted row is turned back (L2-8).
  async function setRate(c: string, v: string, inverted = false) {
    const r = rates;
    if (!r) return;
    const n = inverted && Number(v) > 0 ? 1 / Number(v) : Number(v);
    if (!v.trim()) {
      await api.ratesManual(c, null);
    } else if (n > 0) {
      const get = (x: string) => (x === r.base ? 1 : r.manual[x] ?? r.rates[x]);
      const mainPerBase = get(main()) ?? 1;
      await api.ratesManual(c, n * mainPerBase);
    } else return showToast("Rate must be a number above 0");
    await refresh();
    showToast(`Rate for ${c} saved`);
  }
  async function notifications(on: boolean) {
    if (on && isAndroid()) {
      try {
        const n = await import("@tauri-apps/plugin-notification");
        if (!(await n.isPermissionGranted())) {
          const p = await n.requestPermission();
          if (p !== "granted") return showToast("Notifications are blocked in Android settings");
        }
      } catch {}
    }
    await setPrefs({ notify: on });
    await api.reschedule();
  }
  function record(e: KeyboardEvent) {
    if (!recording || !settings) return;
    e.preventDefault();
    if (e.key === "Escape") return void (recording = false);
    if (["Control", "Shift", "Alt", "Meta", "Super"].includes(e.key)) return;
    const mods = [e.metaKey && "Super", e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift"].filter(Boolean);
    if (!mods.length) return showToast("Use at least one of Super, Ctrl or Alt");
    const key = e.key.length === 1 ? e.key.toUpperCase() : e.key;
    recording = false;
    saveSettings({ ...settings, shortcuts: { ...settings.shortcuts, add_expense: [...mods, key].join("+") } });
  }
  const fetched = $derived(rates?.fetched_at ? new Date(rates.fetched_at).toLocaleString(undefined, { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" }) : "never");
  const row = "flex min-h-12 items-center gap-3 border-b border-sep px-4 last:border-0";
  const h = "m-0 px-1 pt-2 pb-1.5 text-[13px] font-semibold text-muted";
</script>

<svelte:window onkeydown={record} />

<div class="flex flex-col gap-1 {desktop ? 'max-w-[640px]' : ''}">
  <p class={h}>Sync</p>
  <div class="card overflow-hidden"><button type="button" class="{row} w-full text-left text-[15px]" onclick={() => go("sync")}><span class="flex-1">Sync</span><span class="truncate text-[14px] text-muted">{syncLine}</span><Ic name="chevron" size={16} class="text-muted" /></button></div>

  <p class="{h} pt-4">App lock</p>
  <LockSettings phone={!desktop} appName="Hisab" heading={false} />

  <p class="{h} pt-4">Appearance</p>
  <Seg label="Theme" items={[{ id: "system", name: "System" }, { id: "light", name: "Light" }, { id: "dark", name: "Dark" }]} value={app.prefs.theme ?? "system"} onchange={(v) => setPrefs({ theme: v })} />

  <p class="{h} pt-4">Currencies</p>
  <div class="card overflow-hidden">
    <button type="button" class="{row} hl w-full text-left" onclick={() => openSheet({ kind: "currency", value: main(), onpick: setMain })}><span class="flex-1 text-[15px]">Main currency</span>
      <span class="text-[15px] font-semibold text-accent">{main()}</span></button>
    <StaleOffer />
    <div class={row}><span class="flex-1 text-[14px] text-muted">Rates updated {fetched}</span>
      <button type="button" disabled={fetching} class="press text-[14px] font-semibold text-accent disabled:opacity-40" onclick={refetch}>{fetching ? "Updating…" : "Update now"}</button></div>
    {#each used as c (c)}
      {@const r = rateOf(c)}
      {@const inv = !!r && r < 1}
      {@const shown = r ? (inv ? 1 / r : r) : undefined}
      <label class={row}>
        <span class="flex-1 text-[15px]">1 {inv ? c : main()} =</span>
        <input inputmode="decimal" value={shown ? +shown.toPrecision(6) : ""} placeholder="rate" aria-label="Rate for {c}" class="num w-28 bg-transparent py-3 text-right text-[15px] outline-none {!r ? 'placeholder:text-danger' : ''}" onchange={(e) => setRate(c, (e.currentTarget as HTMLInputElement).value, inv)} />
        <span class="w-10 text-[15px] font-semibold">{inv ? main() : c}</span>
        {#if rates?.manual[c]}<span class="rounded-full bg-fill px-2 text-[11px] text-muted">by hand</span>{/if}
      </label>
    {/each}
    <button type="button" class="{row} w-full text-[15px] text-accent" onclick={() => openSheet({ kind: "currency", value: "", onpick: (c) => (newCur = c) })}><Ic name="plus" size={17} />Add a rate</button>
  </div>
  <p class="m-0 px-1 pt-1.5 text-[12px] text-muted">Totals convert into the main currency with these rates. They are fetched online and kept for offline use; type a rate to fix it by hand, clear it to go back to the fetched one.</p>

  <p class="{h} pt-4">Add screen</p>
  <div class="card overflow-hidden">
    <div class={row}><span class="flex-1 text-[15px]">Category order</span><div class="w-[200px]"><Seg small label="Ranking" items={[{ id: "manual", name: "My order" }, { id: "used", name: "Most used" }]} value={app.prefs.rank ?? "manual"} onchange={(v) => setPrefs({ rank: v })} /></div></div>
    <label class={row}><span class="flex-1 text-[15px]">Default account</span>
      <select value={app.prefs.account ?? ""} class="appearance-none bg-transparent py-3 text-right text-[15px] text-accent outline-none" onchange={(e) => setPrefs({ account: (e.currentTarget as HTMLSelectElement).value })}>
        <option value="">First account</option>{#each app.snap?.accounts.filter((a) => !a.archived) ?? [] as a (a.id)}<option value={a.id}>{a.name}</option>{/each}</select></label>
    <button type="button" class="{row} w-full text-left text-[15px]" onclick={() => go("categories")}><span class="flex-1">Categories and budgets</span><Ic name="chevron" size={16} class="text-muted" /></button>
  </div>

  <p class="{h} pt-4">Notifications</p>
  <div class="card overflow-hidden">
    <label class={row}><span class="flex-1 text-[15px]">Loan due-date reminders</span><input type="checkbox" checked={app.prefs.notify ?? false} class="h-5 w-5 accent-[var(--accent)]" onchange={(e) => notifications((e.currentTarget as HTMLInputElement).checked)} /></label>
  </div>
  <p class="m-0 px-1 pt-1.5 text-[12px] text-muted">Off by default. Each loan sets how many days before its due date to remind you.</p>

  {#if desktop}
    <p class="{h} pt-4">Quick add (desktop)</p>
    <div class="card overflow-hidden">
      <div class={row}><span class="flex-1 text-[15px]">Global shortcut</span>
        <button type="button" class="press rounded-[8px] bg-fill px-3 py-1.5 font-mono text-[13px] {recording ? 'ring-2 ring-accent' : ''}" onclick={() => (recording = true)}>{recording ? "Press keys…" : settings?.shortcuts.add_expense || "Off"}</button>
        {#if settings?.shortcuts.add_expense}<button type="button" class="press text-[13px] text-muted" onclick={() => settings && saveSettings({ ...settings, shortcuts: { ...settings.shortcuts, add_expense: "" } })}>Turn off</button>{/if}
      </div>
    </div>
    <p class="m-0 px-1 pt-1.5 text-[12px] text-muted">Opens a small “add expense” window from anywhere. On i3 you can also bind a key to <code>hisab --capture</code>.</p>
  {/if}

  <p class="{h} pt-4">Data</p>
  <div class="card overflow-hidden">
    <button type="button" class="{row} w-full text-left text-[15px]" onclick={() => openSheet({ kind: "export" })}><span class="flex-1">Export CSV or PDF</span><Ic name="chevron" size={16} class="text-muted" /></button>
    <button type="button" class="{row} w-full text-left text-[15px]" onclick={() => openSheet({ kind: "import" })}><span class="flex-1">Import CSV</span><Ic name="chevron" size={16} class="text-muted" /></button>
    <button type="button" class="{row} w-full text-left text-[15px]" onclick={() => go("recurring")}><span class="flex-1">Recurring and subscriptions</span><Ic name="chevron" size={16} class="text-muted" /></button>
  </div>
  <p class="m-0 px-1 pt-1.5 pb-6 text-[12px] text-muted">Everything stays on this device and syncs encrypted through Selfkit. Hisab {version}</p>
</div>
