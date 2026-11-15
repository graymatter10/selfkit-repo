<script lang="ts">
  // "Cash is in USD — switch to BDT?" Accounts whose currency the user never
  // chose but that differ from the main currency (snapshot.stale_accounts,
  // computed in Rust: currency.rs). Settings shows it until answered; Home and
  // Accounts show one at a time as a gentle banner (× hides it there for good).
  // Switch relabels (numbers kept); Keep marks the currency as chosen.
  import { showToast } from "@selfkit/ui";
  import { api } from "../lib/api";
  import { app, main, refresh } from "../lib/state.svelte";
  import Ic from "./Ic.svelte";
  let { banner = false }: { banner?: boolean } = $props();
  const KEY = "hisab.staleSeen";
  const seenGet = (): string[] => { try { return JSON.parse(localStorage.getItem(KEY) ?? "[]"); } catch { return []; } };
  let seen = $state<string[]>(seenGet());
  // Banner: one account at a time (the rest wait in Settings), so Home stays calm.
  const list = $derived.by(() => {
    const all = app.snap?.stale_accounts ?? [];
    return banner ? all.filter((a) => !seen.includes(a.id + ":" + main())).slice(0, 1) : all;
  });
  function hide(id: string) {
    seen = [...seen, id + ":" + main()];
    try { localStorage.setItem(KEY, JSON.stringify(seen)); } catch { /* private mode: hidden for this session */ }
  }
  async function act(id: string, keep: boolean) {
    try {
      if (keep) await api.accountKeepCurrency(id);
      else await api.accountRelabel(id, main());
      await refresh();
    } catch (e) {
      showToast(String(e));
    }
  }
</script>

{#each list as a (a.id)}
  <div class={banner ? "card relative mb-2.5 px-4 py-3 text-[14px]" : "border-b border-sep px-4 py-3 text-[14px]"}>
    {#if banner}<button type="button" aria-label="Hide" class="press absolute top-1.5 right-1.5 flex h-8 w-8 items-center justify-center rounded-full text-muted" onclick={() => hide(a.id)}><Ic name="close" size={15} stroke={2.2} /></button>{/if}
    <p class="m-0 font-semibold {banner ? 'pr-7' : ''}">{a.name} is in {a.currency} — switch to {main()}?</p>
    <p class="m-0 mt-0.5 text-[13px] text-muted">Your main currency is {main()}. {a.entries} {a.entries === 1 ? "entry keeps its number" : "entries keep their numbers"}, only the label changes.</p>
    <div class="mt-2 flex gap-5">
      <button type="button" class="press font-semibold text-accent" onclick={() => act(a.id, false)}>Switch to {main()}</button>
      <button type="button" class="press text-muted" onclick={() => act(a.id, true)}>Keep {a.currency}</button>
    </div>
  </div>
{/each}
