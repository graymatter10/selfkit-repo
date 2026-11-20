<script lang="ts">
  // Month summary card: spent, remaining budget, balance, progress.
  import { money, monthName } from "../lib/fmt";
  import { app, go, main } from "../lib/state.svelte";
  let { big = false, compact = false }: { big?: boolean; compact?: boolean } = $props();
  const o = $derived(app.snap?.overview);
  // Against the budget only spending in budgeted categories counts (L2-12).
  const bspent = $derived(o ? (o.budget_spent ?? o.spent) : 0);
  const pct = $derived(o && o.budget > 0 ? Math.min(1, bspent / o.budget) : 0);
  const over = $derived(!!o && o.budget > 0 && bspent > o.budget);
  // Where spending "should" be today if the budget were spread evenly.
  const pace = $derived(o && o.days_in_month ? o.days_elapsed / o.days_in_month : 0);
</script>

{#if o && compact}
  <button type="button" onclick={() => go("insights")} class="card press block w-full px-4 pt-3 pb-3 text-left" aria-label="Open insights">
    <!-- Month spent on the left, balance / budget left stacked on the right (never wraps mid-figure). -->
    <div class="flex items-end gap-3">
      <div class="min-w-0 flex-1">
        <div class="text-[12.5px] font-medium text-muted">{monthName(o.month)} · spent</div>
        <div class="num truncate {money(o.spent, main()).length > 12 ? 'text-[20px]' : 'text-[24px]'} leading-tight font-semibold tracking-[-0.6px]">{money(o.spent, main())}</div>
      </div>
      <div class="min-w-0 shrink text-right">
        {#if o.budget > 0}
          <div class="text-[12.5px] text-muted">{over ? "Over budget" : "Left"}</div>
          <div class="num truncate text-[15px] font-semibold {over ? 'text-danger' : 'text-ink'}">{money(over ? -(o.remaining ?? 0) : (o.remaining ?? 0), main())}</div>
        {:else}
          <div class="text-[12.5px] text-muted">Balance</div>
          <div class="num truncate text-[15px] font-semibold {o.balance < 0 ? 'text-danger' : 'text-ink'}">{money(o.balance, main())}</div>
        {/if}
      </div>
    </div>
    {#if o.budget > 0}
      <div class="relative mt-2 h-[6px] overflow-hidden rounded-full bg-fill">
        <div class="h-full rounded-full {over ? 'bg-danger' : 'bg-accent'}" style="width:{pct * 100}%"></div>
        {#if pace > 0 && pace < 1}<div class="absolute top-0 h-full w-[2px] bg-ink/25" style="left:{pace * 100}%"></div>{/if}
      </div>
    {/if}
  </button>
{:else if o}
  <button type="button" onclick={() => go("insights")} class="card press block w-full px-4 pt-3.5 pb-4 text-left" aria-label="Open insights">
    <div class="flex items-baseline justify-between">
      <span class="text-[13px] font-medium text-muted">{monthName(o.month)} · spent</span>
      <span class="text-[13px] text-muted">Balance <span class="num font-semibold {o.balance < 0 ? 'text-danger' : 'text-ink'}">{money(o.balance, main())}</span></span>
    </div>
    <div class="num mt-0.5 {big ? 'text-[38px]' : 'text-[30px]'} leading-tight font-semibold tracking-[-0.8px]">{money(o.spent, main())}</div>
    {#if o.budget > 0}
      <div class="relative mt-2.5 h-[7px] overflow-hidden rounded-full bg-fill">
        <div class="h-full rounded-full transition-[width] duration-300 {over ? 'bg-danger' : 'bg-accent'}" style="width:{pct * 100}%"></div>
        {#if pace > 0 && pace < 1}<div class="absolute top-0 h-full w-[2px] bg-ink/25" style="left:{pace * 100}%" title="Even pace for today"></div>{/if}
      </div>
      <div class="mt-1.5 flex justify-between text-[13px] text-muted">
        <span class={over ? "font-medium text-danger" : ""}>{over ? `${money(-(o.remaining ?? 0), main())} over budget` : `${money(o.remaining ?? 0, main())} left`}</span>
        <span>of {money(o.budget, main())}</span>
      </div>
    {:else}
      <div class="mt-1 text-[13px] text-muted">Income <span class="num font-medium text-ink2">{money(o.income, main())}</span> · no budgets set</div>
    {/if}
  </button>
{/if}
