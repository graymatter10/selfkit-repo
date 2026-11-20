<script lang="ts">
  // Adjust times (or add a nap). Times are picked on a clock dial (TimeSheet):
  // phone: tap the time to open it in a bottom sheet; desktop: side panel where
  // the field can also be typed (↑/↓ ±15 min, Alt+↓ or the clock button opens
  // the dial, Tab next field, Enter/Ctrl+Enter save, Esc close).
  // Leaving with changes asks first ("Back never loses input").
  import { onMount } from "svelte";
  import type { Edit } from "../lib/state.svelte";
  import { addNap, app, nightByKey, resetToEstimate, setLog } from "../lib/state.svelte";
  import { dayLong, dur, hm, nightRange, normTime, shiftTime, tdisp, toMin, uses12h } from "../lib/fmt";
  import Confirm from "./Confirm.svelte";
  import Icon from "./Icon.svelte";
  import TimeSheet from "./TimeSheet.svelte";

  let { edit, desktop, onclose }: { edit: Edit; desktop: boolean; onclose: () => void } = $props();
  const nap = edit.kind === "nap";
  const night = $derived(nightByKey(edit.night));
  const n0 = nightByKey(edit.night);
  const tz = n0?.tz_offset ?? -new Date().getTimezoneOffset();
  const init = {
    a: nap ? "" : n0?.bed != null ? hm(n0.bed, tz) : "",
    b: nap ? "" : n0?.wake != null ? hm(n0.wake, tz) : "",
    before: n0?.log?.note_before ?? "",
    after: n0?.log?.note_after ?? "",
  };
  const est = { a: !nap && n0?.estimate.bed != null ? hm(n0.estimate.bed, tz) : "", b: !nap && n0?.estimate.wake != null ? hm(n0.estimate.wake, tz) : "" };
  const LABEL = nap ? { a: "Fell asleep", b: "Woke up" } : { a: "Bedtime", b: "Woke up" };
  const DEF = nap ? { a: "14:00", b: "14:30" } : { a: "23:00", b: "07:00" };
  const typeHint = uses12h() ? "11:30pm or 2330" : "2330 or 23:30";

  // a / b hold what the field shows ("23:30" or "11:30 pm"); val() normalises to "HH:MM".
  let a = $state(tdisp(init.a)), b = $state(tdisp(init.b));
  let before = $state(init.before), after = $state(init.after);
  let focused: "a" | "b" | null = $state(null);
  let dial: "a" | "b" | null = $state(null);
  let error = $state("");
  let saving = $state(false);
  let confirm = $state(false);
  let elA: HTMLInputElement | undefined = $state(), elB: HTMLInputElement | undefined = $state();
  let elBefore: HTMLTextAreaElement | undefined = $state(), elAfter: HTMLTextAreaElement | undefined = $state();

  const val = (s: string) => (s.trim() ? normTime(s) : "");
  const invalid = (s: string) => !!s.trim() && !normTime(s);
  const dirty = $derived(val(a) !== init.a || val(b) !== init.b || before.trim() !== init.before || after.trim() !== init.after || (nap && !!(a || b)));
  const asleep = $derived.by(() => {
    const x = val(a), y = val(b);
    if (!x || !y) return null;
    if (nap) return (toMin(y) - toMin(x) + 1440) % 1440;
    const bm = toMin(x) >= 18 * 60 ? toMin(x) - 1440 : toMin(x);
    const wm = toMin(y) >= 18 * 60 ? toMin(y) - 1440 : toMin(y);
    return wm - bm;
  });
  // Any typed time can be dropped again (also when the pair shows "No data").
  const canReset = $derived(!nap && (night?.log?.bed != null || night?.log?.wake != null) && night?.estimate.status === "found");

  function guard() {
    if (confirm) return (confirm = false), true;
    if (dirty && !saving) return (confirm = true), true;
    return false;
  }
  onMount(() => {
    app.closeGuard = guard;
    const f = edit.focus;
    setTimeout(() => {
      if (app.demo.dial === "a" || app.demo.dial === "b") dial = app.demo.dial;
      else if (f === "note_before") elBefore?.focus();
      else if (f === "note_after") elAfter?.focus();
      else if (desktop) (f === "wake" ? elB : elA)?.focus();
      // phone: go straight to the dial when there is nothing to adjust yet
      else if (nap || f === "bed" || f === "wake" || (!init.a && !est.a)) dial = f === "wake" ? "b" : "a";
    }, 60);
    return () => {
      if (app.closeGuard === guard) app.closeGuard = null;
    };
  });
  function close() {
    if (!guard()) onclose();
  }

  const current = (w: "a" | "b") =>
    val(w === "a" ? a : b) || (w === "a" ? est.a || DEF.a : est.b || (nap && val(a) ? shiftTime(val(a)!, 30) : DEF.b));
  function setField(w: "a" | "b", t: string) {
    if (w === "a") a = tdisp(t);
    else b = tdisp(t);
    error = "";
  }
  function nudge(w: "a" | "b", by: number) {
    setField(w, shiftTime(current(w), by));
  }
  // Same rule as log_set: the effective pair (typed time, else the estimate) must have wake after bed.
  function pairError(): string {
    // A nap end before its start wrapped to "Nap 23h 45m" (P25): naps over midnight are
    // fine, but one longer than the 6 h limit means the times are the wrong way round.
    if (nap) return asleep != null && asleep > 360 ? `Woke up must be after you fell asleep (${tdisp(val(a))} → ${tdisp(val(b))}).` : "";
    const x = val(a) || est.a, y = val(b) || est.b;
    if (!x || !y) return "";
    const bm = toMin(x) >= 18 * 60 ? toMin(x) - 1440 : toMin(x);
    const wm = toMin(y) >= 18 * 60 ? toMin(y) - 1440 : toMin(y);
    return wm <= bm ? `Wake time must be after bedtime (${tdisp(x)} → ${tdisp(y)}).` : "";
  }
  function dialDone(t: string) {
    const w = dial!;
    setField(w, t);
    dial = null;
    error = pairError();
    // first time through (nap, or no times yet): go on to the wake time
    if (!desktop && w === "a" && !val(b)) setTimeout(() => (dial = "b"), 220);
  }
  function onFocus(w: "a" | "b", e: FocusEvent) {
    focused = w;
    (e.target as HTMLInputElement).select();
  }
  function onBlur(w: "a" | "b") {
    const t = normTime(w === "a" ? a : b);
    if (t) setField(w, t);
    setTimeout(() => {
      if (document.activeElement !== elA && document.activeElement !== elB) focused = null;
    }, 0);
  }
  function fieldKeys(w: "a" | "b", e: KeyboardEvent) {
    if (e.key === "ArrowDown" && e.altKey) {
      e.preventDefault();
      dial = w;
    } else if (e.key === "ArrowUp" || e.key === "ArrowDown") {
      e.preventDefault();
      nudge(w, e.key === "ArrowUp" ? 15 : -15);
    } else if (e.key === "Enter") {
      e.preventDefault();
      save();
    }
  }
  function panelKeys(e: KeyboardEvent) {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      save();
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      close();
    }
  }

  async function save() {
    if (saving) return;
    if (invalid(a) || invalid(b)) return void (error = `Check the times: pick them on the clock or type ${typeHint}.`);
    if ((val(a) !== init.a || val(b) !== init.b) && pairError()) return void (error = pairError());
    saving = true;
    error = "";
    try {
      if (nap) {
        if (!val(a) || !val(b)) throw new Error("Enter when the nap started and ended.");
        await addNap(edit.night, val(a)!, val(b)!);
      } else {
        const patch: Record<string, string | null> = {};
        if (val(a) !== init.a) patch.bed = val(a) || null;
        if (val(b) !== init.b) patch.wake = val(b) || null;
        if (before.trim() !== init.before) patch.note_before = before.trim() || null;
        if (after.trim() !== init.after) patch.note_after = after.trim() || null;
        if (Object.keys(patch).length) {
          const what = "bed" in patch || "wake" in patch ? "Times saved" : "Note saved";
          await setLog(edit.night, patch, what);
        }
      }
      onclose();
    } catch (e) {
      error = String(e).replace(/^Error: /, "");
      saving = false;
    }
  }
  async function useEstimate() {
    saving = true;
    await resetToEstimate(edit.night);
    onclose();
  }
