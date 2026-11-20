<script lang="ts">
  // Plain CSV / PDF export with the not-encrypted warning.
  import { showToast } from "@selfkit/ui";
  import { api } from "../lib/api";
  import { addDays, addMonths, monthIso, todayIso } from "../lib/fmt";
  import { closeSheet } from "../lib/state.svelte";
  import Ic from "./Ic.svelte";
  import SheetFrame from "./SheetFrame.svelte";
  import Seg from "./Seg.svelte";
  let { desktop = false }: { desktop?: boolean } = $props();
  let fmt = $state<"csv" | "pdf">("csv");
  let period = $state("month");
  let from = $state(monthIso() + "-01"), to = $state(todayIso());
  let busy = $state(false);
  let done = $state("");
  function range(): [string, string] {
    const m = monthIso();
    if (period === "month") return [m + "-01", ""];
    if (period === "last") return [addMonths(m, -1) + "-01", addDays(m + "-01", -1)];
    if (period === "year") return [todayIso().slice(0, 4) + "-01-01", ""];
    if (period === "custom") return [from, to];
    return ["", ""];
  }
  async function run() {
    busy = true;
    try {
      const [f, t] = range();
      done = await api.exportFile(fmt, f, t);
    } catch (e) {
      showToast(String(e));
    } finally {
      busy = false;
    }
  }
</script>

<SheetFrame title="Export" {desktop} left="Close" onleft={closeSheet}>
  <div class="flex flex-col gap-4 pt-1">
    <Seg label="Format" items={[{ id: "csv", name: "CSV (spreadsheet)" }, { id: "pdf", name: "PDF (report)" }]} value={fmt} onchange={(v) => ((fmt = v), (done = ""))} />
    <div class="card overflow-hidden">
      <label class="flex min-h-12 items-center gap-3 px-4"><span class="flex-1 text-[15px] text-ink2">Period</span>
        <select bind:value={period} class="appearance-none bg-transparent py-3 text-right text-[15px] outline-none" onchange={() => (done = "")}>
          <option value="month">This month</option><option value="last">Last month</option><option value="year">This year</option><option value="all">Everything</option><option value="custom">Custom dates</option></select></label>
      {#if period === "custom"}
        <label class="flex min-h-12 items-center gap-3 border-t border-sep px-4"><span class="flex-1 text-[15px] text-ink2">From</span><input type="date" bind:value={from} class="bg-transparent py-3 text-right text-[15px] outline-none" /></label>
        <label class="flex min-h-12 items-center gap-3 border-t border-sep px-4"><span class="flex-1 text-[15px] text-ink2">To</span><input type="date" bind:value={to} class="bg-transparent py-3 text-right text-[15px] outline-none" /></label>
      {/if}
    </div>
    <div class="flex gap-3 rounded-[14px] bg-danger-soft px-4 py-3 text-[14px] leading-snug text-ink2">
      <Ic name="bell" size={18} class="mt-0.5 shrink-0 text-danger" />
      <span><b class="text-ink">Not encrypted.</b> This file holds your transactions in plain text. Anyone who gets it can read it. When sync is on, it also makes encrypted backups automatically.</span>
    </div>
    <button type="button" disabled={busy} class="press h-[52px] w-full rounded-[14px] bg-accent text-[17px] font-semibold text-on-accent disabled:opacity-40" onclick={run}>{busy ? "Exporting…" : `Export ${fmt.toUpperCase()}`}</button>
    {#if done}<p class="fade m-0 rounded-[14px] bg-accent-soft px-4 py-3 text-[14px] break-all text-accent-ink">Saved to {done}</p>{/if}
  </div>
</SheetFrame>
