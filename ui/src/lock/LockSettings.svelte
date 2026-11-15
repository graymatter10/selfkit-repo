<script lang="ts">
  // Shared "App lock" settings section (docs/components/app-lock.md). Same look as the
  // shared Sync section; the app maps its tokens onto --lock-* (docs/components/app-lock.md, Theming).
  import { onMount } from "svelte";
  import PinPad from "./PinPad.svelte";
  import { TIMEOUT_LABELS, lockApi, lockBridge, lockState, word, wrongText, type Kind, type LockView } from "./api.svelte";

  let { phone = false, appName, heading = true }: { phone?: boolean; appName: string; heading?: boolean } = $props();

  const v = $derived(lockState.view);
  const android = $derived(v?.platform === "android");
  let bio = $state<"fingerprint" | "credential" | "none" | "unavailable">("unavailable");
  let flash = $state("");

  // Sheet: set a lock, change it, or turn it off.
  let flow = $state<"" | "set" | "change" | "off">("");
  let step = $state<"current" | "new" | "confirm">("new");
  let kind = $state<Kind>("pin");
  let current = $state(""), first = $state(""), entry = $state("");
  let err = $state("");
  let busy = $state(false);
  let shake = $state(false);

  onMount(async () => {
    bio = lockBridge()?.biometricStatus() ?? "unavailable";
    if (!lockState.view) { try { lockState.view = await lockApi.status(); } catch (e) { flash = String(e); } }
  });

  function open(f: "set" | "change" | "off") {
    flow = f; err = ""; current = first = entry = "";
    kind = f === "set" ? "pin" : (v?.kind ?? "pin");
    step = f === "set" ? "new" : "current";
    if (f === "change") kind = "pin";
  }
  const close = () => { flow = ""; current = first = entry = ""; };
  function bad(text: string) {
    err = text; entry = ""; shake = true;
    setTimeout(() => (shake = false), 420);
  }

  // What is being typed right now, and how.
  const curKind = $derived<Kind>(step === "current" ? (v?.kind ?? "pin") : kind);
  const pad = $derived(phone && curKind === "pin");
  const title = $derived(flow === "off" ? "Turn off app lock" : flow === "change" ? `Change ${word(v?.kind ?? "pin")}` : "Set up app lock");
  const prompt = $derived(
    step === "current" ? `Enter your current ${word(v?.kind ?? "pin")}` :
    step === "new" ? (curKind === "pin" ? "Choose a PIN (4–12 digits)" : "Choose a passphrase (6+ characters)") :
    `Repeat the ${word(curKind)}`);

  function valid(k: Kind, s: string): string {
    if (k === "pin") return /^\d{4,12}$/.test(s) ? "" : "Use 4 to 12 digits.";
    return s.length >= 6 ? "" : "Use at least 6 characters.";
  }

  async function next(value = entry) {
    if (busy) return;
    err = "";
    if (step === "current") {
      if (!value) return;
      if (flow === "off") {
        busy = true;
        try {
          const r = await lockApi.disable(value);
          lockState.view = r.lock;
          if (r.result === "ok") { close(); flash = "App lock is off."; } else bad(wrongText(r, r.lock.kind));
        } catch (e) { bad(String(e)); }
        busy = false;
        return;
      }
      current = value; entry = ""; step = "new";
      return;
    }
    if (step === "new") {
      const why = valid(kind, value);
      if (why) return bad(why);
      first = value; entry = ""; step = "confirm";
      return;
    }
    if (value !== first) { first = ""; step = "new"; return bad(`The two ${kind === "pin" ? "PINs" : "passphrases"} differ. Try again.`); }
    busy = true;
    try {
      if (flow === "set") {
        lockState.view = await lockApi.enable(kind, value);
        close(); flash = `App lock is on. ${appName} asks for it when you come back.`;
      } else {
        const r = await lockApi.change(current, kind, value);
        lockState.view = r.lock;
        if (r.result === "ok") { close(); flash = `${word(kind) === "PIN" ? "PIN" : "Passphrase"} changed.`; }
        else { current = first = ""; step = "current"; bad(wrongText(r, r.lock.kind)); }
      }
    } catch (e) { bad(String(e)); }
    busy = false;
  }

  async function opts(patch: Partial<Pick<LockView, "timeout_secs" | "biometric" | "hide_in_switcher">>) {
    if (!v) return;
    const n = { timeout_secs: v.timeout_secs, biometric: v.biometric, hide_in_switcher: v.hide_in_switcher, ...patch };
    try { lockState.view = await lockApi.setOptions(n.timeout_secs, n.biometric, n.hide_in_switcher); } catch (e) { flash = String(e); }
  }
  const bioOk = $derived(bio === "fingerprint" || bio === "credential");
