<script lang="ts">
  // Task details (iOS style): nav bar "‹ Tasks" / Edit, grouped rows, steps you
  // can tick, then "Mark as Done" and a red "Delete Task" cell.
  import { Icon } from "@selfkit/ui";
  import type { Task } from "../lib/api";
  import { SEVERITY, whenLabel, fmtMin, parseWhen, shortDay, status, dayLabel, hm, dayDiff } from "../lib/time";
  import { app, focusMin, patchSteps, remove, startFocus, stageOf, STAGE_LABEL, toggle } from "../lib/state.svelte";
  import Notes from "./Notes.svelte";

  let { task, layout = "page", onclose, onedit }: { task: Task; layout?: "page" | "panel"; onclose: () => void; onedit: () => void } = $props();
  const st = $derived(status(task, app.now));
  const s = $derived(parseWhen(task.start));
  const e = $derived(parseWhen(task.stop));
  const steps = $derived(task.steps ?? []);
  const done = $derived(!!task.done_at);
  const REP: Record<string, string> = { daily: "Every day", weekdays: "Weekdays", weekly: "Every week", monthly: "Every month", yearly: "Every year" };
  function day(d: Date) {
    const l = dayLabel(d, app.now);
    return l === "Today" || l === "Tomorrow" || l === "Yesterday" ? l : shortDay(d);
  }
  const startText = $derived(s ? (s.allDay ? `${day(s.date)}, all day` : `${day(s.date)}, ${hm(s.date)}`) : "None");
  const stopText = $derived(e && s ? `${dayDiff(e.date, s.date) ? day(e.date) + ", " : ""}${hm(e.date)}` : "");
  const durText = $derived(e && s ? fmtMin(Math.round((e.date.getTime() - s.date.getTime()) / 60_000)) : "");
  const reminderText = $derived(task.reminder_min == null ? "None" : task.reminder_min === 0 ? "At start" : `${fmtMin(task.reminder_min)} before`);
</script>

{#snippet row(name: string, value: string, last = false, danger = false)}
  <div class="flex min-h-11 items-center pl-4">
    <div class="flex flex-1 items-center gap-3 py-[11px] pr-4 {last ? '' : 'border-b border-sep'}">
      <span class="flex-1 text-[17px]">{name}</span><span class="text-right text-[17px] {danger ? 'text-danger' : 'text-muted'}">{value}</span>
    </div>
  </div>
{/snippet}

<div class="relative flex min-h-0 flex-1 flex-col bg-bg">
  <div class="flex items-center justify-between px-2 py-2 {layout === 'panel' ? 'border-b border-sep bg-card py-2.5' : ''}">
    <button type="button" class="press flex items-center py-2 pr-2 text-[17px] text-accent" onclick={onclose} aria-label={layout === "panel" ? "Close details" : "Back to tasks"}>
      {#if layout === "panel"}<span class="px-2">Close</span>{:else}<span class="rotate-180"><Icon name="chevron" size={22} stroke={2.4} /></span>Tasks{/if}
    </button>
    <button type="button" class="press px-2 py-2 text-[17px] text-accent" onclick={onedit}>Edit</button>
  </div>

  <div class="flex min-h-0 flex-1 flex-col gap-[22px] overflow-y-auto *:shrink-0 px-4 pt-2 pb-16">
    <div class="flex flex-col gap-1 px-1">
      <h1 class="m-0 text-[28px] leading-tight font-bold tracking-[-0.5px] {done ? 'text-muted line-through' : ''}">{task.title}</h1>
      <span class="text-[15px] {st.kind === 'overdue' && !done ? 'text-danger' : 'text-muted'}">{st.text || (done ? "Completed" : s ? whenLabel(task, app.now) : "No date")}</span>
    </div>

    <Notes notes={task.notes} photos={task.photos} />

    {#if task.tags?.length}
      <div class="flex flex-wrap gap-1.5 px-1">
        {#each task.tags as g}<button type="button" class="press h-7 rounded-full bg-fill px-3 text-[14px] text-ink2" onclick={() => ((app.tag = g), onclose())}>#{g}</button>{/each}
      </div>
    {/if}

    <div class="overflow-hidden rounded-xl bg-card">
      {@render row("Starts", startText, !e)}
      {#if e}{@render row("Ends", stopText)}{@render row("Duration", durText, true)}{/if}
    </div>

    <div class="overflow-hidden rounded-xl bg-card">
      {@render row("Severity", SEVERITY[task.severity] ?? "Medium")}
      {#if s}{@render row("Reminder", reminderText)}{/if}
      {@render row("Repeat", task.repeat ? (REP[task.repeat] ?? task.repeat) : "Never")}
      {@render row("List", app.lists.find((l) => l.id === (task.list ?? "inbox"))?.name ?? "Personal")}
      {@render row("Board", STAGE_LABEL[stageOf(task)], true)}
    </div>

    {#if steps.length}
      <div class="flex flex-col gap-2">
        <div class="flex justify-between px-4 text-[13px] text-muted uppercase"><span>Steps</span><span>{steps.filter((x) => x.done).length} of {steps.length}</span></div>
        <div class="overflow-hidden rounded-xl bg-card">
          {#each steps as stp, i (stp.id)}
            <button type="button" aria-pressed={stp.done} onclick={() => patchSteps(task, steps.map((x, j) => (j === i ? { ...x, done: !x.done } : x)))}
              class="hl flex min-h-11 w-full items-center gap-3 pl-4 text-left">
              {#if stp.done}<span class="flex h-[22px] w-[22px] shrink-0 items-center justify-center rounded-full bg-accent text-on-accent"><Icon name="check" size={13} stroke={3.2} /></span>
              {:else}<span class="h-[22px] w-[22px] shrink-0 rounded-full border-[1.5px] border-faint"></span>{/if}
              <span class="flex-1 py-[11px] pr-4 text-[17px] {i === steps.length - 1 ? '' : 'border-b border-sep'} {stp.done ? 'text-muted line-through' : ''}">{stp.title}</span>
            </button>
          {/each}
        </div>
      </div>
    {/if}

    {#if !done}
      <div class="overflow-hidden rounded-xl bg-card">
        {#if app.focus && app.focus.task === task.id}
          <button type="button" class="hl min-h-11 w-full py-[11px] text-[17px] text-accent" onclick={() => (app.focusOpen = true)}>Focus Running…</button>
        {:else}
          <button type="button" class="hl min-h-11 w-full py-[11px] text-[17px] text-accent" onclick={() => (startFocus(task), (app.focusOpen = true))}>Start Focus · {focusMin()} min</button>
        {/if}
      </div>
    {/if}

    <div class="overflow-hidden rounded-xl bg-card">
      <button type="button" class="hl flex min-h-11 w-full items-center justify-center text-[17px] text-accent" onclick={() => (toggle(task), layout === "page" && onclose())}>
        <span class="w-full border-b border-sep py-[11px]">{done ? "Mark as Not Done" : "Mark as Done"}</span></button>
      <button type="button" class="hl min-h-11 w-full py-[11px] text-[17px] text-danger" onclick={() => remove(task)}>Delete Task</button>
    </div>
  </div>
</div>
