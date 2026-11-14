<script lang="ts">
  import type { AppRow } from "../lib/api";
  import { dur } from "../lib/fmt";
  import Icon from "./Icon.svelte";
  let { rows, suffix = "" }: { rows: AppRow[]; suffix?: string } = $props();
  const max = $derived(Math.max(1, ...rows.map((r) => r.min)));
</script>

<div class="mt-1.5">
  {#each rows as r}
    <div class="flex min-h-12 items-center gap-3">
      <span class="grid h-[34px] w-[34px] shrink-0 place-items-center rounded-[10px] bg-fill text-ink2"><Icon name={r.phone ? "phone" : "laptop"} size={17} /></span>
      <div class="min-w-0 flex-1">
        <div class="flex items-baseline gap-2"><span class="truncate text-[14.5px] font-[550]">{r.name}</span><span class="truncate text-[12.5px] text-muted">{r.device}</span><span class="num ml-auto shrink-0 text-[14.5px] font-semibold">{dur(r.min)}{suffix}</span></div>
        <div class="mt-1.5 h-1.5 overflow-hidden rounded-[3px] bg-fill2"><i class="block h-full rounded-[3px] bg-bar" style="width:{(r.min / max) * 100}%"></i></div>
      </div>
    </div>
  {/each}
</div>
