<script lang="ts">
  // CSV import (next to export). Pick a .csv (Android: system file picker;
  // desktop: file dialog) → Rust guesses the columns (Hisab's own export is
  // read as is; bank files get a mapping step) → preview with counts, errors
  // and duplicates (same date + amount + note: skipped) → import into the
  // chosen account. New entries default to the main currency. Undo removes the
  // whole import at once. Rust: src-tauri/src/csvimport.rs.
  import { showToast } from "@selfkit/ui";
  import { api, type CsvMapping, type CsvPreview } from "../lib/api";
  import { money, shortDate } from "../lib/fmt";
  import { accountById, accounts, app, catById, closeSheet, defaultAccount, refresh } from "../lib/state.svelte";
  import Ic from "./Ic.svelte";
  import SheetFrame from "./SheetFrame.svelte";
  let { desktop = false }: { desktop?: boolean } = $props();
  let fileEl: HTMLInputElement | undefined = $state();
  let name = $state("");
  let text = "";
  let account = $state(defaultAccount());
  let p = $state<CsvPreview | null>(null);
  let map = $state<CsvMapping | null>(null);
  let busy = $state(false);
  let error = $state("");

  async function run(m: CsvMapping | null) {
    busy = true;
    try {
      p = await api.csvPreview(text, m, account);
      map = { ...p.mapping };
      error = "";
    } catch (e) {
      error = String(e);
      p = null;
    } finally {
      busy = false;
    }
  }
  async function picked(e: Event) {
    const f = (e.currentTarget as HTMLInputElement).files?.[0];
    (e.currentTarget as HTMLInputElement).value = "";
    if (!f) return;
    if (f.size > 20 * 1024 * 1024) return void (error = "That file is over 20 MB.");
    name = f.name;
    text = await f.text();
    await run(null);
  }
  const remap = () => map && run({ ...map });

  async function doImport() {
    if (!p) return;
    busy = true;
    try {
      const ids = await api.csvImport(p.preview.rows);
      await refresh();
      closeSheet();
      showToast(`Imported ${ids.length} ${ids.length === 1 ? "entry" : "entries"}`, async () => {
        await api.csvImportUndo(ids);
        await refresh();
        showToast("Import undone");
      }, 15000);
    } catch (e) {
      showToast(String(e));
    } finally {
      busy = false;
    }
  }

  const FIELDS: [keyof CsvMapping, string][] = [
    ["date", "Date"], ["amount", "Amount"], ["debit", "Money out"], ["credit", "Money in"], ["description", "Description"],
    ["category", "Category"], ["account", "Account"], ["currency", "Currency"], ["time", "Time"],
  ];
  const FORMATS = [
    ["%Y-%m-%d", "2026-11-15"], ["%d/%m/%Y", "15/11/2026"], ["%m/%d/%Y", "11/15/2026"], ["%d-%m-%Y", "15-11-2026"], ["%d.%m.%Y", "15.11.2026"],
    ["%Y/%m/%d", "2026/11/15"], ["%d/%m/%y", "15/11/26"], ["%m/%d/%y", "11/15/26"], ["%d %b %Y", "15 Nov 2026"], ["%d-%b-%Y", "15-Nov-2026"], ["%b %d, %Y", "Nov 15, 2026"],
  ];
  const row = "flex min-h-12 items-center gap-3 border-b border-sep px-4 last:border-0";
  const sel = "min-w-0 max-w-[60%] appearance-none truncate bg-transparent py-3 text-right text-[15px] text-accent outline-none";
  const shown = $derived(p ? p.preview.rows.slice(0, 60) : []);
</script>

