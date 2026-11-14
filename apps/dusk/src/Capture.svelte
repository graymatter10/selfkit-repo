<script lang="ts">
  // Desktop rate-sleep popup (global shortcut, default Super+Shift+S, or
  // `dusk --rate`): "How did you sleep?" → one key or click → saved, closes.
  import { onMount } from "svelte";
  import Rating from "./components/Rating.svelte";
  import { api, type Dashboard } from "./lib/api";
  import { QUALITY, dur, nightRange } from "./lib/fmt";

  let dash: Dashboard | null = $state(null);
  let done = $state("");
  let err = $state("");
  const night = $derived(dash?.nights.find((n) => n.night === dash?.current));
  document.documentElement.style.background = "transparent";
  document.body.style.background = "transparent";

  onMount(async () => {
    try {
      dash = await api.dashboard();
      const p = await api.device();
      if (p.theme !== "system") document.documentElement.dataset.theme = p.theme;
    } catch (e) {
      err = String(e);
    }
  });
  async function rate(q: number) {
    if (!dash || done) return;
    try {
      await api.logSet(dash.current, { quality: q });
      done = `Rated ${q} · ${QUALITY[q]}`;
      setTimeout(() => api.closeCapture(), 700);
    } catch (e) {
      err = String(e);
    }
  }
  function keys(e: KeyboardEvent) {
    if (/^[1-5]$/.test(e.key)) rate(Number(e.key));
    else if (e.key === "Escape") api.closeCapture();
  }
</script>

<svelte:window onkeydown={keys} onblur={() => !done && api.closeCapture()} />

<div class="flex h-screen items-center justify-center p-2">
  <div class="card w-full p-5" style="box-shadow:0 10px 40px rgba(0,0,0,.25)">
    <div class="mb-1 flex items-baseline">
      <h1 class="m-0 flex-1 text-[17px] font-[650]">How did you sleep?</h1>
      <span class="text-[12px] text-muted">1–5 · Esc</span>
    </div>
    <p class="mt-0 mb-4 text-[13px] text-muted">
      {#if night}Last night · {nightRange(night.night)}{night.asleep_min ? ` · ${dur(night.asleep_min)}` : ""}{:else}Last night{/if}
    </p>
    {#if done}<p role="status" class="m-0 flex h-16 items-center justify-center text-[16px] font-semibold text-accent">{done}</p>
    {:else}<Rating value={night?.quality ?? null} onrate={rate} />{/if}
    {#if err}<p role="alert" class="mt-2 mb-0 text-[13px] text-danger">{err}</p>{/if}
  </div>
</div>
