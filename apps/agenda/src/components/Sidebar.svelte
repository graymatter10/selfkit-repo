<script lang="ts">
  // Desktop sidebar (macOS-style): list filters with counts, the other views,
  // lists, tags (click to filter every view), then History, Stats, Settings.
  import { Icon } from "@selfkit/ui";
  import { allTags, app, counts, type Filter, type View } from "../lib/state.svelte";
  let { onsettings, onview }: { onsettings: () => void; onview: (v: View) => void } = $props();
  const c = $derived(counts());
  const doneCount = $derived(app.tasks.filter((t) => t.done_at).length);
  const items = $derived<{ id: Filter; name: string; icon: string; n: number }[]>([
    { id: "active", name: "Active", icon: "list", n: c.active },
    { id: "overdue", name: "Overdue", icon: "clock", n: c.overdue },
    { id: "all", name: "All", icon: "views", n: app.tasks.length },
    { id: "done", name: "Completed", icon: "check", n: doneCount },
  ]);
  const views: { id: View; name: string; icon: string; key: string }[] = [
    { id: "upcoming", name: "Upcoming", icon: "sun", key: "2" },
    { id: "calendar", name: "Calendar", icon: "calendar", key: "3" },
    { id: "board", name: "Board", icon: "board", key: "4" },
  ];
  const tags = $derived(allTags());
  const cls = (on: boolean) => `flex h-8 items-center gap-2.5 rounded-md px-2.5 text-left text-[14px] ${on ? "bg-fill font-medium" : "hover:bg-fill2"}`;
</script>

<nav aria-label="Filters" class="flex w-[220px] shrink-0 flex-col gap-0.5 overflow-y-auto border-r border-sep bg-sidebar px-2.5 pt-5 pb-3">
  <span class="px-2.5 pb-1.5 text-[11px] font-semibold tracking-wide text-muted uppercase">Agenda</span>
  {#each items as it}
    {@const on = app.view === "list" && app.filter === it.id && !app.searching}
    <button type="button" aria-current={on ? "page" : undefined} onclick={() => ((app.filter = it.id), onview("list"))} class={cls(on)}>
      <span class={on ? "text-accent" : "text-muted"}><Icon name={it.icon} size={16} stroke={2} /></span>
      <span class="flex-1">{it.name}</span>
      <span class="text-[13px] tabular-nums {it.id === 'overdue' && it.n ? 'text-danger' : 'text-muted'}">{it.n || ""}</span>
    </button>
  {/each}
  <span class="px-2.5 pt-5 pb-1.5 text-[11px] font-semibold tracking-wide text-muted uppercase">Views</span>
  {#each views as v}
    {@const on = app.view === v.id && !app.searching}
    <button type="button" aria-current={on ? "page" : undefined} title="{v.name} ({v.key})" onclick={() => onview(v.id)} class={cls(on)}>
      <span class={on ? "text-accent" : "text-muted"}><Icon name={v.icon} size={16} stroke={2} /></span><span class="flex-1">{v.name}</span>
    </button>
  {/each}
  {#if app.lists.length > 1}
    <span class="px-2.5 pt-5 pb-1.5 text-[11px] font-semibold tracking-wide text-muted uppercase">Lists</span>
    {#each app.lists as l}
      <div class="flex h-8 items-center gap-2.5 px-2.5 text-[14px] text-ink2"><span class="h-2 w-2 rounded-full bg-faint"></span>{l.name}
        <span class="ml-auto text-[13px] text-muted tabular-nums">{app.tasks.filter((t) => !t.done_at && (t.list ?? "inbox") === l.id).length || ""}</span></div>
    {/each}
  {/if}
  {#if tags.length}
    <span class="px-2.5 pt-5 pb-1.5 text-[11px] font-semibold tracking-wide text-muted uppercase">Tags</span>
    {#each tags as g}
      {@const on = app.tag === g}
      <button type="button" aria-pressed={on} onclick={() => (app.tag = on ? null : g)} class={cls(on)}>
        <span class={on ? "text-accent" : "text-muted"}><Icon name="hash" size={15} stroke={2} /></span><span class="flex-1 truncate">{g}</span>
        <span class="text-[13px] text-muted tabular-nums">{app.tasks.filter((t) => !t.done_at && t.tags?.includes(g)).length || ""}</span>
      </button>
    {/each}
  {/if}
  <div class="mt-auto flex flex-col gap-0.5 pt-5">
    <button type="button" class={cls(app.view === "history")} title="History (5)" onclick={() => onview("history")}><span class="text-muted"><Icon name="check" size={16} stroke={2} /></span>History</button>
    <button type="button" class={cls(app.view === "stats")} title="Stats (6)" onclick={() => onview("stats")}><span class="text-muted"><Icon name="views" size={16} stroke={2} /></span>Stats</button>
    <button type="button" class="flex h-8 items-center gap-2.5 rounded-md px-2.5 text-[14px] text-muted hover:bg-fill2" onclick={onsettings}><Icon name="gear" size={16} stroke={2} />Settings</button>
  </div>
</nav>
