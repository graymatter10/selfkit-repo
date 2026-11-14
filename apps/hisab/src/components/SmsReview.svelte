<script lang="ts">
  // Review queue for bank / wallet SMS: nothing is saved until you tap Save.
  import { showToast } from "@selfkit/ui";
  import { api, type SmsItem, type Tx } from "../lib/api";
  import { iso, money } from "../lib/fmt";
  import { isAndroid, requestSms, smsAllowed } from "../lib/native";
  import { accounts, app, defaultAccount, go, rankedCats, refresh, setPrefs } from "../lib/state.svelte";
  import { syncSms } from "../lib/sms";
  import CatBadge from "./CatBadge.svelte";
  let { desktop = false }: { desktop?: boolean } = $props();
  let items = $state<SmsItem[]>([]);
  let picks = $state<Record<string, { category: string; account: string; note: string }>>({});
  $effect(() => {
    void app.rev;
    api.smsPending().then((v) => {
      items = v;
      for (const s of v) {
        if (!picks[s.id]) {
          const hint = (s.parsed.hint || s.sender).toLowerCase();
          const acct = accounts().find((a) => hint.includes(a.name.toLowerCase()) || a.name.toLowerCase().includes(s.sender.toLowerCase()))?.id ?? defaultAccount();
          picks[s.id] = { category: "", account: acct, note: s.parsed.merchant };
        }
      }
    });
  });
  async function save(s: SmsItem) {
    const p = picks[s.id];
    if (!p.category) return showToast("Pick a category");
    const d = new Date(s.at);
    try {
      const t = await api.save<Tx>("transactions", { kind: s.parsed.kind, amount: s.parsed.amount, currency: s.parsed.currency || undefined, date: iso(d), time: `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`, account: p.account, category: p.category, note: p.note, source: "sms" });
      await api.save("sms", { ...s, status: "saved", tx: t.id }, s.id);
      await refresh();
      showToast(`Saved · ${money(t.amount, t.currency)}`, async () => {
        await api.remove(t.id);
        await api.save("sms", { ...s, status: "pending" }, s.id);
        await refresh();
      });
    } catch (e) {
      showToast(String(e));
    }
  }
  async function dismiss(s: SmsItem) {
    await api.save("sms", { ...s, status: "dismissed" }, s.id);
    await refresh();
    showToast("Dismissed", async () => {
      await api.save("sms", { ...s, status: "pending" }, s.id);
      await refresh();
    });
  }
  async function enable() {
    requestSms();
    // The permission dialog is async; check again when we're back.
    const t = setInterval(async () => {
      if (smsAllowed()) {
        clearInterval(t);
        await setPrefs({ sms: true, sms_since: Date.now() - 30 * 86400000 });
        await syncSms();
      }
    }, 700);
    setTimeout(() => clearInterval(t), 60000);
  }
</script>

<div class="flex flex-col gap-3 {desktop ? 'max-w-[680px]' : ''}">
  {#if !isAndroid()}
    <p class="m-0 rounded-[14px] bg-fill2 px-4 py-3 text-[14px] text-ink2">SMS import runs on your Android phone. Entries it finds show up here after sync.</p>
  {:else if !app.prefs.sms || !smsAllowed()}
    <div class="card flex flex-col gap-3 p-4">
      <b class="text-[16px]">Import from bank and wallet SMS</b>
      <p class="m-0 text-[14px] leading-snug text-ink2">Hisab reads SMS on this phone to find amounts from your bank or mobile wallet (bKash, Nagad, card alerts). It never sends them anywhere, ignores other messages, and nothing is saved until you check it here.</p>
      <button type="button" class="press h-12 rounded-[14px] bg-accent text-[16px] font-semibold text-on-accent" onclick={enable}>Allow SMS access</button>
    </div>
  {:else}
    <div class="flex items-center justify-between px-1 text-[13px] text-muted"><span>Watching new SMS · last 30 days scanned</span><button type="button" class="font-semibold text-accent" onclick={() => syncSms(true)}>Scan again</button></div>
  {/if}
  {#each items as s (s.id)}
    {@const p = picks[s.id]}
    <div class="card fade flex flex-col gap-3 p-4">
      <div class="flex items-baseline justify-between gap-2">
        <span class="num text-[22px] font-semibold {s.parsed.kind === 'income' ? 'text-accent-ink' : ''}">{s.parsed.kind === "income" ? "+" : "−"}{money(s.parsed.amount, s.parsed.currency || "USD")}</span>
        <span class="text-[12.5px] text-muted">{s.sender} · {new Date(s.at).toLocaleString(undefined, { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" })}</span>
      </div>
      <p class="m-0 line-clamp-3 rounded-[10px] bg-fill2 px-3 py-2 text-[13px] text-ink2">{s.body}</p>
      {#if p}
        <div class="flex gap-1 overflow-x-auto fade-r">
          {#each rankedCats(s.parsed.kind) as c (c.id)}
            <button type="button" aria-pressed={p.category === c.id} class="press flex shrink-0 items-center gap-1.5 rounded-full py-1 pr-3 pl-1 text-[13px] {p.category === c.id ? 'bg-accent-soft font-semibold' : 'bg-fill2'}" onclick={() => (p.category = c.id)}><CatBadge icon={c.icon} color={c.color} size={24} />{c.name}</button>
          {/each}
        </div>
        <div class="flex gap-2">
          <input bind:value={p.note} placeholder="Note" aria-label="Note" class="field h-10 flex-1 text-[14px]" />
          <select bind:value={p.account} aria-label="Account" class="field h-10 w-[38%] appearance-none text-[14px]">{#each accounts() as a (a.id)}<option value={a.id}>{a.name}</option>{/each}</select>
        </div>
        <div class="flex gap-2">
          <button type="button" class="press h-11 flex-1 rounded-[12px] bg-fill text-[15px] font-medium" onclick={() => dismiss(s)}>Not a transaction</button>
          <button type="button" class="press h-11 flex-1 rounded-[12px] bg-accent text-[15px] font-semibold text-on-accent" onclick={() => save(s)}>Save</button>
        </div>
      {/if}
    </div>
  {:else}
    {#if app.prefs.sms}<p class="m-0 py-8 text-center text-[15px] text-muted">Nothing to review.</p>{/if}
  {/each}
  {#if isAndroid() && app.prefs.sms}<button type="button" class="text-[13px] text-muted underline" onclick={() => (setPrefs({ sms: false }), go("home"))}>Turn SMS import off</button>{/if}
</div>
