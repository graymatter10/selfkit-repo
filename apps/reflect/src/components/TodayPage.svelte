<script lang="ts">
  import { app } from "../lib/state.svelte";
  import AppList from "./AppList.svelte";
  import Icon from "./Icon.svelte";
  import Ribbon from "./Ribbon.svelte";
  import Src from "./Src.svelte";
  import Tile from "./Tile.svelte";
  import type { AppKey } from "../lib/api";
  let { desktop }: { desktop: boolean } = $props();
  const o = $derived(app.overview!);
  const t = $derived(o.today);
  const found = $derived(Object.fromEntries(app.sources?.found ?? []) as Record<AppKey, boolean>);
  const missing = $derived((["hisab", "dusk", "agenda", "chrono"] as AppKey[]).filter((k) => !t.has[k]));
  const anyData = $derived(t.has.chrono || t.has.hisab || t.has.agenda || t.has.dusk);
  const names: Record<AppKey, string> = { chrono: "Chrono", hisab: "Hisab", agenda: "Agenda", dusk: "Dusk" };
  const list = (a: AppKey[]) => a.map((k) => names[k]).join(a.length === 2 ? " and " : ", ").replace(/, ([^,]*)$/, " and $1");
  const ribbonApps = $derived((["chrono", "dusk", "hisab", "agenda"] as AppKey[]).filter((k) => t.has[k]));
</script>

{#snippet tiles()}
  <Tile label="Screen time" app="chrono" tile={t.screen} installed={found.chrono ?? false} mobile={o.mobile} />
  <Tile label="Spent" app="hisab" tile={t.spent} installed={found.hisab ?? false} mobile={o.mobile} />
  <Tile label="Tasks" app="agenda" tile={t.tasks} installed={found.agenda ?? false} mobile={o.mobile} />
  <Tile label="Last night" app="dusk" tile={t.sleep} installed={found.dusk ?? false} mobile={o.mobile} />
{/snippet}
{#snippet ribbon()}
  <div class="card p-4"><div class="ctitle">Your day<span class="ml-auto"><Src apps={ribbonApps} /></span></div><Ribbon r={t.ribbon} has={t.has} /></div>
{/snippet}
{#snippet apps()}
  <div class="card p-4"><div class="ctitle">Most used today<span class="ml-auto"><Src apps={["chrono"]} /></span></div>
    {#if t.top_apps.length}<AppList rows={t.top_apps} />{:else}<p class="small mt-2">Nothing yet today.</p>{/if}</div>
{/snippet}
{#snippet missingNote()}
  {#if missing.length && anyData}
    <div class="hint"><Icon name="box" size={20} /><div><b>{missing.length === 1 ? `${list(missing)} isn't` : `${list(missing)} aren't`} on this device</b>{o.mobile ? "On phones, Reflect can't read other apps yet. That comes later." : "Install them and their stats show up here by themselves. Nothing to set up."}</div></div>
  {/if}
{/snippet}

{#if !anyData}
  <div class="mx-auto max-w-[520px] px-2 pt-10 text-center">
    <span class="mx-auto grid h-14 w-14 place-items-center rounded-[16px] bg-accent-soft text-accent"><Icon name="bars" size={26} stroke={2.2} /></span>
    <h1 class="headline mt-5">Nothing to show yet.</h1>
    <p class="subline">Reflect brings together Chrono (screen time), Hisab (money), Agenda (tasks) and Dusk (sleep). It only reads them and never changes anything.</p>
    <p class="subline">{o.mobile ? "On phones, apps can't share their data with Reflect yet. That comes later. For now, open Reflect on your PC." : "Install one of them, use it for a day, and your summary appears here."}</p>
  </div>
{:else if desktop}
  <div class="max-w-[1000px]">
    <div class="mt-0.5 mb-[22px]"><div class="date">{t.date_label}</div><h1 class="headline !text-[26px]">{t.headline}</h1><p class="subline max-w-[640px] !text-[15px]">{t.subline}</p></div>
    <div class="grid grid-cols-4 gap-3.5">{@render tiles()}</div>
    <div class="mt-3.5 grid grid-cols-[1.55fr_1fr] items-start gap-3.5">
      <div class="flex flex-col gap-3">{#if t.has.chrono || t.has.dusk || t.has.hisab || t.has.agenda}{@render ribbon()}{/if}{@render missingNote()}</div>
      {#if t.has.chrono}{@render apps()}{/if}
    </div>
  </div>
{:else}
  <div class="px-1 pt-0.5 pb-4"><div class="date">{t.date_label}</div><h1 class="headline">{t.headline}</h1><p class="subline">{t.subline}</p></div>
  <div class="grid grid-cols-2 gap-3">{@render tiles()}</div>
  <div class="mt-3 flex flex-col gap-3">{@render ribbon()}{@render missingNote()}{#if t.has.chrono}{@render apps()}{/if}</div>
{/if}
