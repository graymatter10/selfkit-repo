<script lang="ts">
  // Loan detail (status, remaining, repayments one by one, proof photos,
  // reminder) and the new/edit form.
  import OptionalDate from "./OptionalDate.svelte";
  import { showToast } from "@selfkit/ui";
  import { api, type Loan } from "../lib/api";
  import { daysBetween, money, shortDate, todayIso } from "../lib/fmt";
  import { isAndroid } from "../lib/native";
  import { addPhoto } from "../lib/photo";
  import { app, closeSheet, guardForm, main, openSheet, refresh, removeRec, saveRec, setPrefs } from "../lib/state.svelte";
  import Confirm from "./Confirm.svelte";
  import Ic from "./Ic.svelte";
  import Photo from "./Photo.svelte";
  import SheetFrame from "./SheetFrame.svelte";
  import Seg from "./Seg.svelte";

  let { id, edit = false, desktop = false }: { id?: string; edit?: boolean; desktop?: boolean } = $props();
  const loan = $derived(id ? app.snap?.loans.find((l) => l.id === id) : undefined);
  let editing = $state(edit || !id);

  // ---- form
  const o = id ? app.snap?.loans.find((l) => l.id === id) : undefined;
  let f = $state({
    direction: o?.direction ?? "lent", person: o?.person ?? "", title: o?.title ?? "", description: o?.description ?? "", amount: o ? String(o.amount) : "",
    currency: o?.currency ?? main(), given: o?.given ?? todayIso(), due: o?.due ?? "", account: o?.account ?? "", photos: [...(o?.photos ?? [])],
    remind: o?.remind ?? true, remind_days: String(o?.remind_days ?? 1), remind_time: o?.remind_time ?? "09:00",
  });
  let start = JSON.stringify(f);
  let asking = $state(false);
  $effect(() => guardForm(() => editing && JSON.stringify(f) !== start, () => (asking = true)));
  function leave() {
    if (editing && JSON.stringify(f) !== start) asking = true;
    else if (editing && id) editing = false;
    else closeSheet();
  }

  /** False when Android refused notifications (the reminder can't show — P11). */
  async function askNotify(): Promise<boolean> {
    if (!app.prefs.notify) await setPrefs({ notify: true });
    if (!isAndroid()) return true;
    try {
      const n = await import("@tauri-apps/plugin-notification");
      if (!(await n.isPermissionGranted())) return (await n.requestPermission()) === "granted";
    } catch {}
    return true;
  }

  async function saveForm() {
    const data = { ...(o ?? { repayments: [] }), ...f, amount: Number(f.amount) || 0, remind_days: Number(f.remind_days) || 0, repayments: loan?.repayments ?? [] };
    const canNotify = f.remind && f.due ? await askNotify() : true;
    const r = await saveRec<Loan>("loans", data, id);
    if (r) {
      start = JSON.stringify(f);
      app.guard = null;
      if (id) editing = false;
      else closeSheet();
      showToast((id ? "Loan saved" : `Loan added · ${f.person}`) + (canNotify ? "" : " · reminders need notifications, which Android blocks for Hisab"), undefined, canNotify ? 5000 : 8000);
    }
  }

  // ---- repayments
  let rep = $state({ amount: "", date: todayIso(), note: "" });
  async function addRepayment() {
    if (!loan) return;
    const a = Number(rep.amount);
    if (!(a > 0)) return showToast("Type the amount paid back");
    if (a > loan.remaining + 0.005) return showToast(`Only ${money(loan.remaining, loan.currency)} is left`);
    const before = { ...loan };
    const r = await saveRec<Loan>("loans", { ...loan, repayments: [...loan.repayments, { id: "", ...rep, amount: a }] }, loan.id);
    if (r) {
      rep = { amount: "", date: todayIso(), note: "" };
      showToast(`Repayment saved · ${money(a, loan.currency)}`, async () => {
        await api.save("loans", before, before.id);
        await refresh();
      });
    }
  }
  async function delRepayment(rid: string) {
    if (!loan) return;
    const before = { ...loan };
    await saveRec("loans", { ...loan, repayments: loan.repayments.filter((r) => r.id !== rid) }, loan.id);
    showToast("Repayment removed", async () => {
      await api.save("loans", before, before.id);
      await refresh();
    });
  }
  async function proof() {
    try {
      const p = await addPhoto();
      if (!p) return;
      if (editing) f.photos = [...f.photos, p];
      else if (loan) await saveRec("loans", { ...loan, photos: [...loan.photos, p] }, loan.id);
    } catch (e) {
      showToast(String(e));
    }
  }
  async function del() {
    if (!loan) return;
    app.guard = null;
    closeSheet();
    await removeRec(loan.id, `Loan with ${loan.person} deleted`);
  }

  const STATUS: Record<string, [string, string]> = {
    overdue: ["Overdue", "bg-danger-soft text-danger"], due_soon: ["Due soon", "bg-fill text-ink2"], ongoing: ["Ongoing", "bg-accent-soft text-accent-ink"], finished: ["Finished", "bg-fill text-muted"],
  };
  const dueText = (l: Loan) => {
    if (!l.due) return "No due date";
    const d = daysBetween(todayIso(), l.due);
    if (l.status === "finished") return `Was due ${shortDate(l.due)}`;
    return d < 0 ? `${-d} day${d === -1 ? "" : "s"} overdue · ${shortDate(l.due)}` : d === 0 ? "Due today" : `Due in ${d} day${d === 1 ? "" : "s"} · ${shortDate(l.due)}`;
  };
  const row = "flex min-h-12 items-center gap-3 border-b border-sep px-4 last:border-0";
  const lbl = "w-28 shrink-0 text-[15px] text-ink2";
  const inp = "min-w-0 flex-1 bg-transparent py-3 text-right text-[15px] outline-none";
