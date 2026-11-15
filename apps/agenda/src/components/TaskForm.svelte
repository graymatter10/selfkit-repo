<script lang="ts">
  // New / edit task sheet (iOS style). A real <form>: the keyboard's ✓ adds the
  // task after the word being typed is committed. Backing out of unsaved input
  // asks first instead of throwing it away.
  import { Icon } from "@selfkit/ui";
  import type { Step, Task } from "../lib/api";
  import { api } from "../lib/api";
  import { SEVERITY, fmtMin, fmtWhen, parseWhen, reminderHint, dayLabel, hm, shortDay } from "../lib/time";
  import { allTags, app, createTask, defaultReminder, defaultSeverity, saveEdit } from "../lib/state.svelte";
  import { attachPhoto } from "../lib/photos";
  import { askPhotoSource, cleanCaptures } from "../lib/photo-source";
  import Photo from "./Photo.svelte";
  import Confirm from "./Confirm.svelte";
  import Seg from "./Seg.svelte";
  import WhenSheet from "./WhenSheet.svelte";

  let { task, layout = "sheet", onclose, ondone, initialStart }: { task?: Task; layout?: "sheet" | "panel"; onclose: () => void; ondone?: (t: Task) => void; initialStart?: string } = $props();

  const editing = !!task;
  let title = $state(task?.title ?? "");
  let notes = $state(task?.notes ?? "");
  let severity = $state(task?.severity ?? defaultSeverity());
  let start = $state<string | undefined>(task ? task.start : initialStart);
  let stop = $state<string | undefined>(task?.stop);
  let reminder = $state<number | null>(task ? (task.reminder_min ?? null) : defaultReminder());
  let repeat = $state(task?.repeat ?? "");
  let list = $state(task?.list ?? "inbox");
  let steps = $state<Step[]>(task?.steps ? task.steps.map((s) => ({ ...s })) : []);
  let tags = $state<string[]>(task?.tags ? [...task.tags] : app.tag ? [app.tag] : []);
  let photos = $state<string[]>(task?.photos ? [...task.photos] : []);
  let newTag = $state("");
  let photoBusy = $state(false);
  let galleryEl: HTMLInputElement | undefined = $state();
  let cameraEl: HTMLInputElement | undefined = $state();
  let picker = $state<"start" | "stop" | null>(null);
  let open = $state<"" | "reminder" | "repeat" | "list">("");
  let newStep = $state("");
  let customMin = $state("");
  let error = $state("");
  let saving = $state(false);
  let confirm = $state(false);
  let titleEl: HTMLInputElement | undefined = $state();

  const snapshot = () => JSON.stringify([title.trim(), notes.trim(), severity, start, stop, reminder, repeat, list, steps, tags, photos, newTag.trim()]);
  const initial = snapshot();
  const dirty = $derived(snapshot() !== initial);
  const timed = $derived(!!start && !parseWhen(start)?.allDay);

  $effect(() => {
    if (!editing && !app.demo.title) setTimeout(() => titleEl?.focus(), layout === "sheet" ? 250 : 50);
    if (app.demo.title && !editing) {
      title = app.demo.title;
      start = app.demo.start;
    }
    if (app.demo.picker) setTimeout(() => (picker = app.demo.picker as "start"), 50);
  });
  // Lets the Android back button ask before discarding.
  $effect(() => {
    app.closeGuard = () => {
      if (picker) return (picker = null), true;
      if (dirty) return (confirm = true), true;
      return false;
    };
    return () => (app.closeGuard = null);
  });

  function cancel() {
    if (dirty) confirm = true;
    else onclose();
  }

  const REMINDERS: { m: number | null; name: string }[] = [
    { m: null, name: "None" }, { m: 0, name: "At start time" }, { m: 5, name: "5 minutes before" }, { m: 10, name: "10 minutes before" },
    { m: 15, name: "15 minutes before" }, { m: 30, name: "30 minutes before" }, { m: 60, name: "1 hour before" }, { m: 120, name: "2 hours before" },
    { m: 1440, name: "1 day before" },
  ];
  const REPEATS = [
    { id: "", name: "Never" }, { id: "daily", name: "Every day" }, { id: "weekdays", name: "Weekdays" },
    { id: "weekly", name: "Every week" }, { id: "monthly", name: "Every month" }, { id: "yearly", name: "Every year" },
  ];
  const reminderName = $derived(reminder == null ? "None" : reminder === 0 ? "At start" : `${fmtMin(reminder)} before`);

  function label(v?: string) {
    const w = parseWhen(v);
    if (!w) return "None";
    const d = dayLabel(w.date, app.now);
    const day = d === "Today" || d === "Tomorrow" || d === "Yesterday" ? d : shortDay(w.date);
    return w.allDay ? `${day}, all day` : `${day}, ${hm(w.date)}`;
  }
  /** Same day as the start → just the time. */
  function endLabel(v?: string) {
    const e = parseWhen(v), s = parseWhen(start);
    return e && s && e.date.toDateString() === s.date.toDateString() ? hm(e.date) : label(v);
  }
  function lasts(min: number) {
    const s = parseWhen(start);
    if (s && !s.allDay) stop = fmtWhen(new Date(s.date.getTime() + min * 60_000), false);
  }
  function setStart(v: string) {
    const old = parseWhen(start), next = parseWhen(v), e = parseWhen(stop);
    if (old && e && next && !next.allDay && !old.allDay) stop = fmtWhen(new Date(next.date.getTime() + (e.date.getTime() - old.date.getTime())), false);
    else if (next?.allDay) stop = undefined;
    start = v;
    picker = null;
  }
  function addStep() {
    const t = newStep.trim();
    if (!t) return;
    steps = [...steps, { id: crypto.randomUUID(), title: t, done: false }];
    newStep = "";
  }
  const cleanTag = (t: string) => t.trim().replace(/^#+/, "").toLowerCase().split(/\s+/).filter(Boolean).join("-").slice(0, 32);
  function addTag(raw = newTag) {
    const parts = raw.split(/[,\s]+/).map(cleanTag).filter(Boolean);
    for (const t of parts) if (!tags.includes(t)) tags = [...tags, t];
    newTag = "";
  }
  const suggestions = $derived(allTags().filter((t) => !tags.includes(t) && (!newTag.trim() || t.startsWith(cleanTag(newTag)))).slice(0, 8));

  async function onPhotos(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const files = [...(input.files ?? [])];
    input.value = "";
    if (!files.length) return;
    photoBusy = true;
    error = "";
    try {
      for (const f of files) photos = [...photos, await attachPhoto(f)];
    } catch (err) {
      error = `Could not add the photo: ${String(err).replace(/^Error:\s*/, "")}`;
    } finally {
      photoBusy = false;
      cleanCaptures(); // drop the temporary camera file (photo is stored by now)
    }
  }

  async function addList() {
    const name = prompt("New list name");
    if (!name?.trim()) return;
    list = await api.addList(name.trim());
    app.lists = await api.lists();
  }

  async function save(e?: SubmitEvent) {
    e?.preventDefault();
    if (saving) return; // double tap / held Enter
    const t = (titleEl?.value ?? title).trim();
    if (!t) {
      error = "Give the task a title.";
      titleEl?.focus();
      return;
    }
    saving = true;
    error = "";
    try {
      const data = {
        title: t, notes: notes.trim() || undefined, severity, start, stop: start ? stop : undefined,
        reminder_min: start ? (reminder ?? undefined) : undefined, repeat: repeat || undefined, list, steps,
        tags: (addTag(), tags), photos,
      };
      const done = editing && task
        ? await saveEdit(task.id, { ...data, notes: data.notes ?? null, start: start ?? null, stop: data.stop ?? null, reminder_min: data.reminder_min ?? null, repeat: data.repeat ?? null })
        : await createTask(data);
      app.closeGuard = null;
      ondone?.(done);
      onclose();
    } catch (err) {
      error = String(err).replace(/^Error:\s*/, "").replace(/^invalid format:\s*/i, "");
    } finally {
      saving = false;
    }
  }
</script>

{#snippet row(name: string, value: string, onclick: () => void, opts: { expanded?: boolean; disabled?: boolean; last?: boolean; accent?: boolean } = {})}
  <button type="button" {onclick} disabled={opts.disabled} aria-expanded={opts.expanded}
    class="hl flex min-h-11 w-full items-center gap-2 pr-3 pl-4 text-left text-[17px] disabled:opacity-40">
    <span class="flex flex-1 items-center gap-2 self-stretch py-[11px] {opts.last ? '' : 'border-b border-sep'}">
      <span class="flex-1">{name}</span>
      <span class={opts.accent ? "text-accent" : "text-muted"}>{value}</span>
      <span class="text-faint transition-transform {opts.expanded ? 'rotate-90' : ''}"><Icon name="chevron" size={16} stroke={2.4} /></span>
    </span>
  </button>
{/snippet}

<form onsubmit={save} class="relative flex min-h-0 flex-1 flex-col bg-bg" novalidate>
  <div class="grid grid-cols-[1fr_auto_1fr] items-center px-4 py-2 {layout === 'panel' ? 'border-b border-sep bg-card py-3' : ''}">
    <button type="button" class="press justify-self-start py-2 text-[17px] text-accent" onclick={cancel}>Cancel</button>
    <span class="text-[17px] font-semibold">{editing ? "Edit Task" : "New Task"}</span>
    <button type="submit" disabled={saving} class="press justify-self-end py-2 text-[17px] font-semibold {title.trim() ? 'text-accent' : 'text-faint'}">{editing ? "Done" : "Add"}</button>
  </div>

  <div class="flex min-h-0 flex-1 flex-col gap-[22px] overflow-y-auto *:shrink-0 px-4 pt-3 pb-16">
    <div>
      <div class="overflow-hidden rounded-xl bg-card">
        <label for="f-title" class="sr">Title</label>
        <input id="f-title" bind:this={titleEl} bind:value={title} placeholder="Title" autocomplete="off" enterkeyhint="done"
          class="h-12 w-full bg-transparent px-4 text-[17px] outline-none" />
        <div class="ml-4 border-t border-sep"></div>
        <label for="f-notes" class="sr">Notes</label>
        <textarea id="f-notes" bind:value={notes} placeholder="Notes · Markdown works" rows="3" class="block w-full resize-none bg-transparent px-4 py-3 text-[17px] outline-none"></textarea>
      </div>
      {#if error}<p role="alert" class="m-0 px-4 pt-2 text-[13px] text-danger">{error}</p>{/if}
    </div>

    <div class="flex flex-col gap-2">
      <span class="px-4 text-[13px] text-muted uppercase">Photos</span>
      <div class="overflow-hidden rounded-xl bg-card">
        {#if photos.length}
          <div class="grid grid-cols-4 gap-1 p-1">
            {#each photos as id, i (id)}
              <div class="relative aspect-square overflow-hidden rounded-lg bg-fill">
                <Photo {id} />
                <button type="button" aria-label="Remove photo {i + 1}" class="absolute top-1 right-1 flex h-7 w-7 items-center justify-center rounded-full bg-black/55 text-white"
                  onclick={() => ((photos = photos.filter((p) => p !== id)), (notes = notes.replace(new RegExp(`!\\[[^\\]]*\\]\\(photo:${id}\\)\\n?`, "g"), "")))}><Icon name="close" size={14} stroke={2.4} /></button>
              </div>
            {/each}
          </div>
        {/if}
        <div class="flex {photos.length ? 'border-t border-sep' : ''}">
          <button type="button" disabled={photoBusy} class="hl flex min-h-11 flex-1 items-center justify-center gap-2 text-[17px] text-accent disabled:opacity-50" onclick={() => askPhotoSource({ gallery: () => galleryEl?.click(), camera: () => cameraEl?.click() })}>
            <Icon name="camera" size={18} stroke={2} />{photoBusy ? "Adding…" : layout === "sheet" ? "Add Photo" : "Add Photo…"}</button>
        </div>
      </div>
      <input bind:this={galleryEl} type="file" accept="image/*" multiple class="sr" tabindex="-1" aria-hidden="true" onchange={onPhotos} />
      <input bind:this={cameraEl} type="file" accept="image/*" capture="environment" class="sr" tabindex="-1" aria-hidden="true" onchange={onPhotos} oncancel={cleanCaptures} />
    </div>

    <div class="flex flex-col gap-2">
      <span class="px-4 text-[13px] text-muted uppercase">Tags</span>
      <div class="flex flex-wrap items-center gap-1.5 rounded-xl bg-card px-3 py-2">
        {#each tags as g, i (g)}
          <span class="flex h-8 items-center gap-0.5 rounded-full bg-accent-soft pr-1 pl-3 text-[15px] text-accent">#{g}
            <button type="button" aria-label="Remove tag {g}" class="flex h-7 w-7 items-center justify-center" onclick={() => (tags = tags.filter((_, j) => j !== i))}><Icon name="close" size={13} stroke={2.4} /></button></span>
        {/each}
        <label class="min-w-[120px] flex-1"><span class="sr">Add tag</span>
          <input bind:value={newTag} placeholder={tags.length ? "Add tag" : "Add tag, e.g. home"} enterkeyhint="enter" autocomplete="off" autocapitalize="off"
            class="h-8 w-full bg-transparent px-1 text-[17px] outline-none"
            oninput={() => /[,\s]$/.test(newTag) && addTag()}
            onkeydown={(e) => { if (e.key === "Enter") { e.preventDefault(); addTag(); } else if (e.key === "Backspace" && !newTag && tags.length) tags = tags.slice(0, -1); }} onblur={() => addTag()} /></label>
      </div>
      {#if suggestions.length}
        <div class="flex flex-wrap gap-1.5 px-1">
          {#each suggestions as g}<button type="button" class="press h-7 rounded-full bg-fill px-3 text-[14px] text-ink2" onmousedown={(e) => e.preventDefault()} onclick={() => (tags = [...tags, g])}>#{g}</button>{/each}
        </div>
      {/if}
    </div>

    <div class="flex flex-col gap-2">
      <span class="px-4 text-[13px] text-muted uppercase">Severity</span>
      <Seg label="Severity" small items={[1, 2, 3, 4, 5].map((n) => ({ id: n, name: SEVERITY[n] }))} value={severity} onchange={(v) => (severity = v)} />
    </div>

    <div class="flex flex-col gap-2">
      <span class="px-4 text-[13px] text-muted uppercase">Schedule</span>
      <div class="overflow-hidden rounded-xl bg-card">
        {@render row("Starts", label(start), () => (picker = "start"), { accent: !!start })}
        {@render row("Ends", timed ? endLabel(stop) : start ? "—" : "None", () => (picker = "stop"), { disabled: !timed, accent: !!stop, last: !timed })}
        {#if timed}
          <div class="flex items-center gap-2 px-4 py-2.5">
            <span class="flex-1 text-[15px] text-muted">Duration</span>
            {#each [30, 60, 120] as m}<button type="button" class="press h-8 rounded-full bg-fill px-3 text-[15px]" onclick={() => lasts(m)}>{fmtMin(m)}</button>{/each}
          </div>
        {/if}
      </div>
      {#if start}<button type="button" class="press self-start px-4 text-[15px] text-accent" onclick={() => ((start = undefined), (stop = undefined))}>Remove date</button>{/if}
    </div>

    <div class="flex flex-col gap-2">
      <div class="overflow-hidden rounded-xl bg-card">
        {@render row("Reminder", start ? reminderName : "Needs a start", () => (open = open === "reminder" ? "" : "reminder"), { expanded: open === "reminder", disabled: !start, last: open !== "reminder" })}
        {#if open === "reminder" && start}
          {#each REMINDERS as r}
            <button type="button" role="menuitemradio" aria-checked={reminder === r.m} onclick={() => ((reminder = r.m), (open = ""))} class="hl flex w-full items-center pl-8 text-left text-[17px]">
              <span class="flex flex-1 items-center border-b border-sep py-[11px] pr-4"><span class="flex-1">{r.name}</span>{#if reminder === r.m}<span class="text-accent"><Icon name="check" size={18} stroke={2.6} /></span>{/if}</span>
            </button>
          {/each}
          <div class="flex items-center gap-2 py-2 pr-4 pl-8">
            <label class="flex flex-1 items-center gap-2 text-[15px] text-muted">Custom
              <input type="number" inputmode="numeric" min="1" bind:value={customMin} placeholder="min" class="h-9 w-20 rounded-lg bg-fill px-2 text-[17px] text-ink outline-none" /></label>
            <button type="button" class="press text-[15px] font-semibold text-accent" onclick={() => { const n = parseInt(customMin); if (n > 0) { reminder = n; open = ""; } }}>Set</button>
          </div>
        {/if}
      </div>
      <span class="px-4 text-[13px] text-muted">{reminderHint(start, reminder, app.now)}</span>
    </div>

    <div class="overflow-hidden rounded-xl bg-card">
      {@render row("Repeat", REPEATS.find((r) => r.id === repeat)?.name ?? "Never", () => (open = open === "repeat" ? "" : "repeat"), { expanded: open === "repeat" })}
      {#if open === "repeat"}
        {#each REPEATS as r}
          <button type="button" role="menuitemradio" aria-checked={repeat === r.id} onclick={() => ((repeat = r.id), (open = ""))} class="hl flex w-full items-center pl-8 text-left text-[17px]">
            <span class="flex flex-1 items-center border-b border-sep py-[11px] pr-4"><span class="flex-1">{r.name}</span>{#if repeat === r.id}<span class="text-accent"><Icon name="check" size={18} stroke={2.6} /></span>{/if}</span>
          </button>
        {/each}
      {/if}
      {@render row("List", app.lists.find((l) => l.id === list)?.name ?? "Personal", () => (open = open === "list" ? "" : "list"), { expanded: open === "list", last: open !== "list" })}
      {#if open === "list"}
        {#each app.lists as l}
          <button type="button" role="menuitemradio" aria-checked={list === l.id} onclick={() => ((list = l.id), (open = ""))} class="hl flex w-full items-center pl-8 text-left text-[17px]">
            <span class="flex flex-1 items-center border-b border-sep py-[11px] pr-4"><span class="flex-1">{l.name}</span>{#if list === l.id}<span class="text-accent"><Icon name="check" size={18} stroke={2.6} /></span>{/if}</span>
          </button>
        {/each}
        <button type="button" class="hl flex min-h-11 w-full items-center pl-8 text-left text-[17px] text-accent" onclick={addList}>New List…</button>
      {/if}
    </div>

    <div class="flex flex-col gap-2">
      <span class="px-4 text-[13px] text-muted uppercase">Steps</span>
      <div class="overflow-hidden rounded-xl bg-card">
        {#each steps as s, i (s.id)}
          <div class="flex items-center gap-3 pl-4">
            <span class="h-[18px] w-[18px] shrink-0 rounded-full border-[1.5px] border-faint"></span>
            <div class="flex flex-1 items-center border-b border-sep">
              <input bind:value={s.title} aria-label="Step {i + 1}" class="h-11 flex-1 bg-transparent text-[17px] outline-none" />
              <button type="button" aria-label="Remove step {s.title}" class="flex h-11 w-11 items-center justify-center text-muted" onclick={() => (steps = steps.filter((_, j) => j !== i))}><Icon name="close" size={16} stroke={2} /></button>
            </div>
          </div>
        {/each}
        <div class="flex items-center gap-3 pl-4">
          <span class="text-accent"><Icon name="plus" size={18} stroke={2.4} /></span>
          <label class="flex-1"><span class="sr">Add step</span>
            <input bind:value={newStep} placeholder="Add Step" enterkeyhint="enter" class="h-11 w-full bg-transparent pr-4 text-[17px] outline-none placeholder:text-accent"
              onkeydown={(e) => { if (e.key === "Enter") { e.preventDefault(); addStep(); } }} onblur={addStep} /></label>
        </div>
      </div>
    </div>
  </div>
</form>

{#if picker}
  <WhenSheet which={picker} value={picker === "start" ? start : stop} base={start} onclose={() => (picker = null)}
    onset={(v) => (picker === "start" ? setStart(v) : ((stop = v), (picker = null)))} />
{/if}
{#if confirm}
  <Confirm title={editing ? "Discard your changes to this task?" : "Discard this new task?"} action={editing ? "Discard Changes" : "Discard Task"}
    onconfirm={() => ((confirm = false), (app.closeGuard = null), onclose())} oncancel={() => (confirm = false)} />
{/if}
