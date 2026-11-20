<script lang="ts">
  // Pick a currency: common ones, plus every code we have a rate for.
  import { CURRENCIES } from "../lib/fmt";
  import { app, closeSheet, main } from "../lib/state.svelte";
  import Ic from "./Ic.svelte";
  import SheetFrame from "./SheetFrame.svelte";
  let { value, onpick, desktop = false }: { value: string; onpick: (c: string) => void; desktop?: boolean } = $props();
  let q = $state("");
  const all = $derived.by(() => {
    const set = new Set([main(), ...CURRENCIES, ...Object.keys(app.snap?.rates.rates ?? {}), ...Object.keys(app.snap?.rates.manual ?? {})]);
    return [...set];
  });
  const names = new Intl.DisplayNames(undefined, { type: "currency" });
  const nm = (c: string) => { try { return names.of(c) ?? c; } catch { return c; } };
  const shown = $derived(all.filter((c) => !q || c.toLowerCase().includes(q.toLowerCase()) || nm(c).toLowerCase().includes(q.toLowerCase())));
  function pick(c: string) {
    onpick(c);
    closeSheet();
  }
  // Only real ISO 4217 codes: a typo like "UDS" or "XYZ" was accepted and counted 1:1 (L2-26).
  const iso: Set<string> = (() => {
    try { return new Set((Intl as unknown as { supportedValuesOf(k: string): string[] }).supportedValuesOf("currency")); } catch { return new Set(); }
  })();
  const strict = new Intl.DisplayNames(undefined, { type: "currency", fallback: "none" });
  const known = (c: string) => iso.size ? iso.has(c) : (() => { try { return strict.of(c) !== undefined; } catch { return false; } })();
  const custom = $derived(/^[A-Za-z]{3}$/.test(q.trim()) && !all.includes(q.trim().toUpperCase()) && known(q.trim().toUpperCase()));
  // Desktop: typing goes straight into the search (phones would pop the keyboard up).
  let search = $state<HTMLInputElement>();
  $effect(() => { if (desktop) search?.focus(); });
</script>

<SheetFrame title="Currency" {desktop} left="Close" onleft={closeSheet}>
  <input bind:this={search} bind:value={q} placeholder="Search or type a code" class="field mb-3 text-[15px]" aria-label="Search currencies" />
  <div class="card overflow-hidden">
    {#if !custom && !shown.length && q.trim()}<p class="m-0 px-4 py-3 text-[14px] text-muted">No currency “{q.trim().toUpperCase()}”.</p>{/if}
    {#if custom}<button type="button" class="hl flex h-12 w-full items-center px-4 text-left text-[15px] text-accent" onclick={() => pick(q.trim().toUpperCase())}>Use {q.trim().toUpperCase()}</button>{/if}
    {#each shown as c (c)}
      <button type="button" class="hl flex h-12 w-full items-center gap-3 border-b border-sep px-4 text-left last:border-0" onclick={() => pick(c)}>
        <span class="w-12 text-[15px] font-semibold">{c}</span><span class="flex-1 truncate text-[14px] text-muted">{nm(c)}</span>
        {#if c === value}<Ic name="check" size={18} stroke={2.4} class="text-accent" />{/if}
      </button>
    {/each}
  </div>
</SheetFrame>
