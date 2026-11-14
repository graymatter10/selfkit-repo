<script lang="ts">
  // Adjust times (or add a nap). Phone: a page with the in-app number pad
  // under the fields while a time field is focused; desktop: a side panel
  // typed on the keyboard (↑/↓ ±15 min, Tab next field, Enter/Ctrl+Enter save,
  // Esc close). Leaving with changes asks first ("Back never loses input").
  import { onMount } from "svelte";
  import type { Edit } from "../lib/state.svelte";
  import { addNap, app, nightByKey, resetToEstimate, setLog } from "../lib/state.svelte";
  import { dur, hm, nightRange, normTime, shiftTime, toMin } from "../lib/fmt";
  import Confirm from "./Confirm.svelte";
  import Icon from "./Icon.svelte";

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

  let a = $state(init.a), b = $state(init.b);
  let before = $state(init.before), after = $state(init.after);
  let focused: "a" | "b" | null = $state(null);
  let lastField: "a" | "b" | null = null; // refocused after "Keep editing"
  let fresh = false; // first key after focusing replaces the value
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
  const canReset = $derived(!nap && night?.source === "logged" && night.estimate.status === "found");

  function keepEditing() {
    confirm = false;
    if (!desktop && lastField) setTimeout(() => (lastField === "a" ? elA : elB)?.focus(), 30);
  }
  function guard() {
    if (confirm) return keepEditing(), true;
    if (dirty && !saving) return (lastField = focused), (confirm = true), true;
    return false;
  }
  onMount(() => {
    app.closeGuard = guard;
    const f = edit.focus ?? (nap || !init.a ? "bed" : "bed");
    setTimeout(() => {
      if (f === "note_before") elBefore?.focus();
      else if (f === "note_after") elAfter?.focus();
      else if (f === "wake") elB?.focus();
      else elA?.focus();
    }, 60);
    return () => {
      if (app.closeGuard === guard) app.closeGuard = null;
    };
  });
  function close() {
    if (!guard()) onclose();
  }

  function nudge(which: "a" | "b", by: number) {
    const cur = val(which === "a" ? a : b) || (which === "a" ? est.a || (nap ? "14:00" : "23:00") : est.b || (nap ? "14:30" : "07:00"));
    const next = shiftTime(cur!, by);
    if (which === "a") a = next;
    else b = next;
    error = "";
  }

  // Number pad (phone) — types into the focused time field.
  function key(k: string) {
    if (!focused) return;
    let s = focused === "a" ? a : b;
    if (fresh && k !== "⌫" && k !== "next") s = "";
    fresh = false;
    if (k === "⌫") s = s.slice(0, -1);
    else if (k === "next") return next();
    else if (k === "am" || k === "pm") s = s.replace(/\s*(am|pm)$/, "") + k;
    else if (k === ".") { if (!s.includes(":") && s.length) s += ":"; }
    else if (s.replace(/\D/g, "").length < 4 && !/(am|pm)$/.test(s)) s += k;
    if (focused === "a") a = s;
    else b = s;
    error = "";
  }
  function next() {
    if (focused === "a") elB?.focus();
    else { elB?.blur(); focused = null; }
  }
  function onFocus(w: "a" | "b", e: FocusEvent) {
    focused = w;
    fresh = true;
    if (desktop) (e.target as HTMLInputElement).select();
  }
  function onBlur(w: "a" | "b") {
    const v = w === "a" ? a : b;
    const t = normTime(v);
    if (t) w === "a" ? (a = t) : (b = t);
    setTimeout(() => {
      if (document.activeElement !== elA && document.activeElement !== elB) focused = null;
    }, 0);
  }
  function fieldKeys(w: "a" | "b", e: KeyboardEvent) {
    if (e.key === "ArrowUp" || e.key === "ArrowDown") {
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
    if (invalid(a) || invalid(b)) return void (error = "Check the times: type 2330, 23:30 or 11:30pm.");
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
  <div class="flex items-center gap-2 py-3">
    <div class="flex min-w-0 flex-1 flex-col gap-1">
      <label for="tf-{w}" class="text-[16px] font-[550]">{LABEL[w]}</label>
      {#if est[w]}<span class="num text-[12.5px] whitespace-nowrap text-muted">Estimate {est[w]}</span>{/if}
    </div>
    <button type="button" tabindex="-1" class="press grid h-12 w-12 shrink-0 place-items-center rounded-[12px] bg-fill text-[14.5px] font-semibold text-ink2" aria-label="{LABEL[w]} 15 minutes earlier" onmousedown={(e) => e.preventDefault()} onclick={() => nudge(w, -15)}>−15</button>
    {#if w === "a"}
      <input id="tf-a" bind:this={elA} bind:value={a} inputmode={desktop ? "text" : "none"} autocomplete="off" placeholder={est.a || "hh:mm"} aria-invalid={invalid(v)}
        onfocus={(e) => onFocus("a", e)} onblur={() => onBlur("a")} onkeydown={(e) => fieldKeys("a", e)} oninput={() => (error = "")}
        class="num h-12 w-24 shrink-0 rounded-[12px] text-center text-[19px] font-semibold outline-none {focused === 'a' ? 'bg-card shadow-[inset_0_0_0_2px_var(--accent)]' : 'bg-fill'} {invalid(v) ? 'text-danger shadow-[inset_0_0_0_2px_var(--danger)]' : ''}" />
    {:else}
      <input id="tf-b" bind:this={elB} bind:value={b} inputmode={desktop ? "text" : "none"} autocomplete="off" placeholder={est.b || "hh:mm"} aria-invalid={invalid(v)}
        onfocus={(e) => onFocus("b", e)} onblur={() => onBlur("b")} onkeydown={(e) => fieldKeys("b", e)} oninput={() => (error = "")}
        class="num h-12 w-24 shrink-0 rounded-[12px] text-center text-[19px] font-semibold outline-none {focused === 'b' ? 'bg-card shadow-[inset_0_0_0_2px_var(--accent)]' : 'bg-fill'} {invalid(v) ? 'text-danger shadow-[inset_0_0_0_2px_var(--danger)]' : ''}" />
    {/if}
    <button type="button" tabindex="-1" class="press grid h-12 w-12 shrink-0 place-items-center rounded-[12px] bg-fill text-[14.5px] font-semibold text-ink2" aria-label="{LABEL[w]} 15 minutes later" onmousedown={(e) => e.preventDefault()} onclick={() => nudge(w, 15)}>+15</button>
  </div>
{/snippet}

{#snippet form()}
  <div class="card px-4 py-1">{@render timeRow("a")}<div class="h-px bg-sep"></div>{@render timeRow("b")}</div>
  <div class="mx-1 mt-2.5 flex text-[13px]">
    <span class="text-muted">Type 2330 or 11:30pm</span><span class="flex-1"></span>
    {#if asleep != null && asleep > 0}<span class="num text-ink2">{nap ? "Nap" : "Asleep"} <b class="text-ink">{dur(asleep)}</b></span>{/if}
  </div>
  {#if error}<p role="alert" class="mx-1 mt-2 mb-0 text-[14px] text-danger">{error}</p>{/if}
  {#if !nap && !(focused && !desktop)}
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

{#snippet pad()}
  <div class="border-t border-sep bg-card px-2 pt-2 pb-[max(14px,env(safe-area-inset-bottom))]">
    <div class="flex items-center px-1.5 pb-2 text-[12.5px]">
      <span class="flex-1 text-muted">{focused === "a" ? LABEL.a : LABEL.b} · type 2330 or 11:30pm</span>
      <span class="flex gap-2">
        {#each ["am", "pm"] as k}<button type="button" class="press h-9 rounded-[10px] bg-fill px-3 text-[13px] font-semibold text-ink2" onmousedown={(e) => e.preventDefault()} onclick={() => key(k)}>{k}</button>{/each}
        <button type="button" class="press h-9 rounded-[10px] bg-fill px-3 text-[13px] font-semibold text-accent" onmousedown={(e) => e.preventDefault()} onclick={() => key("next")}>{focused === "a" ? "Next" : "Done"}</button>
      </span>
    </div>
    <div class="grid grid-cols-3 gap-1.5">
      {#each ["1", "2", "3", "4", "5", "6", "7", "8", "9", ".", "0", "⌫"] as k}
        <button type="button" class="press num grid h-[50px] place-items-center rounded-[12px] bg-fill text-[22px] font-[550]" aria-label={k === "⌫" ? "Delete" : k === "." ? "Colon" : k}
          onmousedown={(e) => e.preventDefault()} onclick={() => key(k)}>{#if k === "⌫"}<Icon name="bksp" size={24} stroke={1.8} />{:else}{k}{/if}</button>
      {/each}
    </div>
  </div>
{/snippet}

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="flex h-full min-h-0 flex-col" onkeydown={panelKeys}>
  {#if desktop}
    <div class="flex flex-1 flex-col overflow-y-auto px-5 py-[22px]">
      <div class="mb-3.5 flex items-center">
        <div class="flex flex-1 flex-col"><h2 class="m-0 text-[18px] font-bold tracking-[-0.02em]">{nap ? "Add nap" : "Adjust times"}</h2><span class="text-[12.5px] text-muted">{nightRange(edit.night)}</span></div>
        <button type="button" class="press -mr-2 grid h-11 w-11 place-items-center rounded-[12px] text-ink2" aria-label="Close (Esc)" onclick={close}><Icon name="close" size={20} stroke={2} /></button>
      </div>
      {@render form()}
      <div class="mx-1 mt-3.5 rounded-[10px] bg-fill2 px-3 py-2 text-[12px] leading-[1.7] text-muted">
        Type a time (2330, 11:30pm) · <b class="font-semibold text-ink2">↑ ↓</b> ±15 min<br /><b class="font-semibold text-ink2">Tab</b> next field · <b class="font-semibold text-ink2">Ctrl+Enter</b> save · <b class="font-semibold text-ink2">Esc</b> close
      </div>
      <span class="flex-1"></span>
      {#if canReset}<button type="button" class="press mt-4 mb-2 h-11 w-full text-[15px] font-[550] text-accent" onclick={useEstimate}>Use the estimate instead</button>{/if}
      <button type="button" class="press mt-2 flex h-[52px] w-full items-center justify-center gap-1.5 rounded-[14px] bg-accent text-[16px] font-semibold text-on-accent disabled:opacity-60" disabled={saving} onclick={save}>{nap ? "Add nap" : "Save"} <span class="ml-1 text-[13px] font-medium opacity-70">Ctrl+Enter</span></button>
    </div>
  {:else}
    <div class="flex h-14 shrink-0 items-center pr-1.5 pl-1">
      <button type="button" class="press grid h-11 w-11 place-items-center rounded-[12px] text-ink2" aria-label="Back" onclick={close}><Icon name="back" /></button>
      <div class="ml-1 flex flex-col"><h2 class="m-0 text-[18px] font-bold tracking-[-0.02em]">{nap ? "Add nap" : "Adjust times"}</h2><span class="text-[12.5px] text-muted">{nightRange(edit.night)}</span></div>
    </div>
    <div class="min-h-0 flex-1 overflow-y-auto px-4 pt-3 pb-4">{@render form()}</div>
    <div class="shrink-0 bg-bg">
      {#if focused}
        <div class="flex items-center gap-2 px-4 py-2.5">
          {#if canReset}<button type="button" class="press h-11 px-1 text-[15px] font-[550] text-accent" onmousedown={(e) => e.preventDefault()} onclick={useEstimate}>Use the estimate</button>{/if}
          <span class="flex-1"></span>
          <button type="button" class="press h-12 rounded-[14px] bg-accent px-7 text-[16px] font-semibold text-on-accent" onmousedown={(e) => e.preventDefault()} onclick={save}>{nap ? "Add nap" : "Save"}</button>
        </div>
        {@render pad()}
      {:else}
        <div class="px-4 pt-3 pb-[max(20px,env(safe-area-inset-bottom))]">
          {#if canReset}<button type="button" class="press mb-2 h-11 w-full text-[15px] font-[550] text-accent" onclick={useEstimate}>Use the estimate instead</button>{/if}
          <button type="button" class="press h-[52px] w-full rounded-[14px] bg-accent text-[16px] font-semibold text-on-accent disabled:opacity-60" disabled={saving} onclick={save}>{nap ? "Add nap" : "Save"}</button>
        </div>
      {/if}
    </div>
  {/if}
</div>
{#if confirm}
  <Confirm title={nap ? "Discard this nap?" : "Discard your changes?"} action="Discard" onconfirm={() => ((confirm = false), (saving = true), onclose())} oncancel={keepEditing} />
{/if}
