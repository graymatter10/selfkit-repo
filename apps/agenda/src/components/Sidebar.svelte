<script lang="ts">
  // Desktop sidebar (macOS-style): smart filters with counts, then lists.
  import { Icon } from "@selfkit/ui";
  import { app, counts, type Filter } from "../lib/state.svelte";
  let { onsettings }: { onsettings: () => void } = $props();
  const c = $derived(counts());
  const doneCount = $derived(app.tasks.filter((t) => t.done_at).length);
  const items = $derived<{ id: Filter; name: string; icon: string; n: number }[]>([
    { id: "active", name: "Active", icon: "list", n: c.active },
    { id: "overdue", name: "Overdue", icon: "clock", n: c.overdue },
    { id: "all", name: "All", icon: "views", n: app.tasks.length },
    { id: "done", name: "Completed", icon: "check", n: doneCount },
  ]);
</script>

<nav aria-label="Filters" class="flex w-[220px] shrink-0 flex-col gap-0.5 border-r border-sep bg-sidebar px-2.5 pt-5 pb-3">
  <span class="px-2.5 pb-1.5 text-[11px] font-semibold tracking-wide text-muted uppercase">Agenda</span>
  {#each items as it}
    {@const on = app.filter === it.id && !app.searching}
    <button type="button" aria-current={on ? "page" : undefined} onclick={() => ((app.filter = it.id), (app.searching = false))}
      class="flex h-8 items-center gap-2.5 rounded-md px-2.5 text-left text-[14px] {on ? 'bg-fill font-medium' : 'hover:bg-fill2'}">
      <span class={on ? "text-accent" : "text-muted"}><Icon name={it.icon} size={16} stroke={2} /></span>
      <span class="flex-1">{it.name}</span>
      <span class="text-[13px] tabular-nums {it.id === 'overdue' && it.n ? 'text-danger' : 'text-muted'}">{it.n || ""}</span>
    </button>
  {/each}
  {#if app.lists.length > 1}
    <span class="px-2.5 pt-5 pb-1.5 text-[11px] font-semibold tracking-wide text-muted uppercase">Lists</span>
    {#each app.lists as l}
      <div class="flex h-8 items-center gap-2.5 px-2.5 text-[14px] text-ink2"><span class="h-2 w-2 rounded-full bg-faint"></span>{l.name}
        <span class="ml-auto text-[13px] text-muted tabular-nums">{app.tasks.filter((t) => !t.done_at && (t.list ?? "inbox") === l.id).length || ""}</span></div>
    {/each}
  {/if}
  <button type="button" class="mt-auto flex h-8 items-center gap-2.5 rounded-md px-2.5 text-[14px] text-muted hover:bg-fill2" onclick={onsettings}><Icon name="gear" size={16} stroke={2} />Settings</button>
</nav>
