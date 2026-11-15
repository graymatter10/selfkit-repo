<script lang="ts">
  // Shared conflict screen: an item changed on two devices. A = this device,
  // B = the other one. Keep A / Keep B / Merge field by field; long text gets a
  // line list where the user ticks the lines to keep. Same CSS variables as SyncSettings.
  import { onMount } from "svelte";
  import { bottomInset, label, show, syncApi, type Choice, type ConflictView, type SeenPeer } from "./api";

  let { phone = false, onclose, names = {} }: {
    phone?: boolean;
    onclose?: () => void;
    /** Optional readable names per collection, e.g. { tasks: "Agenda task" }. */
    names?: Record<string, string>;
  } = $props();

  let list = $state<ConflictView[]>([]);
  let peers = $state<SeenPeer[]>([]);
  let names2 = $state<Record<string, string>>({});
  const inset = bottomInset();
  let myName = $state("This device");
  let idx = $state(0);
  let picks = $state<Record<string, "a" | "b">>({});
  let lines = $state<Record<string, number[]>>({});
  let loaded = $state(false);
  let err = $state<string | null>(null);

  async function load() {
    try {
      const [cs, st] = await Promise.all([syncApi.conflicts(), syncApi.status()]);
      list = cs;
      peers = st.peers;
      names2 = st.device_names ?? {};
      myName = phone ? "This phone" : "This PC";
      if (idx >= list.length) idx = Math.max(0, list.length - 1);
      reset();
    } catch (e) {
      err = String(e);
    }
    loaded = true;
  }
  onMount(load);

  const cur = $derived(list[idx] ?? null);
  function reset() {
    const c = list[idx];
    if (!c) return;
    picks = Object.fromEntries(c.fields.map((f) => [f.key, "a" as const]));
    lines = Object.fromEntries(c.fields.filter((f) => f.lines).map((f) => [f.key, f.lines!.map((l, i) => (l.tag === "Local" ? i : -1)).filter((i) => i >= 0)]));
  }
  const other = (dev: string) => names2[dev] ?? peers.find((p) => p.device_id === dev)?.name ?? "Other device";
  const when = (t: number) => {
    const d = new Date(t), today = new Date();
    const time = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    return d.toDateString() === today.toDateString() ? `Today ${time}` : `${d.toLocaleDateString([], { day: "numeric", month: "short" })} ${time}`;
  };
  const title = (c: ConflictView) => {
    const d = (c.a.deleted ? c.b.data : c.a.data) as Record<string, unknown>;
    return String(d.title ?? d.name ?? d.note ?? d.night ?? d.day ?? d.person ?? label(c.collection));
  };
  /** Timestamps (`*_at`, UTC ms) as dates; everything else as text. */
  const fmt = (key: string, v: unknown) =>
    key === "day_start_min" && typeof v === "number" ? `${String(Math.floor(v / 60)).padStart(2, "0")}:${String(v % 60).padStart(2, "0")}` :
    typeof v === "number" && /(^|_)(at|until)$/.test(key) && v > 1e11 ? new Date(v).toLocaleString([], { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" }) : show(v);
  const kindName = (c: ConflictView) => names[c.collection] ?? `${label(c.store)} · ${label(c.collection)}`;
  function toggleLine(key: string, i: number) {
    const s = new Set(lines[key] ?? []);
    if (s.has(i)) s.delete(i); else s.add(i);
    lines[key] = [...s].sort((a, b) => a - b);
  }
  const picked = (key: string) => (lines[key] ?? []).length;

  async function resolve(choice: Choice) {
    if (!cur) return;
    err = null;
    try {
      await syncApi.resolve(cur.store, cur.id, choice);
      list = list.filter((_, i) => i !== idx);
      if (idx >= list.length) idx = Math.max(0, list.length - 1);
      reset();
      if (!list.length) onclose?.();
    } catch (e) {
      err = String(e);
    }
  }
  const merge = () => resolve({ choice: "merge", fields: $state.snapshot(picks), lines: $state.snapshot(lines) });
</script>

{#snippet check()}<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg>{/snippet}

{#snippet tile(side: "a" | "b", key: string, v: unknown, gone: boolean)}
  <button type="button" class="k-pick k-{side}" class:k-sel={picks[key] === side} aria-pressed={picks[key] === side} onclick={() => (picks[key] = side)}>
    <span class="k-tag">{side.toUpperCase()}</span>
    <span class="k-v" class:k-gone={gone}>{gone ? "Removed" : fmt(key, v)}</span>
    <span class="k-chk">{#if picks[key] === side}{@render check()}{/if}</span>
  </button>
{/snippet}

{#snippet detail(c: ConflictView)}
  <div class="k-kind">{kindName(c)}</div>
  <div class="k-title">{title(c)}</div>
  <div class="k-who">
    <div class="k-sa"><div class="k-k">A</div><div class="k-d">{myName}</div><div class="k-m">{c.a.deleted ? "Deleted" : "Edited"} {when(c.a.updated_at)}</div></div>
    <div class="k-sb"><div class="k-k">B</div><div class="k-d">{other(c.b.device_id)}</div><div class="k-m">{c.b.deleted ? "Deleted" : "Edited"} {when(c.b.updated_at)}</div></div>
  </div>
  {#if c.delete_vs_edit}
    <div class="k-explain">
      <div class="k-t">{c.a.deleted ? `Deleted here, edited on ${other(c.b.device_id)}` : `Edited here, deleted on ${other(c.b.device_id)}`}</div>
      <div class="k-s">{c.a.deleted ? "Keep A keeps it deleted everywhere (you can undo). Keep B brings it back with the other device's changes." : "Keep A keeps your edit and brings the item back everywhere. Keep B deletes it here too (you can undo)."}</div>
    </div>
  {:else}
    <div class="k-card k-fields">
      {#each c.fields as f (f.key)}
        <div class="k-frow">
          <div class="k-lab"><span>{label(f.key)}</span>{#if f.lines}<span class="k-cnt">{picked(f.key)} line{picked(f.key) === 1 ? "" : "s"} picked</span>{/if}</div>
          {#if f.lines}
            <div class="k-diff">
              {#each f.lines as l, i}
                {#if l.tag === "Same"}
                  <div class="k-dl k-same"><span class="k-cbx"></span><span class="k-mk"></span><span class="k-tx">{l.text || " "}</span></div>
                {:else}
                  <button type="button" class="k-dl {l.tag === 'Local' ? 'k-la' : 'k-lb'}" aria-pressed={(lines[f.key] ?? []).includes(i)} onclick={() => toggleLine(f.key, i)}>
                    <span class="k-cb" class:k-on={(lines[f.key] ?? []).includes(i)}>{#if (lines[f.key] ?? []).includes(i)}{@render check()}{/if}</span>
                    <span class="k-mk">{l.tag === "Local" ? "A" : "B"}</span><span class="k-tx">{l.text || " "}</span>
                  </button>
                {/if}
              {/each}
            </div>
          {:else}
            <div class="k-picks">{@render tile("a", f.key, f.a, f.a === undefined || f.a === null)}{@render tile("b", f.key, f.b, f.b === undefined || f.b === null)}</div>
          {/if}
        </div>
      {/each}
      {#if c.same.length}<div class="k-same">{c.same.map(label).join(", ")} {c.same.length === 1 ? "is" : "are"} the same</div>{/if}
    </div>
  {/if}
  {#if err}<p class="k-err">{err}</p>{/if}
{/snippet}

{#snippet actions(c: ConflictView)}
  {#if c.delete_vs_edit}
    <button type="button" class="k-btn" class:k-pri={!c.a.deleted} onclick={() => resolve({ choice: "keep_a" })}>{c.a.deleted ? "Keep A · deleted" : "Keep A"}</button>
    <button type="button" class="k-btn" class:k-pri={c.a.deleted} onclick={() => resolve({ choice: "keep_b" })}>{c.b.deleted ? "Keep B · delete" : "Keep B"}</button>
  {:else}
    <button type="button" class="k-btn" onclick={() => resolve({ choice: "keep_a" })}>Keep A</button>
    <button type="button" class="k-btn" onclick={() => resolve({ choice: "keep_b" })}>Keep B</button>
    <button type="button" class="k-btn k-pri" onclick={merge}>Merge</button>
  {/if}
{/snippet}

<div class="skc" class:k-phone={phone}>
  {#if !loaded}
    <p class="k-empty">Loading…</p>
  {:else if !cur}
    <div class="k-empty"><div class="k-t">Nothing to review</div><div class="k-s">Every change is in sync.</div>{#if onclose}<button type="button" class="k-btn" onclick={() => onclose?.()}>Back</button>{/if}</div>
  {:else if phone}
    <div class="k-ph">
      <div class="k-scroll">
        {#if list.length > 1}
          <div class="k-pager"><button type="button" class="k-btn k-sm" disabled={idx === 0} onclick={() => { idx--; reset(); }}>Previous</button><span>{idx + 1} of {list.length}</span><button type="button" class="k-btn k-sm" disabled={idx === list.length - 1} onclick={() => { idx++; reset(); }}>Next</button></div>
        {/if}
        {@render detail(cur)}
      </div>
      <div class="k-bar" style={inset ? `padding-bottom:${12 + inset}px` : ""}>{@render actions(cur)}</div>
    </div>
  {:else}
    <div class="k-desk">
      <div class="k-clist">
        {#each list as c, i (c.store + c.id)}
          <button type="button" class="k-citem" class:k-on={i === idx} onclick={() => { idx = i; reset(); }}>
            <div class="k-t">{title(c)}</div><div class="k-s">{kindName(c)} · {c.delete_vs_edit ? "edited and deleted" : "changed on both"}</div>
          </button>
        {/each}
        <p class="k-hint">Nothing is lost while you decide. Both versions stay on every device.</p>
      </div>
      <div class="k-cdetail">
        <div class="k-scroll">{@render detail(cur)}</div>
        <div class="k-bar" style={inset ? `padding-bottom:${12 + inset}px` : ""}><span class="k-hint k-grow">{cur.delete_vs_edit ? "" : "Merge saves your picks as a new version on every device."}</span>{@render actions(cur)}</div>
      </div>
    </div>
  {/if}
</div>

<style>
  .skc {
    --a: var(--sync-accent, var(--accent, #3e6bb3));
    --on-a: var(--sync-on-accent, #fff);
    --surf: var(--sync-surface, var(--surface, #fff));
    --sunk: var(--sync-sunk, color-mix(in srgb, var(--ink-c) 6%, transparent));
    --line-c: var(--sync-line, var(--line, rgba(127, 127, 127, 0.22)));
    --ink-c: var(--sync-ink, var(--ink, currentColor));
    --mut: var(--sync-muted, var(--muted, rgba(127, 127, 127, 0.95)));
    --bad: var(--sync-danger, var(--danger, #c2412d));
    --sa: var(--sync-a, #4a72c4);
    --sb: var(--sync-b, #b7772f);
    --sa-soft: color-mix(in srgb, var(--sa) 13%, transparent);
    --sb-soft: color-mix(in srgb, var(--sb) 14%, transparent);
    --r: var(--sync-radius, 16px);
    color: var(--ink-c); font-size: 14px; line-height: 1.4; height: 100%; display: flex; flex-direction: column; min-height: 0;
  }
  .k-grow { flex: 1; min-width: 0; }
  .k-t { font-weight: 600; } .k-s { color: var(--mut); font-size: 12.5px; margin-top: 2px; }
  .k-hint { color: var(--mut); font-size: 12.5px; margin: 0; }
  .k-empty { padding: 40px 16px; text-align: center; display: grid; gap: 10px; justify-items: center; }
  .k-btn { height: 44px; padding: 0 16px; border-radius: 12px; font: inherit; font-weight: 600; border: 1px solid var(--line-c); background: var(--surf); color: var(--ink-c); cursor: pointer; white-space: nowrap; }
  .k-btn.k-pri { background: var(--a); color: var(--on-a); border-color: transparent; }
  .k-btn.k-sm { padding: 0 12px; font-size: 13px; }
  .k-btn:disabled { opacity: 0.45; }
  .k-ph { display: flex; flex-direction: column; flex: 1; min-height: 0; }
  .k-scroll { flex: 1; overflow: auto; padding: 4px 16px 20px; min-height: 0; }
  .k-bar { display: flex; gap: 8px; padding: 12px 16px 16px; border-top: 1px solid var(--line-c); background: var(--surf); align-items: center; }
  .k-phone .k-bar .k-btn { flex: 1; height: 46px; }
  .k-phone .k-bar { padding-bottom: calc(12px + env(safe-area-inset-bottom, 0px)); }
  .k-pager { display: flex; align-items: center; justify-content: space-between; gap: 8px; margin: 4px 0 12px; color: var(--mut); font-size: 13px; }
  .k-kind { color: var(--mut); font-size: 12.5px; margin-top: 4px; }
  .k-title { font-size: 18px; font-weight: 650; margin: 0 0 12px; overflow-wrap: anywhere; }
  .k-who { display: flex; gap: 8px; }
  .k-who > div { flex: 1; padding: 10px 12px; border-radius: 12px; min-width: 0; }
  .k-who .k-k { font-size: 11.5px; font-weight: 700; letter-spacing: 0.05em; }
  .k-who .k-d { font-weight: 600; margin-top: 2px; }
  .k-who .k-m { color: var(--mut); font-size: 12px; }
  .k-sa { background: var(--sa-soft); } .k-sa .k-k { color: var(--sa); }
  .k-sb { background: var(--sb-soft); } .k-sb .k-k { color: var(--sb); }
  .k-card { background: var(--surf); border: 1px solid var(--line-c); border-radius: var(--r); }
  .k-fields { padding: 0 14px; margin-top: 12px; }
  .k-frow { padding: 14px 0 4px; }
  .k-frow + .k-frow { border-top: 1px solid var(--line-c); }
  .k-lab { font-size: 12.5px; font-weight: 600; color: var(--mut); margin-bottom: 8px; display: flex; justify-content: space-between; }
  .k-cnt { color: var(--a); }
  .k-picks { display: flex; gap: 8px; margin-bottom: 10px; }
  .k-pick { flex: 1; min-width: 0; border: 1.5px solid var(--line-c); border-radius: 12px; padding: 10px 30px 10px 12px; position: relative; min-height: 52px; background: var(--surf); text-align: left; font: inherit; color: inherit; cursor: pointer; display: flex; flex-direction: column; justify-content: center; }
  .k-pick .k-tag { font-size: 11px; font-weight: 700; letter-spacing: 0.05em; margin-bottom: 2px; }
  .k-pick.k-a .k-tag { color: var(--sa); } .k-pick.k-b .k-tag { color: var(--sb); }
  .k-pick.k-sel.k-a { border-color: var(--sa); background: var(--sa-soft); }
  .k-pick.k-sel.k-b { border-color: var(--sb); background: var(--sb-soft); }
  .k-pick .k-chk { position: absolute; top: 9px; right: 9px; width: 18px; height: 18px; border-radius: 9px; border: 1.5px solid var(--line-c); box-sizing: border-box; display: grid; place-items: center; color: #fff; }
  .k-pick.k-sel .k-chk { border: none; }
  .k-pick.k-sel.k-a .k-chk { background: var(--sa); } .k-pick.k-sel.k-b .k-chk { background: var(--sb); }
  .k-v { font-weight: 550; overflow-wrap: anywhere; }
  .k-v.k-gone { color: var(--mut); font-weight: 500; font-style: italic; }
  .k-diff { border: 1px solid var(--line-c); border-radius: 12px; overflow: hidden; margin-bottom: 10px; font-size: 13px; }
  .k-dl { display: flex; align-items: center; gap: 10px; padding: 8px 12px; min-height: 44px; width: 100%; box-sizing: border-box; background: none; border: 0; font: inherit; color: inherit; text-align: left; }
  .k-dl + .k-dl { border-top: 1px solid var(--line-c); }
  button.k-dl { cursor: pointer; }
  .k-dl .k-mk { width: 14px; flex: none; font-weight: 700; font-size: 11px; }
  .k-dl.k-same .k-tx { color: var(--mut); }
  .k-dl.k-la { background: var(--sa-soft); } .k-dl.k-la .k-mk { color: var(--sa); }
  .k-dl.k-lb { background: var(--sb-soft); } .k-dl.k-lb .k-mk { color: var(--sb); }
  .k-cb, .k-cbx { width: 18px; height: 18px; border-radius: 5px; flex: none; box-sizing: border-box; display: grid; place-items: center; color: #fff; }
  .k-cb { border: 1.5px solid var(--mut); }
  .k-cb.k-on { border: none; }
  .k-dl.k-la .k-cb.k-on { background: var(--sa); } .k-dl.k-lb .k-cb.k-on { background: var(--sb); }
  .k-tx { flex: 1; overflow-wrap: anywhere; white-space: pre-wrap; }
  .k-same { color: var(--mut); font-size: 12.5px; padding: 12px 0; border-top: 1px solid var(--line-c); }
  .k-explain { margin-top: 12px; padding: 14px; border-radius: var(--r); background: var(--sunk); }
  .k-err { color: var(--bad); margin: 12px 2px 0; font-size: 13px; }
  .k-desk { display: flex; flex: 1; min-height: 0; }
  .k-clist { width: 300px; border-right: 1px solid var(--line-c); overflow: auto; flex: none; }
  .k-citem { display: block; width: 100%; text-align: left; padding: 14px 20px; border: 0; border-bottom: 1px solid var(--line-c); background: none; font: inherit; color: inherit; cursor: pointer; }
  .k-citem.k-on { background: var(--surf); box-shadow: inset 3px 0 0 var(--a); }
  .k-clist .k-hint { padding: 16px 20px; }
  .k-cdetail { flex: 1; display: flex; flex-direction: column; min-width: 0; }
  .k-cdetail .k-scroll { padding: 20px 28px; }
  .k-cdetail .k-bar { background: transparent; padding: 14px 28px; }
</style>
