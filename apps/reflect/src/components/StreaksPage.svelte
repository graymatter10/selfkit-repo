<script lang="ts">
  import { app, openTab } from "../lib/state.svelte";
  import Icon from "./Icon.svelte";
  import StreakCard from "./StreakCard.svelte";
  let { desktop }: { desktop: boolean } = $props();
  const list = $derived(app.streaks ?? []);
  const change = () => openTab("settings");
</script>

{#snippet hint()}
  <div class="hint"><Icon name="hab" size={20} /><div><b>Later: habits you tick yourself</b>Things like gym or reading will be able to join your streaks here.</div></div>
{/snippet}

{#if desktop}
  <div class="max-w-[1000px]">
    <div class="mb-5 flex items-end gap-4"><div class="flex-1"><h1 class="dtitle">Streaks</h1><p class="mt-1.5 text-[14px] text-muted">Days in a row you kept a line you set. Last 30 days below; today is the outlined square.</p></div>
      <button type="button" class="btn press" onclick={() => ((app.adding = true), openTab("settings"))}><Icon name="plus" size={18} stroke={2.1} />New streak</button></div>
    {#if list.length}<div class="grid grid-cols-2 gap-3.5">{#each list as s}<StreakCard {s} n={30} onchange={change} />{/each}</div>
    {:else}<p class="small">No streaks turned on. Turn some on in Settings.</p>{/if}
    <div class="mt-3.5">{@render hint()}</div>
  </div>
{:else}
  <p class="intro">Days in a row you kept a line you set. Last 21 days below.</p>
  <div class="flex flex-col gap-3">
    {#each list as s}<StreakCard {s} n={21} onchange={change} />{:else}<p class="small">No streaks turned on. Turn some on in Settings.</p>{/each}
    <button type="button" class="btn press w-full" onclick={() => ((app.adding = true), openTab("settings"))}><Icon name="plus" size={18} stroke={2.1} />New streak</button>
    {@render hint()}
  </div>
{/if}
