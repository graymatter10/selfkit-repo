<script lang="ts">
  // App lock gate (docs/components/app-lock.md). Put it once at the root of the app's main
  // window: it covers everything while locked, and tells Rust when the app was away
  // (Android: SelfkitLock.kt measures stop -> start; desktop: unfocused/hidden or idle).
  // Neutral; the app maps its tokens onto --lock-* (docs/components/app-lock.md, Theming).
  import { onDestroy, onMount } from "svelte";
  import PinPad from "./PinPad.svelte";
  import { biometricPrompt, lockApi, lockBridge, lockState, waitText, word, wrongText, type LockView } from "./api.svelte";

  let { appName, phone = false }: { appName: string; phone?: boolean } = $props();

  const TEST_SECS = 5;
  const v = $derived(lockState.view);
  const locked = $derived(!!v?.enabled && !!v?.locked);
  let pin = $state("");
  let msg = $state("");
  let shake = $state(false);
  let busy = $state(false);
  let waitUntil = $state(0);
  let now = $state(Date.now());
  let bio = $state<"fingerprint" | "credential" | "none" | "unavailable">("unavailable");
  let testLeft = $state(0);
  let forgot = $state(false);
  let useRecovery = $state(false);
  let resetSecret = $state("");
  let resetErr = $state("");
  let input = $state<HTMLInputElement | null>(null);
  let root = $state<HTMLDivElement | null>(null);

  const android = $derived(v?.platform === "android");
  const isPin = $derived(v?.kind !== "passphrase");
  const usePad = $derived(phone && isPin);
  const waitMs = $derived(Math.max(0, waitUntil - now));
  const bioKey = $derived(android && v?.biometric && (bio === "fingerprint" || bio === "credential") ? bio : null);

  function set(next: LockView) {
    const was = locked;
    lockState.view = next;
    if (next.retry_in_ms > 0) waitUntil = Date.now() + next.retry_in_ms;
    if (!was && next.enabled && next.locked) onLocked();
    if (was && !(next.enabled && next.locked)) onUnlocked();
  }
  // Lock commands run in parallel (async, C13), so replies can come back out of order:
  // apply only replies at least as new as the last applied one (C91).
  let reqSeq = 0, appliedSeq = 0;
  async function latest<T>(p: Promise<T>, use: (v: T) => void) {
    const n = ++reqSeq;
    const v = await p;
    if (n < appliedSeq) return;
    appliedSeq = n;
    use(v);
  }
  async function refresh() {
    try { await latest(lockApi.status(), set); } catch { /* commands not ready yet: ipc retries */ }
  }

  function onLocked() {
    pin = ""; msg = ""; forgot = false; resetSecret = ""; resetErr = "";
    bio = lockBridge()?.biometricStatus() ?? "unavailable";
    if (lockState.view?.test_build) {
      testLeft = TEST_SECS;
    } else if (android && lockState.view?.biometric && bio === "fingerprint") {
      setTimeout(useBio, 250);
    }
    setTimeout(() => input?.focus(), 50);
  }
  function onUnlocked() {
    pin = ""; msg = ""; testLeft = 0; forgot = false;
  }
  function stopTest() { testLeft = 0; }

  async function submit(value = pin) {
    if (!value || busy || waitMs > 0) return;
    busy = true;
    stopTest();
    try {
      const n = ++reqSeq;
      const r = await lockApi.unlock(value);
      const fresh = n >= appliedSeq;
      if (fresh) appliedSeq = n;
      pin = "";
      if (r.result === "ok") {
        if (fresh) set(r.lock);
      } else {
        // A wait is shown by the live countdown line, then "Enter your PIN" again; a fixed
        // "Try again in 30 s." in msg stayed on screen after the wait ended (B5, L7).
        msg = "retry_in_ms" in r && r.retry_in_ms > 0 ? "" : wrongText(r, r.lock.kind);
        shake = true;
        setTimeout(() => (shake = false), 420);
        if (fresh) set(r.lock);
      }
    } catch (e) {
      msg = String(e);
    }
    busy = false;
    setTimeout(() => input?.focus(), 30);
  }

  async function useBio() {
    stopTest();
    const r = await biometricPrompt(`Unlock ${appName}`);
    if (r === "ok") await latest(lockApi.unlockBiometric(), set);
    else if (r.startsWith("error:")) msg = r.slice(6);
  }

  async function doReset() {
    resetErr = "";
    try {
      set(await lockApi.reset(resetSecret, useRecovery));
      resetSecret = "";
    } catch (e) {
      resetErr = String(e);
    }
  }

  // ---- time away
  let awaySince = 0;
  let lastInput = Date.now();
  async function away(ms: number) {
    if (!lockState.view?.enabled) return;
    try { await latest(lockApi.away(ms), set); } catch { /* ignore */ }
  }
  const onResume = () => {
    const ms = lockBridge()?.takeAway() ?? -1;
    if (ms >= 0) away(ms);
  };
  const goAway = () => { if (!awaySince) awaySince = Date.now(); };
  const comeBack = () => {
    if (!awaySince) return;
    const ms = Date.now() - awaySince;
    awaySince = 0;
    lastInput = Date.now();
    away(ms);
  };
  const onVis = () => (document.visibilityState === "hidden" ? goAway() : document.hasFocus() && comeBack());
  const touch = () => { lastInput = Date.now(); };

  let tick: ReturnType<typeof setInterval>;
  onMount(() => {
    refresh().then(onResume);
    if (lockBridge()) {
      window.addEventListener("selfkit-lock-resume", onResume);
    } else {
      window.addEventListener("blur", goAway);
      window.addEventListener("focus", comeBack);
      document.addEventListener("visibilitychange", onVis);
      for (const e of ["pointerdown", "pointermove", "keydown", "wheel", "touchstart"]) window.addEventListener(e, touch, { passive: true });
    }
    tick = setInterval(async () => {
      now = Date.now();
      const s = lockState.view;
      if (testLeft > 0 && locked) {
        testLeft = Math.max(0, testLeft - 0.25);
        if (testLeft === 0) { try { await latest(lockApi.testUnlock(), set); } catch { /* not a test build */ } }
      }
      // Desktop idle: no input for the chosen time (at least 1 minute) while open.
      if (!lockBridge() && s?.enabled && !s.locked && !awaySince && now - lastInput >= Math.max(s.timeout_secs, 60) * 1000) {
        try { await latest(lockApi.now(), set); } catch { /* ignore */ }
      }
    }, 250);
  });
  onDestroy(() => {
    clearInterval(tick);
    window.removeEventListener("selfkit-lock-resume", onResume);
    window.removeEventListener("blur", goAway);
    window.removeEventListener("focus", comeBack);
    document.removeEventListener("visibilitychange", onVis);
    for (const e of ["pointerdown", "pointermove", "keydown", "wheel", "touchstart"]) window.removeEventListener(e, touch);
    setInert(false);
  });

  // "Hide content in app switcher" (Android FLAG_SECURE) follows the stored setting.
  $effect(() => {
    const s = lockState.view;
    if (s) lockBridge()?.setSecure(s.hide_in_switcher);
  });

  // While locked, nothing behind the lock screen can be focused or clicked.
  function setInert(on: boolean) {
    for (const el of Array.from(document.body.children)) {
      if (el === root) continue;
      if (on) el.setAttribute("inert", ""); else el.removeAttribute("inert");
    }
  }
  $effect(() => {
    setInert(locked);
    if (!locked) return;
    // Toasts, portaled sheets or the conflict bar added while locked are inert too (C17).
    const mo = new MutationObserver(() => setInert(true));
    mo.observe(document.body, { childList: true });
    return () => mo.disconnect();
  });

  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy() { node.remove(); } };
  }

  function onKey(e: KeyboardEvent) {
    stopTest();
    if (e.key === "Enter") submit();
  }
  function onInput() {
    stopTest();
    if (isPin) pin = pin.replace(/\D/g, "");
    if (isPin && v?.pin_len && pin.length === v.pin_len) submit();
  }
