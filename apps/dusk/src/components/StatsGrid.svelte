<script lang="ts">
  // The four headline numbers: 7-night average, sleep debt, consistency, bed on time.
  import type { Dashboard } from "../lib/api";
  import { clock, dur } from "../lib/fmt";
  let { dash, cols = 2 }: { dash: Dashboard; cols?: number } = $props();
  const s7 = $derived(dash.s7);
  const s14 = $derived(dash.s14);
  const diff = $derived(s7.avg_min != null && s7.prev_avg_min != null ? s7.avg_min - s7.prev_avg_min : null);
  const goalH = $derived(dur(dash.goals.goal_min).replace(" 00m", ""));
  const grace = $derived(clock(Number(dash.goals.bedtime.slice(0, 2)) * 60 + Number(dash.goals.bedtime.slice(3)) + 15));
</script>

<div class="grid gap-2.5" style="grid-template-columns:repeat({cols},minmax(0,1fr))">
  <div class="card px-4 py-3.5">
    <div class="text-[13px] font-medium text-muted">Average, last 7 nights</div>
    <div class="num mt-[3px] text-[24px] font-[650] tracking-[-0.02em]">{s7.avg_min != null ? dur(s7.avg_min) : "–"}</div>
    <div class="mt-[3px] text-[12.5px] text-muted">{diff == null ? `${s7.nights} nights with data` : `${diff >= 0 ? "+" : "−"}${dur(Math.abs(diff))} on the week before`}</div>
  </div>
  <div class="card px-4 py-3.5">
    <div class="text-[13px] font-medium text-muted">Sleep debt</div>
    <div class="num mt-[3px] text-[24px] font-[650] tracking-[-0.02em]">{s14.debt_min ? dur(s14.debt_min) : "None"}</div>
    <div class="mt-[3px] text-[12.5px] text-muted">Short of {goalH}, last 14 nights</div>
  </div>
  <div class="card px-4 py-3.5">
    <div class="text-[13px] font-medium text-muted">Consistency</div>
    <div class="num mt-[3px] text-[24px] font-[650] tracking-[-0.02em]">{s14.consistency ?? "–"}<span class="text-[15px] font-[550] text-muted">&nbsp;/ 100</span></div>
    <div class="mt-[3px] text-[12.5px] text-muted">{s14.bed_sd_min != null ? `Bedtime varies ±${dur(s14.bed_sd_min)}` : "Needs 3 nights"}</div>
  </div>
  <div class="card px-4 py-3.5">
    <div class="text-[13px] font-medium text-muted">Bed on time</div>
    <div class="num mt-[3px] text-[24px] font-[650] tracking-[-0.02em]">{s7.bedtime_hits}<span class="text-[15px] font-[550] text-muted">&nbsp;of {s7.bedtime_total} nights</span></div>
    <div class="mt-[3px] text-[12.5px] text-muted">By {grace} · goal {dash.goals.bedtime}</div>
  </div>
</div>
