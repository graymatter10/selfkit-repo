<script lang="ts">
  // One task row (inset grouped list): circle, quiet "!!" for High/Critical,
  // title, one grey detail line. Red appears only for overdue.
  import { Icon } from "@selfkit/ui";
  import type { Task } from "../lib/api";
  import { status, whenLabel, sevMark } from "../lib/time";
  import { app, toggle } from "../lib/state.svelte";

  let { task, last = false, desktop = false, onopen, when: whenOverride }: { task: Task; last?: boolean; desktop?: boolean; onopen: (t: Task) => void; when?: string } = $props();
  const done = $derived(!!task.done_at);
  /** First line of the notes without Markdown marks. */
  const plain = (n: string) => n.split("\n").find((l) => l.trim() && !/^!\[/.test(l.trim()))?.replace(/!\[[^\]]*\]\([^)]*\)|[#*_`>~]|\[( |x)\]/g, "").replace(/\[([^\]]+)\]\([^)]*\)/g, "$1").trim() ?? "";
  const st = $derived(status(task, app.now));
  const when = $derived(whenOverride ?? whenLabel(task, app.now));
  const steps = $derived(task.steps ?? []);
  const selected = $derived(desktop && "id" in app.panel && app.panel.id === task.id);
  const mark = $derived(done ? "" : sevMark(task.severity));
  const detail = $derived.by(() => {
    if (done) return st.text;
    const parts: string[] = [];
    if (st.kind === "overdue") parts.push(st.text);
    else if (when) parts.push(when + (st.kind === "soon" ? ` · ${st.text.replace("Starts in", "in")}` : st.kind === "progress" ? " · now" : ""));
    if (steps.length) parts.push(`${steps.filter((s) => s.done).length}/${steps.length}`);
    return parts.join(" · ");
  });
</script>

<div class="relative flex items-start gap-3 pl-4 {selected ? 'bg-accent-soft' : ''}">
  <button type="button" onclick={() => toggle(task)} aria-label="{done ? 'Mark not done' : 'Mark done'}: {task.title}" class="-ml-2 flex h-11 w-11 shrink-0 items-center justify-center">
    {#if done}
      <span class="flex h-[22px] w-[22px] items-center justify-center rounded-full bg-accent text-on-accent"><Icon name="check" size={13} stroke={3.2} /></span>
    {:else}
      <span class="h-[22px] w-[22px] rounded-full border-[1.5px] border-faint transition-transform duration-100 active:scale-90"></span>
    {/if}
  </button>
  <button type="button" onclick={() => onopen(task)} class="hl -ml-1 flex min-w-0 flex-1 flex-col gap-0.5 py-[11px] pr-4 text-left {last ? '' : 'border-b border-sep'}">
    <span class="flex items-baseline gap-1.5 text-[17px] leading-snug {done ? 'text-muted' : ''}">
      {#if mark}<span class="font-bold text-ink2">{mark}</span>{/if}
      <span class={desktop ? "truncate" : "line-clamp-2"}>{task.title}</span>
      {#if (task.photos?.length || (task.reminder_min != null && task.start)) && !done}<span class="ml-auto flex shrink-0 gap-1.5 self-center text-muted">
        {#if task.photos?.length}<Icon name="camera" size={14} stroke={2} />{/if}
        {#if task.reminder_min != null && task.start}<Icon name="bell" size={14} stroke={2} />{/if}</span>{/if}
    </span>
    {#if task.notes && !done && !desktop}<span class="truncate text-[15px] text-muted">{plain(task.notes)}</span>{/if}
    {#if detail || task.tags?.length}<span class="text-[15px] {st.kind === 'overdue' && !done ? 'text-danger' : 'text-muted'}">{detail}{#if task.tags?.length}<span class="text-muted">{detail ? " · " : ""}{task.tags.map((g) => "#" + g).join(" ")}</span>{/if}</span>{/if}
  </button>
</div>