<SheetFrame title="Import CSV" {desktop} left="Close" onleft={closeSheet} right={p && p.preview.ok ? "Import" : ""} rightDisabled={busy} onright={doImport}>
  <div class="flex flex-col gap-4 pt-1 pb-2">
    <input bind:this={fileEl} type="file" accept=".csv,text/csv,text/comma-separated-values,text/plain,application/vnd.ms-excel" class="hidden" onchange={picked} />
    <button type="button" class="press card flex min-h-14 w-full items-center gap-3 px-4 text-left" onclick={() => fileEl?.click()}>
      <Ic name="import" size={20} class="text-accent" />
      <span class="min-w-0 flex-1"><span class="block truncate text-[15px] font-medium">{name || "Choose a CSV file"}</span>
        <span class="block text-[12.5px] text-muted">{p ? `${p.count} rows · ${p.mapping.hisab ? "Hisab export" : `bank file · “${p.delimiter}” separated`}` : "Hisab's own export, or a bank statement"}</span></span>
      <Ic name="chevron" size={16} class="text-muted" />
    </button>
    {#if error}<p class="m-0 rounded-[12px] bg-danger-soft px-3 py-2 text-[14px] text-ink2">{error}</p>{/if}

    <div class="card overflow-hidden">
      <label class={row}><span class="flex-1 text-[15px] text-ink2">Into account</span>
        <select bind:value={account} class={sel} onchange={() => text && remap()}>{#each accounts() as a (a.id)}<option value={a.id}>{a.name}</option>{/each}</select></label>
    </div>
    <p class="-mt-3 m-0 px-1 text-[12px] text-muted">Rows without an account column (or with an unknown one) go here. Without a currency column, amounts are in {app.snap?.main ?? ""}.</p>

    {#if p && map && !map.hisab}
      <div>
        <p class="m-0 px-1 pb-1.5 text-[13px] font-semibold text-muted">Columns</p>
        <div class="card overflow-hidden">
          {#each FIELDS as [k, label] (k)}
            <label class={row}><span class="flex-1 text-[15px] text-ink2">{label}</span>
              <select class={sel} value={map[k] == null ? "" : String(map[k])} onchange={(e) => { const v = (e.currentTarget as HTMLSelectElement).value; (map as unknown as Record<string, unknown>)[k] = v === "" ? null : Number(v); remap(); }}>
                <option value="">—</option>{#each p.headers as h, i (i)}<option value={String(i)}>{h || `Column ${i + 1}`}</option>{/each}</select></label>
          {/each}
          <label class={row}><span class="flex-1 text-[15px] text-ink2">Date format</span>
            <select class={sel} bind:value={map.date_format} onchange={remap}>{#each FORMATS as [f, ex] (f)}<option value={f}>{ex}</option>{/each}</select></label>
          {#if map.amount != null && map.debit == null && map.credit == null}
            <label class={row}><span class="flex-1 text-[15px] text-ink2">Positive amounts are spending</span>
              <input type="checkbox" bind:checked={map.positive_is_expense} onchange={remap} class="h-5 w-5 accent-[var(--accent)]" /></label>
          {/if}
        </div>
      </div>
    {/if}

    {#if p}
      <div class="flex flex-wrap gap-2 text-[13px]">
        <span class="rounded-full bg-accent-soft px-3 py-1 font-semibold text-accent-ink">{p.preview.ok} to import</span>
        {#if p.preview.duplicates}<span class="rounded-full bg-fill px-3 py-1 text-ink2">{p.preview.duplicates} already in Hisab (skipped)</span>{/if}
        {#if p.preview.errors}<span class="rounded-full bg-danger-soft px-3 py-1 text-ink2">{p.preview.errors} with problems (skipped)</span>{/if}
      </div>
      <div class="card overflow-hidden">
        {#each shown as r (r.line)}
          <div class="flex items-start gap-3 border-b border-sep px-4 py-2.5 last:border-0 {r.error || r.duplicate ? 'opacity-60' : ''}">
            <span class="num w-14 shrink-0 pt-px text-[12.5px] text-muted">{r.date ? shortDate(r.date) : `Line ${r.line}`}</span>
            <span class="min-w-0 flex-1">
              <span class="block truncate text-[14.5px]">{r.note || catById(r.category)?.name || "—"}</span>
              <span class="block truncate text-[12.5px] {r.error ? 'text-danger' : 'text-muted'}">{r.error ? `Line ${r.line}: ${r.error}` : r.duplicate ? "Already in Hisab" : `${r.kind === "transfer" ? "Transfer" : catById(r.category)?.name ?? ""}${r.category_name && catById(r.category)?.name !== r.category_name ? ` (“${r.category_name}”)` : ""} · ${accountById(r.account)?.name ?? ""}`}</span>
            </span>
            {#if !r.error}<span class="num shrink-0 text-[14.5px] font-semibold {r.kind === 'income' ? 'text-accent-ink' : ''}">{r.kind === "income" ? "+" : r.kind === "expense" ? "−" : ""}{money(r.amount, r.currency)}</span>{/if}
          </div>
        {/each}
        {#if p.preview.rows.length > shown.length}<p class="m-0 px-4 py-2.5 text-[13px] text-muted">…and {p.preview.rows.length - shown.length} more rows</p>{/if}
      </div>
      <button type="button" disabled={busy || !p.preview.ok} class="press h-[52px] w-full rounded-[14px] bg-accent text-[17px] font-semibold text-on-accent disabled:opacity-40" onclick={doImport}>
        {busy ? "Working…" : p.preview.ok ? `Import ${p.preview.ok} ${p.preview.ok === 1 ? "entry" : "entries"}` : "Nothing new to import"}</button>
      <p class="m-0 -mt-2 px-1 text-[12px] text-muted">You can undo the whole import right after.</p>
    {/if}
  </div>
</SheetFrame>
