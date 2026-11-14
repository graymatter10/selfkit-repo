<script lang="ts">
  // Kanban board: To Do / Doing / Done.
  // Desktop: three columns, drag a card (mouse) to another column.
  // Phone: one column at a time (segmented control) and big "move" buttons on
  // each card, which work well with a thumb.
  import { Icon } from "@selfkit/ui";
  import type { Task } from "../lib/api";
  import { app, moveStage, stageOf, STAGE_LABEL, tagged, type Stage } from "../lib/state.svelte";
  import { sevMark, sortTasks, status, whenLabel } from "../lib/time";
  import Seg from "./Seg.svelte";

  let { desktop = false, onopen }: { desktop?: boolean; onopen: (t: Task) => void } = $props();
  const STAGES: Stage[] = ["todo", "doing", "done"];
  const DONE_SHOWN = 30;
  let col = $state<Stage>("todo");

  const cols = $derived.by(() => {
    const by: Record<Stage, Task[]> = { todo: [], doing: [], done: [] };
    for (const t of tagged()) by[stageOf(t)].push(t);
    by.todo = sortTasks(by.todo, app.sort);
    by.doing = sortTasks(by.doing, app.sort);
    by.done = by.done.sort((a, b) => (b.done_at ?? 0) - (a.done_at ?? 0)).slice(0, DONE_SHOWN);
    return by;
  });

  // ---- desktop drag (pointer events; no HTML5 DnD so it behaves the same everywhere)
  let drag = $state<{ id: string; x0: number; y0: number; dx: number; dy: number; on: boolean; over: Stage | null } | null>(null);
  let justDragged = false;
  function down(e: PointerEvent, t: Task) {
    if (!desktop || e.button !== 0) return;
    drag = { id: t.id, x0: e.clientX, y0: e.clientY, dx: 0, dy: 0, on: false, over: null };
  }
  function move(e: PointerEvent) {
    if (!drag) return;
    drag.dx = e.clientX - drag.x0;
    drag.dy = e.clientY - drag.y0;
    if (!drag.on && Math.hypot(drag.dx, drag.dy) > 6) drag.on = true;
    if (drag.on) {
      const el = document.elementsFromPoint(e.clientX, e.clientY).find((x) => (x as HTMLElement).dataset?.col) as HTMLElement | undefined;
      drag.over = (el?.dataset.col as Stage) ?? null;
    }
  }
  function up() {
    if (!drag) return;
    const d = drag;
    drag = null;
    if (d.on) {
      justDragged = true;
      setTimeout(() => (justDragged = false), 50);
      const t = app.tasks.find((x) => x.id === d.id);
      if (t && d.over) moveStage(t, d.over);
    }
  }
  const prev = (s: Stage) => STAGES[STAGES.indexOf(s) - 1];
  const next = (s: Stage) => STAGES[STAGES.indexOf(s) + 1];
</script>

<svelte:window onpointermove={move} onpointerup={up} onpointercancel={() => (drag = null)} />

{#snippet card(t: Task, s: Stage)}
  {@const st = status(t, app.now)}
  {@const dragging = drag?.on && drag.id === t.id}
  <div class="rounded-xl bg-card {dragging ? 'relative z-50 shadow-[0_12px_32px_rgba(0,0,0,0.22)]' : ''}" style={dragging ? `transform: translate(${drag!.dx}px, ${drag!.dy}px) rotate(1.5deg)` : ""}>
    <button type="button" onpointerdown={(e) => down(e, t)} onclick={() => !justDragged && onopen(t)}
      class="hl flex w-full flex-col gap-1 rounded-xl px-4 py-3 text-left {desktop ? 'cursor-grab touch-none' : ''}">
      <span class="flex items-baseline gap-1.5 text-[16px] leading-snug {t.done_at ? 'text-muted line-through' : ''}">
        {#if sevMark(t.severity) && !t.done_at}<span class="font-bold text-ink2">{sevMark(t.severity)}</span>{/if}<span class="line-clamp-3">{t.title}</span></span>
      {#if t.start || t.tags?.length || t.steps?.length}
        <span class="text-[13px] {st.kind === 'overdue' && !t.done_at ? 'text-danger' : 'text-muted'}">
          {[st.kind === "overdue" && !t.done_at ? st.text : whenLabel(t, app.now), t.steps?.length ? `${t.steps.filter((x) => x.done).length}/${t.steps.length}` : "", (t.tags ?? []).map((g) => "#" + g).join(" ")].filter(Boolean).join(" · ")}</span>
      {/if}
    </button>
    {#if !desktop}
      <div class="flex border-t border-sep text-[15px] text-accent">
        {#if prev(s)}<button type="button" class="hl flex h-11 flex-1 items-center justify-center gap-1" onclick={() => moveStage(t, prev(s))}><span class="rotate-180"><Icon name="chevron" size={15} stroke={2.4} /></span>{STAGE_LABEL[prev(s)]}</button>{/if}
        {#if next(s)}<button type="button" class="hl flex h-11 flex-1 items-center justify-center gap-1 {prev(s) ? 'border-l border-sep' : ''}" onclick={() => moveStage(t, next(s))}>{STAGE_LABEL[next(s)]}<Icon name="chevron" size={15} stroke={2.4} /></button>{/if}
      </div>
    {/if}
  </div>
{/snippet}

{#if desktop}
  <div class="grid grid-cols-3 gap-4">
    {#each STAGES as s}
      <section data-col={s} aria-label={STAGE_LABEL[s]} class="flex min-h-[300px] flex-col gap-2 rounded-2xl p-2 transition-colors {drag?.on && drag.over === s ? 'bg-accent-soft' : 'bg-fill2'}">
        <h2 class="m-0 flex justify-between px-2 pt-1 text-[13px] font-semibold tracking-wide text-muted uppercase"><span>{STAGE_LABEL[s]}</span><span class="tabular-nums">{cols[s].length}</span></h2>
        {#each cols[s] as t (t.id)}{@render card(t, s)}{/each}
        {#if !cols[s].length}<p class="m-0 px-2 py-6 text-center text-[14px] text-muted">{s === "doing" ? "Drag a task here when you start it" : "Empty"}</p>{/if}
      </section>
    {/each}
  </div>
{:else}
  <div class="flex flex-col gap-3">
    <Seg label="Column" items={STAGES.map((s) => ({ id: s, name: `${STAGE_LABEL[s]} ${cols[s].length}` }))} value={col} onchange={(v) => (col = v)} />
    {#each cols[col] as t (t.id)}{@render card(t, col)}{/each}
    {#if !cols[col].length}<p class="m-0 pt-16 text-center text-[17px] text-muted">{col === "doing" ? "Nothing in progress" : col === "done" ? "Nothing done yet" : "Nothing to do"}</p>{/if}
  </div>
{/if}