</script>

{#snippet toggle(on: boolean, lab: string, fn: () => void, disabled = false)}
  <button type="button" class="l-tg" class:l-on={on} role="switch" aria-checked={on} aria-label={lab} {disabled} onclick={fn}><span></span></button>
{/snippet}

<div class="skl l" class:l-phone={phone}>
  {#if heading}<h3>App lock</h3>{/if}
  {#if v}
    <div class="l-card">
      <div class="l-row">
        <span class="l-ic" aria-hidden="true"><svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><rect x="4.5" y="10.5" width="15" height="10" rx="2.2"/><path d="M8 10.5V7.5a4 4 0 0 1 8 0v3"/></svg></span>
        <div class="l-grow"><div class="l-t">App lock</div><div class="l-s">{v.enabled ? `On · ${word(v.kind)}` : `Ask for a PIN when you come back to ${appName}`}</div></div>
        {@render toggle(v.enabled, "App lock", () => open(v!.enabled ? "off" : "set"))}
      </div>
      {#if v.enabled}
        <div class="l-row l-col">
          <div class="l-t">Lock after</div>
          <div class="l-seg" role="radiogroup" aria-label="Lock after">
            {#each TIMEOUT_LABELS as [secs, lab]}
              <button type="button" role="radio" aria-checked={v.timeout_secs === secs} class:l-on={v.timeout_secs === secs} onclick={() => opts({ timeout_secs: secs })}>{lab}</button>
            {/each}
          </div>
          <div class="l-s">{android ? "Time away from the app." : "Time the window is in the background or untouched (idle: at least 1 min)."} It always locks when {appName} starts.</div>
        </div>
        {#if android}
          <div class="l-row">
            <div class="l-grow"><div class="l-t">Unlock with fingerprint</div>
              <div class="l-s">{bio === "fingerprint" ? "Or the phone's screen lock" : bio === "credential" ? "No fingerprint set up: uses the phone's screen lock" : "Set up a fingerprint or screen lock in the phone's settings first"}</div></div>
            {@render toggle(v.biometric && bioOk, "Unlock with fingerprint", () => opts({ biometric: !v!.biometric }), !bioOk)}
          </div>
        {/if}
        <button type="button" class="l-row l-link" onclick={() => open("change")}><span class="l-grow">Change {word(v.kind)}</span>
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9 6l6 6-6 6"/></svg></button>
      {/if}
      {#if android}
        <div class="l-row">
          <div class="l-grow"><div class="l-t">Hide content in app switcher</div><div class="l-s">Blank preview in recent apps. Also blocks screenshots.</div></div>
          {@render toggle(v.hide_in_switcher, "Hide content in app switcher", () => opts({ hide_in_switcher: !v!.hide_in_switcher }))}
        </div>
      {/if}
    </div>
    {#if flash}<p class="l-hint l-flash">{flash}</p>{/if}
    <p class="l-hint">The lock covers the screen; it doesn't encrypt the data stored on this device. 4 wrong tries are free, then the waits grow (30 s up to 1 h). Nothing is ever deleted.</p>
    {#if v.test_build}<p class="l-hint">Test build: the lock screen opens by itself after 5 seconds unless you type.</p>{/if}
  {:else}
    <div class="l-card"><div class="l-row"><div class="l-s">Loading…</div></div></div>
  {/if}

  {#if flow && v}
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <div class="l-scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && close()}>
      <div class="l-dlg" role="dialog" aria-modal="true" aria-label={title}>
        <div class="l-dh"><h4>{title}</h4>
          <button type="button" class="l-x" aria-label="Close" onclick={close}><svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18"/></svg></button></div>
        <p class="l-prompt">{prompt}</p>
        {#if pad}
          {#key step}
            <div class="l-pad"><PinPad bind:value={entry} len={step === "current" ? v.pin_len : step === "confirm" ? first.length : 0} disabled={busy} {shake}
              onfull={(p) => next(p)} onkey={() => (err = "")} /></div>
          {/key}
          {#if err}<p class="l-msg">{err}</p>{/if}
          {#if step === "new" || (step === "current" && !v.pin_len)}
            <div class="l-acts"><button type="button" class="l-btn l-pri" disabled={entry.length < 4 || busy} onclick={() => next()}>{step === "current" ? "Continue" : "Next"}</button></div>
          {/if}
        {:else}
          {#key step}
            <!-- svelte-ignore a11y_autofocus -->
            <input class="l-in" type="password" inputmode={curKind === "pin" ? "numeric" : "text"} bind:value={entry} autofocus autocomplete="new-password" autocapitalize="off" spellcheck="false"
              aria-label={prompt} onkeydown={(e) => e.key === "Enter" && next()} />
          {/key}
          {#if err}<p class="l-msg">{err}</p>{/if}
          <div class="l-acts"><button type="button" class="l-btn l-pri" disabled={!entry || busy} onclick={() => next()}>{step === "confirm" ? (flow === "change" ? "Change" : "Turn on") : flow === "off" ? "Turn off" : "Next"}</button></div>
        {/if}
        {#if step === "new"}
          <button type="button" class="l-sw" onclick={() => { kind = kind === "pin" ? "passphrase" : "pin"; entry = ""; err = ""; }}>{kind === "pin" ? "Use a passphrase instead" : "Use a PIN instead"}</button>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .skl {
    --lk-a: var(--lock-accent, var(--sync-accent, var(--accent, #3e6bb3)));
    --lk-on-a: var(--lock-on-accent, var(--on-accent, #fff));
    --lk-a-soft: var(--lock-accent-soft, color-mix(in srgb, var(--lk-a) 14%, transparent));
    --lk-surf: var(--lock-surface, var(--card, var(--surface, #fff)));
    --lk-line: var(--lock-line, var(--sep, var(--line, rgba(127, 127, 127, 0.22))));
    --lk-ink: var(--lock-ink, var(--ink, #111));
    --lk-mut: var(--lock-muted, var(--muted, #888));
    --lk-bad: var(--lock-danger, var(--danger, #c2412d));
    --lk-key: var(--lock-key, color-mix(in srgb, var(--lk-ink) 7%, transparent));
    --lk-key-down: var(--lock-key-down, color-mix(in srgb, var(--lk-ink) 14%, transparent));
    --lk-r: var(--lock-radius, 16px);
    color: var(--lk-ink); font-size: 14px; line-height: 1.4;
  }
  h3 { font-size: 12px; font-weight: 600; color: var(--lk-mut); letter-spacing: 0.04em; text-transform: uppercase; margin: 0 4px 8px; }
  .l-card { background: var(--lk-surf); border: 1px solid var(--lk-line); border-radius: var(--lk-r); }
  .l-row { display: flex; align-items: center; gap: 12px; padding: 12px 16px; min-height: 52px; width: 100%; box-sizing: border-box; text-align: left; color: inherit; background: none; border: 0; font: inherit; }
  .l-card > .l-row + .l-row { border-top: 1px solid var(--lk-line); }
  .l-row.l-col { flex-direction: column; align-items: stretch; gap: 8px; }
  .l-link { color: var(--lk-a); font-weight: 600; cursor: pointer; }
  .l-link svg { color: var(--lk-mut); }
  .l-ic { color: var(--lk-mut); display: flex; }
  .l-grow { flex: 1; min-width: 0; }
  .l-t { font-weight: 550; }
  .l-s { color: var(--lk-mut); font-size: 12.5px; margin-top: 1px; }
  .l-hint { color: var(--lk-mut); font-size: 12.5px; margin: 10px 4px 0; }
  .l-flash { color: var(--lk-ink); }
  .l-tg { width: 46px; height: 44px; flex: none; display: grid; place-items: center; background: none; border: 0; cursor: pointer; padding: 0; }
  .l-tg:disabled { opacity: 0.4; cursor: default; }
  .l-tg span { width: 42px; height: 26px; border-radius: 13px; background: var(--lk-line); position: relative; transition: background 0.15s; }
  .l-tg span::after { content: ""; position: absolute; top: 3px; left: 3px; width: 20px; height: 20px; border-radius: 10px; background: #fff; box-shadow: 0 1px 2px rgba(0, 0, 0, 0.2); transition: transform 0.15s; }
  .l-tg.l-on span { background: var(--lk-a); }
  .l-tg.l-on span::after { transform: translateX(16px); }
  .l-seg { display: flex; background: var(--lk-key); border-radius: 12px; padding: 3px; gap: 3px; }
  .l-seg button { flex: 1; min-height: 36px; border-radius: 9px; font: inherit; font-weight: 550; color: var(--lk-mut); background: none; border: 0; cursor: pointer; font-size: 13px; white-space: nowrap; padding: 0 4px; }
  .l-seg button.l-on { background: var(--lk-surf); color: var(--lk-ink); box-shadow: 0 1px 3px rgba(0, 0, 0, 0.12); }
  .l-scrim { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.4); display: grid; place-items: center; z-index: 60; padding: 16px; }
  .l-dlg { width: min(440px, 100%); max-height: calc(100% - 32px); overflow: auto; background: var(--lk-surf); border: 1px solid var(--lk-line); border-radius: 18px; padding: 20px; box-sizing: border-box; }
  .l-phone .l-scrim { place-items: end stretch; padding: 0; }
  .l-phone .l-dlg { width: 100%; border-radius: 22px 22px 0 0; max-height: 96%; padding: 18px 16px max(24px, env(safe-area-inset-bottom)); }
  .l-dh { display: flex; align-items: center; }
  .l-dh h4 { flex: 1; margin: 0; font-size: 18px; font-weight: 650; }
  .l-x { width: 44px; height: 44px; display: grid; place-items: center; background: none; border: 0; color: var(--lk-mut); cursor: pointer; margin-right: -10px; }
  .l-prompt { margin: 4px 0 16px; color: var(--lk-mut); }
  .l-phone .l-prompt { text-align: center; margin: 8px 0 22px; color: var(--lk-ink); font-weight: 550; }
  .l-pad { display: flex; justify-content: center; }
  .l-in { display: block; width: 100%; box-sizing: border-box; height: 46px; border: 1px solid var(--lk-line); border-radius: 12px; background: var(--lk-surf); padding: 0 14px; color: var(--lk-ink); font: inherit; font-size: 16px; letter-spacing: 0.06em; }
  .l-in:focus { outline: 2px solid var(--lk-a); outline-offset: -1px; border-color: transparent; }
  .l-msg { margin: 12px 2px 0; font-size: 13px; color: var(--lk-bad); text-align: center; }
  .l-acts { display: flex; justify-content: flex-end; margin-top: 18px; }
  .l-phone .l-acts .l-btn { flex: 1; }
  .l-btn { height: 44px; padding: 0 18px; border-radius: 12px; font: inherit; font-weight: 600; border: 1px solid var(--lk-line); background: var(--lk-surf); color: var(--lk-ink); cursor: pointer; }
  .l-btn.l-pri { background: var(--lk-a); color: var(--lk-on-a); border-color: transparent; }
  .l-btn:disabled { opacity: 0.5; cursor: default; }
  .l-sw { display: block; margin: 12px auto 0; background: none; border: 0; color: var(--lk-a); font: inherit; font-weight: 600; padding: 10px 14px; cursor: pointer; }
</style>