</script>

{#if locked && v}
  <div class="skl lk" class:lk-phone={phone} bind:this={root} use:portal role="dialog" aria-modal="true" aria-label={`${appName} is locked`}>
    <div class="lk-col">
      <div class="lk-top">
        <div class="lk-ic" aria-hidden="true">
          <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="4.5" y="10.5" width="15" height="10" rx="2.6"/><path d="M8 10.5V7.5a4 4 0 0 1 8 0v3"/></svg>
        </div>
        <h1>{appName}</h1>
        {#if !forgot}
          <p class="lk-sub" class:lk-bad={!!msg || waitMs > 0} aria-live="polite">
            {#if waitMs > 0}Too many tries. Try again in {waitText(waitMs)}.
            {:else if msg}{msg}
            {:else}Enter your {word(v.kind)}{/if}
          </p>
        {/if}
      </div>

      {#if forgot}
        <div class="lk-forgot">
          {#if v.can_reset}
            <p class="lk-hint">Turn the lock off with your Selfkit encryption passphrase or recovery key. Then set a new {word(v.kind)} in Settings.</p>
            <div class="lk-seg"><button type="button" class:lk-on={!useRecovery} onclick={() => (useRecovery = false)}>Passphrase</button><button type="button" class:lk-on={useRecovery} onclick={() => (useRecovery = true)}>Recovery key</button></div>
            <input class="lk-in" type={useRecovery ? "text" : "password"} bind:value={resetSecret} placeholder={useRecovery ? "XXXX-XXXX-…" : "Encryption passphrase"} autocapitalize="off" spellcheck="false" autocomplete="off" onkeydown={(e) => e.key === "Enter" && doReset()} />
            {#if resetErr}<p class="lk-sub lk-bad">{resetErr}</p>{/if}
            <button type="button" class="lk-btn lk-pri" disabled={!resetSecret} onclick={doReset}>Turn off the lock</button>
          {:else}
            <p class="lk-hint">{#if android}Without sync encryption set up there is no reset code. The way back is Android Settings → Apps → {appName} → Storage → Clear data, which also deletes anything not synced yet.{:else}Without sync encryption set up there is no reset code. Close {appName} and delete <code>{v.lock_file}</code>.{/if}</p>
          {/if}
          <button type="button" class="lk-link" onclick={() => { forgot = false; resetErr = ""; }}>Back</button>
        </div>
      {:else if usePad}
        <PinPad bind:value={pin} len={v.pin_len} disabled={busy || waitMs > 0} {shake} bio={bioKey} onfull={(p) => submit(p)} onkey={stopTest} onbio={useBio} />
        {#if !v.pin_len}<button type="button" class="lk-btn lk-pri lk-ok" disabled={pin.length < 4 || busy || waitMs > 0} onclick={() => submit()}>Unlock</button>{/if}
      {:else}
        <div class="lk-desk" class:lk-shake={shake}>
          <input class="lk-in" bind:this={input} type="password" inputmode={isPin ? "numeric" : "text"} bind:value={pin} disabled={busy || waitMs > 0}
            placeholder={isPin ? "PIN" : "Passphrase"} aria-label={isPin ? "PIN" : "Passphrase"} autocomplete="off" autocapitalize="off" spellcheck="false" onkeydown={onKey} oninput={onInput} />
          <button type="button" class="lk-btn lk-pri" disabled={!pin || busy || waitMs > 0} onclick={() => submit()}>Unlock</button>
        </div>
        {#if bioKey}<button type="button" class="lk-btn" onclick={useBio}>{bioKey === "fingerprint" ? "Use fingerprint" : "Use phone screen lock"}</button>{/if}
      {/if}

      <div class="lk-foot">
        {#if v.test_build && testLeft > 0}
          <p class="lk-test">Test build · opens by itself in {Math.ceil(testLeft)} s. Tap a key to stay locked.</p>
        {/if}
        {#if !forgot}<button type="button" class="lk-link" onclick={() => { stopTest(); forgot = true; }}>Forgot {word(v.kind)}?</button>{/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .skl {
    --lk-a: var(--lock-accent, var(--accent, #3e6bb3));
    --lk-a-soft: var(--lock-accent-soft, color-mix(in srgb, var(--lk-a) 14%, transparent));
    --lk-on-a: var(--lock-on-accent, var(--on-accent, #fff));
    --lk-bg: var(--lock-bg, var(--bg, var(--page, #f4f4f4)));
    --lk-surf: var(--lock-surface, var(--card, var(--surface, #fff)));
    --lk-line: var(--lock-line, var(--sep, var(--line, rgba(127, 127, 127, 0.22))));
    --lk-ink: var(--lock-ink, var(--ink, #111));
    --lk-mut: var(--lock-muted, var(--muted, #888));
    --lk-bad: var(--lock-danger, var(--danger, #c2412d));
    --lk-key: var(--lock-key, color-mix(in srgb, var(--lk-ink) 7%, transparent));
    --lk-key-down: var(--lock-key-down, color-mix(in srgb, var(--lk-ink) 14%, transparent));
  }
  .lk { position: fixed; inset: 0; z-index: 2147483000; background: var(--lk-bg); color: var(--lk-ink); display: flex; justify-content: center; overflow-y: auto;
    padding: max(24px, env(safe-area-inset-top)) 16px max(20px, env(safe-area-inset-bottom)); box-sizing: border-box; font-size: 14px; line-height: 1.4; }
  .lk-col { width: min(360px, 100%); display: flex; flex-direction: column; align-items: center; justify-content: center; min-height: 100%; gap: 0; }
  .lk-phone .lk-col { justify-content: flex-end; }
  .lk-top { display: flex; flex-direction: column; align-items: center; text-align: center; margin-bottom: 28px; }
  .lk-phone .lk-top { margin-top: auto; margin-bottom: 36px; }
  .lk-ic { width: 52px; height: 52px; border-radius: 17px; background: var(--lk-a-soft); color: var(--lk-a); display: grid; place-items: center; margin-bottom: 16px; }
  h1 { font-size: 21px; font-weight: 650; letter-spacing: -0.015em; margin: 0; }
  .lk-sub { margin: 6px 0 0; color: var(--lk-mut); font-size: 14px; min-height: 20px; }
  .lk-sub.lk-bad { color: var(--lk-bad); }
  .lk-desk { display: flex; gap: 8px; width: 100%; }
  .lk-shake { animation: lk-shake 0.38s ease-in-out; }
  @keyframes lk-shake { 0%, 100% { transform: translateX(0); } 25% { transform: translateX(-8px); } 50% { transform: translateX(7px); } 75% { transform: translateX(-4px); } }
  .lk-in { flex: 1; min-width: 0; width: 100%; height: 46px; box-sizing: border-box; border: 1px solid var(--lk-line); border-radius: 13px; background: var(--lk-surf); color: var(--lk-ink); padding: 0 14px; font: inherit; font-size: 16px; letter-spacing: 0.08em; }
  .lk-in:focus { outline: 2px solid var(--lk-a); outline-offset: -1px; border-color: transparent; }
  .lk-btn { height: 46px; padding: 0 18px; border-radius: 13px; border: 1px solid var(--lk-line); background: var(--lk-surf); color: var(--lk-ink); font: inherit; font-weight: 600; cursor: pointer; white-space: nowrap; margin-top: 10px; }
  .lk-desk .lk-btn { margin-top: 0; }
  .lk-btn.lk-pri { background: var(--lk-a); color: var(--lk-on-a); border-color: transparent; }
  .lk-btn:disabled { opacity: 0.5; cursor: default; }
  .lk-ok { width: min(300px, 100%); margin-top: 18px; }
  .lk-foot { display: flex; flex-direction: column; align-items: center; margin-top: 22px; min-height: 64px; }
  .lk-phone .lk-foot { margin-top: 14px; }
  .lk-link { background: none; border: 0; color: var(--lk-a); font: inherit; font-weight: 600; padding: 12px 16px; cursor: pointer; }
  .lk-test { margin: 0 0 2px; font-size: 12.5px; color: var(--lk-mut); text-align: center; }
  .lk-forgot { width: 100%; display: flex; flex-direction: column; align-items: stretch; gap: 10px; }
  .lk-forgot .lk-btn { margin-top: 0; }
  .lk-forgot .lk-link { align-self: center; }
  .lk-hint { margin: 0 0 4px; color: var(--lk-mut); font-size: 13.5px; text-align: center; }
  .lk-hint code { font-size: 12.5px; }
  .lk-seg { display: flex; background: var(--lk-key); border-radius: 12px; padding: 3px; gap: 3px; }
  .lk-seg button { flex: 1; min-height: 38px; border-radius: 9px; border: 0; background: none; color: var(--lk-mut); font: inherit; font-weight: 550; font-size: 13.5px; cursor: pointer; }
  .lk-seg button.lk-on { background: var(--lk-surf); color: var(--lk-ink); box-shadow: 0 1px 3px rgba(0, 0, 0, 0.12); }
</style>