</script>

{#if editing}
  <SheetFrame title={id ? "Edit loan" : "New loan"} {desktop} left="Cancel" right={id ? "Save" : "Add"} onleft={leave} onright={saveForm} rightDisabled={!f.person.trim() || !(Number(f.amount) > 0)}>
    <div class="flex flex-col gap-4 pt-1">
      <Seg label="Direction" items={[{ id: "lent", name: "I lent" }, { id: "borrowed", name: "I borrowed" }]} value={f.direction} onchange={(v) => (f.direction = v)} />
      <div class="card overflow-hidden">
        <label class={row}><span class={lbl}>{f.direction === "lent" ? "To" : "From"}</span><input bind:value={f.person} placeholder="Name" class={inp} /></label>
        <label class={row}><span class={lbl}>Amount</span><input inputmode="decimal" bind:value={f.amount} placeholder="0" class="{inp} num font-semibold" /></label>
        <div class={row}><span class={lbl}>Currency</span><button type="button" class="press ml-auto text-[15px] font-medium text-accent" onclick={() => openSheet({ kind: "currency", value: f.currency, onpick: (c) => (f.currency = c) })}>{f.currency}</button></div>
        <label class={row}><span class={lbl}>Title</span><input bind:value={f.title} placeholder="What for" class={inp} /></label>
        <label class="flex items-start gap-3 px-4"><span class="{lbl} pt-3">Description</span><textarea bind:value={f.description} rows="2" class="min-w-0 flex-1 resize-none bg-transparent py-3 text-right text-[15px] outline-none"></textarea></label>
      </div>
      <div class="card overflow-hidden">
        <label class={row}><span class={lbl}>Date given</span><input type="date" bind:value={f.given} class={inp} /></label>
        <label class={row}><span class={lbl}>Due date</span><OptionalDate bind:value={f.due} label="Due date" cls={inp} /></label>
        <label class={row}><span class={lbl}>Account</span>
          <select bind:value={f.account} class="{inp} appearance-none"><option value="">Not linked</option>{#each app.snap?.accounts ?? [] as a (a.id)}<option value={a.id}>{a.name}</option>{/each}</select></label>
      </div>
      <p class="m-0 -mt-2 px-1 text-[12px] text-muted">A linked account changes its balance when the money leaves or comes back.</p>
      {#if f.due}
        <div class="card overflow-hidden">
          <label class={row}><span class="flex-1 text-[15px] text-ink2">Remind me</span><input type="checkbox" bind:checked={f.remind} class="h-5 w-5 accent-[var(--accent)]" /></label>
          {#if f.remind}
            <label class={row}><span class={lbl}>Days before</span>
              <select bind:value={f.remind_days} class="{inp} appearance-none">{#each [0, 1, 2, 3, 5, 7, 14] as d}<option value={String(d)}>{d === 0 ? "On the due date" : `${d} day${d > 1 ? "s" : ""} before`}</option>{/each}</select></label>
            <label class={row}><span class={lbl}>At</span><input type="time" bind:value={f.remind_time} class={inp} /></label>
          {/if}
        </div>
      {/if}
      <div>
        <div class="flex items-center justify-between px-1 pb-1.5"><span class="text-[13px] font-semibold text-muted">Proof photos</span><button type="button" class="press flex items-center gap-1 text-[14px] font-medium text-accent" onclick={proof}><Ic name="camera" size={16} />Add</button></div>
        {#if f.photos.length}<div class="flex flex-wrap gap-2">{#each f.photos as p (p)}<div class="relative"><Photo id={p} class="h-20 w-20 rounded-[12px] object-cover" /><button type="button" aria-label="Remove photo" class="absolute -top-1.5 -right-1.5 flex h-6 w-6 items-center justify-center rounded-full bg-ink text-bg" onclick={() => (f.photos = f.photos.filter((x) => x !== p))}><Ic name="close" size={13} stroke={2.4} /></button></div>{/each}</div>{/if}
      </div>
    </div>
  </SheetFrame>
{:else if loan}
  <SheetFrame title={loan.direction === "lent" ? "Money lent" : "Money borrowed"} {desktop} left="Close" right="Edit" onleft={closeSheet} onright={() => ((f = { ...f, ...loan, amount: String(loan.amount), remind_days: String(loan.remind_days), photos: [...loan.photos] }), (start = JSON.stringify(f)), (editing = true))}>
    <div class="flex flex-col gap-4 pt-1">
      <div class="card p-4">
        <div class="flex items-center gap-3">
          <span class="flex h-11 w-11 items-center justify-center rounded-full bg-fill text-[17px] font-semibold text-ink2">{loan.person.slice(0, 1).toUpperCase()}</span>
          <div class="min-w-0 flex-1"><div class="truncate text-[17px] font-semibold">{loan.person}</div><div class="truncate text-[13px] text-muted">{loan.title || (loan.direction === "lent" ? "Owes you" : "You owe")}</div></div>
          <span class="rounded-full px-2.5 py-0.5 text-[12px] font-semibold {STATUS[loan.status][1]}">{STATUS[loan.status][0]}</span>
        </div>
        <div class="mt-4 flex items-baseline justify-between">
          <span class="num text-[28px] font-semibold tracking-[-0.6px]">{money(loan.remaining, loan.currency)}<span class="text-[14px] font-medium text-muted">&nbsp;left of {money(loan.amount, loan.currency)}</span></span>
        </div>
        <div class="mt-2 h-[7px] overflow-hidden rounded-full bg-fill"><div class="h-full rounded-full {loan.status === 'overdue' ? 'bg-danger' : 'bg-accent'}" style="width:{(loan.repaid / loan.amount) * 100}%"></div></div>
        <div class="mt-2 flex justify-between text-[13px] {loan.status === 'overdue' ? 'text-danger' : 'text-muted'}"><span>{dueText(loan)}</span><span class="text-muted">given {shortDate(loan.given)}</span></div>
        {#if loan.description}<p class="m-0 mt-3 text-[14px] text-ink2">{loan.description}</p>{/if}
        {#if loan.due && loan.remind && loan.status !== "finished"}<p class="m-0 mt-2 flex items-center gap-1.5 text-[13px] text-muted"><Ic name="bell" size={14} />Reminder {loan.remind_days ? `${loan.remind_days} day${loan.remind_days > 1 ? "s" : ""} before` : "on the due date"} at {loan.remind_time}</p>{/if}
      </div>

      {#if loan.status !== "finished"}
        <div class="card flex flex-col gap-2 p-3">
          <p class="m-0 px-1 text-[13px] font-semibold text-muted">{loan.direction === "lent" ? "They paid back" : "I paid back"}</p>
          <div class="grid grid-cols-2 gap-2">
            <input inputmode="decimal" bind:value={rep.amount} placeholder="Amount" aria-label="Repayment amount" class="field num min-w-0 text-[16px] font-semibold" />
            <input type="date" bind:value={rep.date} aria-label="Repayment date" class="field min-w-0 text-[14px]" />
          </div>
          <div class="flex gap-2">
            <input bind:value={rep.note} placeholder="Note (optional)" aria-label="Repayment note" class="field min-w-0 flex-1 text-[14px]" />
            <button type="button" class="press h-11 shrink-0 rounded-[12px] bg-fill px-3 text-[13px] font-medium" onclick={() => (rep.amount = String(loan.remaining))}>All</button>
            <button type="button" class="press h-11 rounded-[12px] bg-accent px-4 text-[15px] font-semibold text-on-accent" onclick={addRepayment}>Add</button>
          </div>
        </div>
      {/if}

      <div>
        <p class="m-0 px-1 pb-1.5 text-[13px] font-semibold text-muted">Repayments · {loan.repayments.length}</p>
        {#if loan.repayments.length}
          <div class="card overflow-hidden">
            {#each [...loan.repayments].sort((a, b) => b.date.localeCompare(a.date)) as r (r.id)}
              <div class="flex min-h-12 items-center gap-3 border-b border-sep px-4 last:border-0">
                <span class="w-20 shrink-0 text-[14px] text-muted">{shortDate(r.date)}</span>
                <span class="min-w-0 flex-1 truncate text-[14px] text-ink2">{r.note}</span>
                <span class="num text-[15px] font-semibold text-accent-ink">{money(r.amount, loan.currency)}</span>
                <button type="button" aria-label="Remove repayment" class="press text-muted" onclick={() => delRepayment(r.id)}><Ic name="close" size={15} /></button>
              </div>
            {/each}
          </div>
        {:else}<p class="m-0 px-1 text-[13px] text-muted">Nothing paid back yet.</p>{/if}
      </div>

      <div>
        <div class="flex items-center justify-between px-1 pb-1.5"><span class="text-[13px] font-semibold text-muted">Proof photos</span><button type="button" class="press flex items-center gap-1 text-[14px] font-medium text-accent" onclick={proof}><Ic name="camera" size={16} />Add</button></div>
        {#if loan.photos.length}
          <div class="flex flex-wrap gap-2">{#each loan.photos as p (p)}<button type="button" aria-label="View proof photo" class="press overflow-hidden rounded-[12px]" onclick={() => openSheet({ kind: "photo", id: p })}><Photo id={p} class="h-24 w-24 object-cover" /></button>{/each}</div>
        {:else}<p class="m-0 px-1 text-[13px] text-muted">No proof yet. A photo of a note or chat helps later.</p>{/if}
      </div>
      <button type="button" class="press card h-12 w-full text-[16px] font-medium text-danger" onclick={del}>Delete loan</button>
    </div>
  </SheetFrame>
{/if}
{#if asking}<Confirm title="Discard your changes?" onconfirm={() => ((asking = false), (app.guard = null), id ? (editing = false) : closeSheet())} oncancel={() => (asking = false)} />{/if}
