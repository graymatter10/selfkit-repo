<script lang="ts">
  // One transaction in a list.
  import type { Tx } from "../lib/api";
  import { money } from "../lib/fmt";
  import { accountById, catById, main } from "../lib/state.svelte";
  import CatBadge from "./CatBadge.svelte";
  import Ic from "./Ic.svelte";
  let { t, onopen, last = false }: { t: Tx; onopen: (t: Tx) => void; last?: boolean } = $props();
  const cat = $derived(catById(t.category));
  const split = $derived(t.splits?.length > 1);
  const title = $derived(
    t.kind === "transfer" ? `${accountById(t.account)?.name ?? "?"} → ${accountById(t.to_account ?? "")?.name ?? "?"}` : t.note || (split ? t.splits.map((s) => catById(s.category)?.name ?? "?").join(" + ") : cat?.name ?? "Uncategorized"),
  );
  const sub = $derived(
    t.kind === "transfer" ? (t.note || "Transfer") : [split ? "Split" : t.note ? cat?.name : "", accountById(t.account)?.name, t.tags.length ? t.tags.map((x) => "#" + x).join(" ") : ""].filter(Boolean).join(" · "),
  );
</script>

<button type="button" onclick={() => onopen(t)} class="hl flex min-h-[60px] w-full items-center gap-3 px-3.5 text-left">
  {#if t.kind === "transfer"}<CatBadge icon="repeat" color="#8a8a8e" />{:else}<CatBadge icon={cat?.icon ?? "dot"} color={cat?.color ?? "#8a8a8e"} />{/if}
  <span class="flex min-w-0 flex-1 flex-col py-2.5 {last ? '' : 'border-b border-sep'} -mr-3.5 pr-3.5">
    <span class="flex items-baseline gap-2">
      <span class="min-w-0 flex-1 truncate text-[15px] font-medium">{title}</span>
      <span class="num shrink-0 text-[15px] font-semibold {t.kind === 'income' ? 'text-accent-ink' : t.kind === 'transfer' ? 'text-muted' : ''}">{t.kind === "income" ? "+" : t.kind === "expense" ? "−" : ""}{money(t.amount, t.currency)}</span>
    </span>
    <span class="flex items-center gap-1.5 text-[13px] text-muted">
      <span class="min-w-0 flex-1 truncate">{sub}</span>
      {#if t.photos?.length}<Ic name="camera" size={13} />{/if}
      {#if t.source === "recurring"}<Ic name="repeat" size={13} />{/if}
      {#if t.currency !== main() && t.amount_main !== undefined}<span class="num shrink-0">{money(t.amount_main, main())}</span>{/if}
    </span>
  </span>
</button>
