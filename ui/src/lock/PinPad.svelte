<script lang="ts">
  // Number pad for the app lock on phones (own design: soft rounded keys, no letters).
  // `len` > 0: that many dots and `onfull` fires on the last digit; 0: dots grow as you type.
  let {
    value = $bindable(""),
    len = 0,
    max = 12,
    disabled = false,
    shake = false,
    bio = null,
    onfull,
    onkey,
    onbio,
  }: {
    value?: string; len?: number; max?: number; disabled?: boolean; shake?: boolean;
    bio?: "fingerprint" | "credential" | null;
    onfull?: (v: string) => void; onkey?: () => void; onbio?: () => void;
  } = $props();

  const slots = $derived(len > 0 ? len : Math.max(4, value.length));
  function press(d: string) {
    if (disabled) return;
    onkey?.();
    const lim = len > 0 ? len : max;
    if (value.length >= lim) return;
    value += d;
    navigator.vibrate?.(8);
    if (len > 0 && value.length === len) onfull?.(value);
  }
  function back() {
    if (disabled) return;
    onkey?.();
    value = value.slice(0, -1);
  }
</script>

<div class="lp">
  <div class="lp-dots" class:lp-shake={shake} aria-label={`${value.length} digits entered`} role="status">
    {#each Array(slots) as _, i}<span class:lp-on={i < value.length}></span>{/each}
  </div>
  <div class="lp-grid" class:lp-off={disabled}>
    {#each ["1", "2", "3", "4", "5", "6", "7", "8", "9"] as d}
      <button type="button" class="lp-key" {disabled} onclick={() => press(d)}>{d}</button>
    {/each}
    {#if bio}
      <button type="button" class="lp-key lp-fn" aria-label={bio === "fingerprint" ? "Use fingerprint" : "Use phone screen lock"} onclick={() => onbio?.()}>
        <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          {#if bio === "fingerprint"}
            <path d="M6.6 5.4A8 8 0 0 1 20 11.5v1"/><path d="M4 11.5a8 8 0 0 1 .9-3.7"/><path d="M8 12a4 4 0 0 1 8 0v1.5a10 10 0 0 0 .9 4.2"/><path d="M12 11.5v2.5a14 14 0 0 0 2 7"/><path d="M8.3 15.5a17 17 0 0 0 1.4 4.8"/><path d="M4.2 15.4a11 11 0 0 0 .8 2.3"/>
          {:else}
            <rect x="6.5" y="2.5" width="11" height="19" rx="2.5"/><rect x="9.5" y="10.5" width="5" height="4" rx="1"/><path d="M10.5 10.5V9a1.5 1.5 0 0 1 3 0v1.5"/>
          {/if}
        </svg>
      </button>
    {:else}<span></span>{/if}
    <button type="button" class="lp-key" {disabled} onclick={() => press("0")}>0</button>
    <button type="button" class="lp-key lp-fn" aria-label="Delete" disabled={disabled || !value} onclick={back}>
      <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9 5h10.5A1.5 1.5 0 0 1 21 6.5v11a1.5 1.5 0 0 1-1.5 1.5H9l-6-7z"/><path d="M12.5 9.5l5 5M17.5 9.5l-5 5"/></svg>
    </button>
  </div>
</div>

<style>
  .lp { display: flex; flex-direction: column; align-items: center; width: 100%; }
  .lp-dots { display: flex; gap: 14px; height: 18px; align-items: center; margin-bottom: 30px; }
  .lp-dots span { width: 12px; height: 12px; border-radius: 6px; border: 1.6px solid var(--lk-mut); box-sizing: border-box; transition: background 0.1s, border-color 0.1s, transform 0.1s; }
  .lp-dots span.lp-on { background: var(--lk-ink); border-color: var(--lk-ink); transform: scale(1.05); }
  .lp-shake { animation: lp-shake 0.38s ease-in-out; }
  .lp-shake span { border-color: var(--lk-bad) !important; }
  .lp-shake span.lp-on { background: var(--lk-bad); }
  @keyframes lp-shake { 0%, 100% { transform: translateX(0); } 20% { transform: translateX(-9px); } 40% { transform: translateX(8px); } 60% { transform: translateX(-6px); } 80% { transform: translateX(4px); } }
  .lp-grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 12px 16px; width: min(300px, 100%); }
  .lp-grid.lp-off { opacity: 0.45; }
  .lp-key { height: 62px; border-radius: 20px; border: 0; background: var(--lk-key); color: var(--lk-ink); font: inherit; font-size: 26px; font-weight: 450; font-variant-numeric: tabular-nums; display: grid; place-items: center; cursor: pointer; -webkit-tap-highlight-color: transparent; touch-action: manipulation; transition: background 0.08s, transform 0.08s; }
  .lp-key:active:not(:disabled) { background: var(--lk-key-down); transform: scale(0.97); }
  .lp-key.lp-fn { background: transparent; color: var(--lk-mut); }
  .lp-key.lp-fn:active:not(:disabled) { background: var(--lk-key); }
  .lp-key:disabled { cursor: default; }
  .lp-key.lp-fn:disabled { opacity: 0.35; }
</style>
