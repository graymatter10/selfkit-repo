<script lang="ts">
  // Connections: topic + strength, one plain finding, two compared bars, and
  // "Details" with the 4-group breakdown and the correlation.
  import type { Connection } from "../lib/api";
  import { app, loadConnections } from "../lib/state.svelte";
  import Icon from "./Icon.svelte";

  let { desktop }: { desktop: boolean } = $props();
  let open = $state<Record<string, boolean>>({});
  $effect(() => {
    if (!app.connections) loadConnections();
  });
  const STR = ["No clear link", "Weak link", "Moderate link", "Strong link"];
  const fmt = (c: Connection, v: number | null) => (v == null ? "–" : c.id === "money" ? `${c.unit}${Math.round(v)}` : v.toFixed(1));
  const unit = (c: Connection) => (c.id === "money" ? "" : c.unit);
  const scale = (c: Connection) => (c.id === "late_use" ? 5 : Math.max(...c.groups.map((g) => g.avg ?? 0), ...c.buckets.map((b) => b.avg ?? 0), 1) * 1.15);
  const basis = (c: Connection) => (c.pairs === 1 ? `1 ${c.per.replace(/s$/, "")}` : `${c.pairs} ${c.per}`);
</script>

{#snippet strength(s: number)}
  <span class="inline-flex h-6 items-center gap-1.5 rounded-full pr-[9px] pl-2 text-[12px] font-semibold whitespace-nowrap {s === 3 ? 'bg-accent-soft text-accent' : 'bg-fill text-ink2'}">
    <span class="inline-flex items-end gap-[2px]" aria-hidden="true">{#each [1, 2, 3] as k}<i class="w-[3px] rounded-[1px] {k <= s ? 'bg-current' : 'bg-faint'}" style="height:{4 + k * 3}px"></i>{/each}</span>{STR[s]}
  </span>
{/snippet}

{#snippet bars(c: Connection)}
  <div class="flex flex-col gap-[11px]">
    {#each c.groups as g, i}
      <div>
        <div class="mb-[5px] flex items-baseline text-[13.5px] text-ink2"><span>{g.label}</span><b class="num ml-auto text-[15px] font-[650] text-ink">{fmt(c, g.avg)}{#if unit(c)}<small class="ml-[3px] text-[12.5px] font-medium text-muted">{unit(c)}</small>{/if}</b></div>
        <div class="h-2.5 overflow-hidden rounded-[5px] bg-fill2"><i class="block h-full rounded-[5px] {i === 0 ? 'bg-accent' : 'bg-bar2'}" style="width:{((g.avg ?? 0) / scale(c)) * 100}%"></i></div>
      </div>
    {/each}
  </div>
{/snippet}

{#snippet details(c: Connection)}
  {@const max = Math.max(...c.buckets.map((b) => b.avg ?? 0), 0.1) * 1.1}
  <div class="pt-1 pb-1.5 {desktop ? 'max-w-[560px]' : ''}">
    <div class="pb-1.5 text-[12px] text-muted">{c.measure}, split by amount</div>
    <table class="num w-full border-collapse text-[13px]"><tbody>
      {#each c.buckets as bk}
        <tr><td class="w-[44%] py-[5px] text-ink2">{bk.label}</td>
          <td><div class="mx-3.5 h-1.5 overflow-hidden rounded-[3px] bg-fill2"><i class="block h-full rounded-[3px] bg-accent2" style="width:{((bk.avg ?? 0) / max) * 100}%"></i></div></td>
          <td class="w-[52px] text-right font-semibold">{fmt(c, bk.avg)}</td><td class="w-20 text-right text-muted">{bk.nights} {c.per === "days" ? "days" : "nights"}</td></tr>
      {/each}
    </tbody></table>
    <div class="mt-2 text-[12px] text-muted">{c.r != null ? `Correlation ${c.r.toFixed(2).replace("-", "−")} over ${basis(c)}. ` : ""}Data from {c.src}. A link doesn't prove a cause.</div>
  </div>
{/snippet}

{#snippet card(c: Connection)}
  <div class="card {desktop ? 'px-[22px] pt-[18px] pb-0.5' : 'px-4 pt-4 pb-1'} {c.available ? '' : 'opacity-75'}">
    {#if desktop}
      <div class="grid grid-cols-2 items-center gap-9">
        <div>
          <div class="flex items-center gap-2 text-[12.5px] font-semibold text-muted">{#if c.enough}{@render strength(c.strength)}{/if}<span>{c.topic}</span></div>
          <div class="mt-2.5 text-[19px] leading-[1.3] font-[650] tracking-[-0.015em]">{c.finding}</div>
        </div>
        {#if c.enough}{@render bars(c)}{/if}
      </div>
    {:else}
      <div class="flex items-center gap-2 text-[12.5px] font-semibold text-muted"><span class="flex-1">{c.topic}</span>{#if c.enough}{@render strength(c.strength)}{/if}</div>
      <div class="mt-2 mb-3 text-[17px] leading-[1.3] font-[650] tracking-[-0.015em]">{c.finding}</div>
      {#if c.enough}{@render bars(c)}{/if}
    {/if}
    <div class="mt-2.5 flex min-h-11 items-center border-t border-sep text-[12.5px] text-muted {desktop ? 'mt-3.5' : ''}">
      <span class="num">{c.available ? `${basis(c)} · from ${c.src}` : c.src === "Hisab" || c.src === "Agenda" ? `${c.src} isn't on this device` : ""}</span>
      {#if c.enough}
        <button type="button" class="press ml-auto inline-flex h-11 items-center gap-0.5 pl-3 text-[13px] font-semibold text-accent" aria-expanded={!!open[c.id]} onclick={() => (open[c.id] = !open[c.id])}>{open[c.id] ? "Hide details" : "Details"}<Icon name={open[c.id] ? "up" : "down"} size={16} stroke={2.2} /></button>
      {/if}
    </div>
    {#if open[c.id]}{@render details(c)}{/if}
  </div>
{/snippet}

<div class={desktop ? "max-w-[920px]" : ""}>
  {#if desktop}<h1 class="m-0 text-[26px] font-bold tracking-[-0.03em]">Connections</h1>{/if}
  <p class="mx-0.5 mt-1.5 mb-3.5 text-[13.5px] text-muted {desktop ? 'mb-5 text-[14px]' : ''}">Last 3 months. A link doesn't prove a cause.</p>
  {#if app.connections}
    <div class="flex flex-col gap-3">{#each app.connections as c (c.id)}{@render card(c)}{/each}</div>
  {:else}
    <p class="pt-10 text-center text-muted">Working it out…</p>
  {/if}
</div>