</script>

{#snippet timeRow(w: "a" | "b")}
  {@const v = w === "a" ? a : b}
  {@const bad = invalid(v)}
  <div class="flex items-center gap-2 py-3">
    <div class="flex min-w-0 flex-1 flex-col gap-1">
      <label for="tf-{w}" class="text-[16px] font-[550]">{LABEL[w]}</label>
      {#if est[w]}<span class="num text-[12.5px] whitespace-nowrap text-muted">Estimate {tdisp(est[w])}</span>{/if}
    </div>
    <button type="button" tabindex="-1" class="press grid h-12 shrink-0 place-items-center rounded-[12px] bg-fill text-[14.5px] font-semibold text-ink2 {desktop ? 'w-11' : 'w-12'}" aria-label="{LABEL[w]} 15 minutes earlier" onmousedown={(e) => e.preventDefault()} onclick={() => nudge(w, -15)}>−15</button>
    {#if desktop}
      <div class="relative shrink-0">
        {#if w === "a"}
          <input id="tf-a" bind:this={elA} bind:value={a} autocomplete="off" placeholder={tdisp(est.a) || "hh:mm"} aria-invalid={bad} aria-describedby="tf-help"
            onfocus={(e) => onFocus("a", e)} onblur={() => onBlur("a")} onkeydown={(e) => fieldKeys("a", e)} oninput={() => (error = "")}
            class="num h-12 rounded-[12px] pr-7 pl-1.5 text-center font-semibold outline-none {uses12h() ? 'w-[118px] text-[16px]' : 'w-[100px] text-[18px]'} {focused === 'a' ? 'bg-card shadow-[inset_0_0_0_2px_var(--accent)]' : 'bg-fill'} {bad ? 'text-danger shadow-[inset_0_0_0_2px_var(--danger)]' : ''}" />
        {:else}
          <input id="tf-b" bind:this={elB} bind:value={b} autocomplete="off" placeholder={tdisp(est.b) || "hh:mm"} aria-invalid={bad} aria-describedby="tf-help"
            onfocus={(e) => onFocus("b", e)} onblur={() => onBlur("b")} onkeydown={(e) => fieldKeys("b", e)} oninput={() => (error = "")}
            class="num h-12 rounded-[12px] pr-7 pl-1.5 text-center font-semibold outline-none {uses12h() ? 'w-[118px] text-[16px]' : 'w-[100px] text-[18px]'} {focused === 'b' ? 'bg-card shadow-[inset_0_0_0_2px_var(--accent)]' : 'bg-fill'} {bad ? 'text-danger shadow-[inset_0_0_0_2px_var(--danger)]' : ''}" />
        {/if}
        <button type="button" tabindex="-1" class="press absolute top-1.5 right-1 grid h-9 w-7 place-items-center rounded-[9px] text-muted hover:bg-fill2 hover:text-accent" aria-label="Pick {LABEL[w]} on the clock (Alt+↓)" title="Pick on the clock (Alt+↓)"
          onmousedown={(e) => e.preventDefault()} onclick={() => (dial = w)}><Icon name="clock" size={17} stroke={2} /></button>
      </div>
    {:else}
      <button type="button" id="tf-{w}" aria-haspopup="dialog" aria-label="{LABEL[w]}: {val(v) ? tdisp(val(v)) : est[w] ? 'estimate ' + tdisp(est[w]) : 'not set'}. Pick on the clock"
        class="press num h-12 shrink-0 rounded-[12px] px-2 text-center font-semibold {uses12h() ? 'w-[104px] text-[17px]' : 'w-24 text-[19px]'} {dial === w ? 'bg-card shadow-[inset_0_0_0_2px_var(--accent)]' : 'bg-fill'} {bad ? 'text-danger shadow-[inset_0_0_0_2px_var(--danger)]' : val(v) ? '' : 'text-muted'}"
        onclick={() => (dial = w)}>{v || tdisp(est[w]) || "––:––"}</button>
    {/if}
    <button type="button" tabindex="-1" class="press grid h-12 shrink-0 place-items-center rounded-[12px] bg-fill text-[14.5px] font-semibold text-ink2 {desktop ? 'w-11' : 'w-12'}" aria-label="{LABEL[w]} 15 minutes later" onmousedown={(e) => e.preventDefault()} onclick={() => nudge(w, 15)}>+15</button>
  </div>
{/snippet}

{#snippet form()}
  <div class="card px-4 py-1">{@render timeRow("a")}<div class="h-px bg-sep"></div>{@render timeRow("b")}</div>
  <div class="mx-1 mt-2.5 flex text-[13px]">
    <span class="text-muted">{desktop ? `Type ${typeHint} or use the clock` : "Tap a time to pick it on the clock"}</span><span class="flex-1"></span>
    {#if asleep != null && asleep > 0 && !(nap && asleep > 360)}<span class="num text-ink2">{nap ? "Nap" : "Asleep"} <b class="text-ink">{dur(asleep)}</b></span>{/if}
  </div>
  {#if error}<p role="alert" class="mx-1 mt-2 mb-0 text-[14px] text-danger">{error}</p>{/if}
  {#if !nap}
    <div class="mx-1 mt-[22px] mb-2 text-[13.5px] font-semibold text-ink2">Notes</div>
    <div class="card">
      <label class="block px-4 pt-2.5 pb-3">
        <span class="flex items-center gap-2 text-[12.5px] font-[550] text-muted"><Icon name="moon" size={14} stroke={2} />Before bed</span>
        <textarea bind:this={elBefore} bind:value={before} rows="2" maxlength="500" placeholder="Caffeine, mood, what's on your mind…" class="mt-1.5 block w-full resize-none bg-transparent text-[15.5px] leading-snug outline-none"></textarea>
      </label>
      <div class="h-px bg-sep"></div>
      <label class="block px-4 pt-2.5 pb-3">
        <span class="flex items-center gap-2 text-[12.5px] font-[550] text-muted"><Icon name="sun" size={14} stroke={2} />After waking</span>
        <textarea bind:this={elAfter} bind:value={after} rows="2" maxlength="500" placeholder="Dreams, how you feel…" class="mt-1.5 block w-full resize-none bg-transparent text-[15.5px] leading-snug outline-none"></textarea>
      </label>
    </div>
  {/if}
{/snippet}

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="flex h-full min-h-0 flex-col" onkeydown={panelKeys}>
  {#if desktop}
    <div class="flex flex-1 flex-col overflow-y-auto px-5 py-[22px]">
      <div class="mb-3.5 flex items-center">
        <div class="flex flex-1 flex-col"><h2 class="m-0 text-[18px] font-bold tracking-[-0.02em]">{nap ? "Add nap" : "Adjust times"}</h2><span class="text-[12.5px] text-muted">{nap ? `Nap · ${dayLong(edit.night)}` : nightRange(edit.night)}</span></div>
        <button type="button" class="press -mr-2 grid h-11 w-11 place-items-center rounded-[12px] text-ink2" aria-label="Close (Esc)" onclick={close}><Icon name="close" size={20} stroke={2} /></button>
      </div>
      {@render form()}
      <div id="tf-help" class="mx-1 mt-3.5 rounded-[10px] bg-fill2 px-3 py-2 text-[12px] leading-[1.7] text-muted">
        <b class="font-semibold text-ink2">Alt+↓</b> clock dial · <b class="font-semibold text-ink2">↑ ↓</b> ±15 min<br /><b class="font-semibold text-ink2">Tab</b> next field · <b class="font-semibold text-ink2">Ctrl+Enter</b> save · <b class="font-semibold text-ink2">Esc</b> close
      </div>
      <span class="flex-1"></span>
      {#if canReset}<button type="button" class="press mt-4 mb-2 h-11 w-full text-[15px] font-[550] text-accent" onclick={useEstimate}>Use the estimate instead</button>{/if}
      <button type="button" class="press mt-2 flex h-[52px] w-full items-center justify-center gap-1.5 rounded-[14px] bg-accent text-[16px] font-semibold text-on-accent disabled:opacity-60" disabled={saving} onclick={save}>{nap ? "Add nap" : "Save"} <span class="ml-1 text-[13px] font-medium opacity-70">Ctrl+Enter</span></button>
    </div>
  {:else}
    <div class="flex h-14 shrink-0 items-center pr-1.5 pl-1">
      <button type="button" class="press grid h-11 w-11 place-items-center rounded-[12px] text-ink2" aria-label="Back" onclick={close}><Icon name="back" /></button>
      <div class="ml-1 flex flex-col"><h2 class="m-0 text-[18px] font-bold tracking-[-0.02em]">{nap ? "Add nap" : "Adjust times"}</h2><span class="text-[12.5px] text-muted">{nap ? `Nap · ${dayLong(edit.night)}` : nightRange(edit.night)}</span></div>
    </div>
    <div class="min-h-0 flex-1 overflow-y-auto px-4 pt-3 pb-4">{@render form()}</div>
    <div class="shrink-0 bg-bg px-4 pt-3 pb-[max(20px,env(safe-area-inset-bottom))]">
      {#if canReset}<button type="button" class="press mb-2 h-11 w-full text-[15px] font-[550] text-accent" onclick={useEstimate}>Use the estimate instead</button>{/if}
      <button type="button" class="press h-[52px] w-full rounded-[14px] bg-accent text-[16px] font-semibold text-on-accent disabled:opacity-60" disabled={saving} onclick={save}>{nap ? "Add nap" : "Save"}</button>
    </div>
  {/if}
</div>
{#if dial}
  {#key dial}
    <TimeSheet title={LABEL[dial]} value={current(dial)} hint={est[dial] ? `Estimate ${tdisp(est[dial])}` : ""} {desktop} ondone={dialDone} oncancel={() => (dial = null)} />
  {/key}
{/if}
{#if confirm}
  <Confirm title={nap ? "Discard this nap?" : "Discard your changes?"} action="Discard" onconfirm={() => ((confirm = false), (saving = true), onclose())} oncancel={() => (confirm = false)} />
{/if}
