<script lang="ts">
  // Same pattern as Dusk's reviewed Connections cards: strength badge, one
  // sentence, two compared bars, details hidden behind "Details".
  import type { Conn } from "../lib/api";
  import { APP_NAMES, unitValue } from "../lib/fmt";
  import Icon from "./Icon.svelte";
  import Src from "./Src.svelte";
  import { app } from "../lib/state.svelte";
  let { c, sym, desk = false, open = false, ontoggle }: { c: Conn; sym: string; desk?: boolean; open?: boolean; ontoggle: () => void } = $props();
  // "Needs Chrono" for an app that is installed but has no data yet reads wrong (L30).
  function missingText(m: string): string {
    const name = m.startsWith("Needs ") ? m.slice(6) : "";
    const key = (Object.entries(APP_NAMES) as [string, string][]).find(([, n]) => n === name)?.[0];
    const found = key && app.sources?.found.find(([k]) => k === key)?.[1];
    if (found) return `No ${name} data yet. This link shows up once ${name} has recorded some.`;
    return name ? `${m}. This link shows up once it's on this device.` : `${m}.`;
  }
  const STR = ["No clear link", "Weak link", "Moderate link", "Strong link"];
  const scale = $derived(Math.max(...c.groups.map((g) => g.value), 1) * 1.15);
  const fmt = (v: number) => unitValue(c.unit, sym, v);
  const bmax = $derived(Math.max(1, ...c.buckets.map((b) => b.value ?? 0)) * 1.1);
</script>

{#snippet badge()}
  <span class="inline-flex h-6 shrink-0 items-center gap-1.5 rounded-full pr-[9px] pl-2 text-[12px] font-semibold whitespace-nowrap {c.strength === 3 ? 'bg-accent-soft text-accent' : 'bg-fill text-ink2'}">
    <span class="inline-flex items-end gap-0.5">{#each [1, 2, 3] as k}<i class="w-[3px] rounded-[1px] {k <= c.strength ? 'bg-current' : 'bg-faint'}" style="height:{4 + k * 3}px"></i>{/each}</span>{STR[c.strength]}</span>
{/snippet}
{#snippet bars()}
  <div class="flex flex-col gap-[11px]">
    {#each c.groups as g, i}
      <div>
        <div class="mb-[5px] flex items-baseline gap-2 text-[13.5px] text-ink2"><span class="min-w-0 truncate">{g.label}</span><b class="num ml-auto text-[15px] font-[650] text-ink">{fmt(g.value)}{#if c.unit === "tasks"}<small class="ml-[3px] text-[12.5px] font-medium text-muted">tasks</small>{/if}</b></div>
        <div class="h-2.5 overflow-hidden rounded-[5px] bg-fill2"><i class="block h-full rounded-[5px] {i === 0 && c.strength > 0 ? 'bg-accent' : 'bg-bar2'}" style="width:{(g.value / scale) * 100}%"></i></div>
      </div>
    {/each}
  </div>
{/snippet}

{#if !c.available}
  <div class="missing px-4 py-3.5">
    <div class="flex items-center gap-2 text-[12.5px] font-semibold text-muted"><span class="min-w-0 flex-1 truncate">{c.topic}</span><Src apps={c.apps} /></div>
    <p class="mt-1.5 text-[14px] text-ink2">{missingText(c.missing)}</p>
  </div>
{:else}
  <div class="card {desk ? 'px-[22px] pt-[18px] pb-0.5' : 'px-4 pt-4 pb-1'}">
    {#if desk}
      <div class="grid grid-cols-2 items-center gap-9">
        <div><div class="flex items-center gap-2 text-[12.5px] font-semibold text-muted">{#if c.enough}{@render badge()}{/if}<span class="truncate">{c.topic}</span></div>
          <div class="mt-2.5 text-[19px] leading-[1.3] font-[650] tracking-[-0.015em] {c.enough ? '' : 'text-muted'}">{c.finding}</div></div>
        {#if c.enough}{@render bars()}{/if}
      </div>
    {:else}
      <div class="flex items-center gap-2 text-[12.5px] font-semibold text-muted"><span class="min-w-0 flex-1 truncate">{c.topic}</span>{#if c.enough}{@render badge()}{/if}</div>
      <div class="mt-2 mb-3 text-[17px] leading-[1.3] font-[650] tracking-[-0.015em] {c.enough ? '' : 'text-[15px] text-muted'}">{c.finding}</div>
      {#if c.enough}{@render bars()}{/if}
    {/if}
    <div class="mt-2.5 flex min-h-11 items-center gap-2.5 border-t border-sep text-[12.5px] text-muted {desk ? 'mt-3.5' : ''}">
      <span class="num">{c.pairs} {c.per}</span><Src apps={c.apps} />
      {#if c.enough}<button type="button" class="press ml-auto inline-flex h-11 items-center gap-0.5 pl-3 text-[13px] font-semibold text-accent" aria-expanded={open} onclick={ontoggle}>{open ? "Hide details" : "Details"}<Icon name={open ? "up" : "down"} size={16} stroke={2.2} /></button>{/if}
    </div>
    {#if open}
      <div class="fade pt-1 pb-2 {desk ? 'max-w-[600px]' : ''}">
        <p class="mb-1.5 text-[12px] text-muted">{c.measure}</p>
        <table class="num w-full border-collapse text-[13px]">
          <tbody>
            {#each c.buckets as b}
              <tr><td class="py-[5px] text-ink2">{b.label}</td>
                <td class="w-[30%] px-3"><div class="h-1.5 overflow-hidden rounded-[3px] bg-fill2"><i class="block h-full rounded-[3px] bg-accent2" style="width:{((b.value ?? 0) / bmax) * 100}%"></i></div></td>
                <td class="w-[64px] text-right font-semibold text-ink">{b.value == null ? "–" : fmt(b.value)}</td><td class="w-[74px] text-right text-muted">{b.n} {c.per}</td></tr>
            {/each}
          </tbody>
        </table>
        <p class="mt-2 text-[12px] text-muted">Correlation {c.r?.toFixed(2).replace("-", "−")} over {c.pairs} {c.per}. Data from {c.apps.map((a) => APP_NAMES[a]).join(" and ")}. A link doesn't prove a cause.</p>
      </div>
    {/if}
  </div>
{/if}
