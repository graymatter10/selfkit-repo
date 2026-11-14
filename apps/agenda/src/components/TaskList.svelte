<script lang="ts">
  import type { Task } from "../lib/api";
  import { app, groups } from "../lib/state.svelte";
  import TaskRow from "./TaskRow.svelte";
  let { desktop = false, onopen }: { desktop?: boolean; onopen: (t: Task) => void } = $props();
  const gs = $derived(groups());
  const empty = $derived({ active: "No open tasks", all: "No tasks yet", done: "Nothing completed yet", overdue: "Nothing overdue" }[app.filter]);
</script>

<div class="flex flex-col gap-6">
  {#each gs as g (g.key)}
    <section aria-label={g.label}>
      <h2 class="m-0 px-4 pb-1.5 text-[13px] font-normal tracking-wide uppercase {g.key === 'overdue' ? 'text-danger' : 'text-muted'}">{g.label}</h2>
      <div class="overflow-hidden rounded-xl bg-card">
        {#each g.tasks as t, i (t.id)}<TaskRow task={t} last={i === g.tasks.length - 1} {desktop} {onopen} />{/each}
      </div>
    </section>
  {/each}
  {#if app.loaded && !gs.length}
    <div class="flex flex-col items-center gap-1 pt-24 text-center">
      <span class="text-[20px] font-semibold text-ink2">{empty}</span>
      {#if app.filter !== "done"}<span class="text-[15px] text-muted">Tap “New Task” to add one.</span>{/if}
    </div>
  {/if}
</div>
