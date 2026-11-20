<script lang="ts">
  // One Today tile: label + app marker, one big value, one plain line.
  // A missing app (or no data today) shows a dashed card that says which.
  // On phones `installed` means Reflect has a mirror of that app (SelfkitMirror).
  import type { AppKey, Tile } from "../lib/api";
  import { APP_NAMES } from "../lib/fmt";
  import Src from "./Src.svelte";
  let { label, app, tile, installed, mobile = false }: { label: string; app: AppKey; tile: Tile | null; installed: boolean; mobile?: boolean } = $props();
  const why: Record<AppKey, string> = {
    chrono: "Chrono tracks your screen time.",
    hisab: "Hisab keeps track of what you spend.",
    agenda: "Agenda keeps your tasks.",
    dusk: "Dusk estimates your sleep from screen use.",
  };
</script>

{#if tile}
  <div class="card flex min-h-[120px] flex-col px-4 pt-3.5 pb-[15px]">
    <!-- The label wraps rather than "Screen t…" next to the marker (P32). -->
    <div class="flex items-center gap-2 text-[13px] font-semibold text-ink2"><span class="min-w-0 flex-1 leading-tight">{label}</span><Src apps={[app]} /></div>
    <div class="num mt-2.5 text-[27px] leading-[1.1] font-[650] tracking-[-0.03em]">{tile.value}{#if tile.unit}<small class="ml-0.5 text-[15px] font-[550] tracking-[-0.01em] text-muted">{tile.unit}</small>{/if}</div>
    <div class="mt-1.5 text-[13px] leading-[1.35] text-muted">{#if tile.strong}<b class="font-semibold text-ink2">{tile.strong}</b>{" "}{/if}{tile.sub}</div>
  </div>
{:else}
  <div class="missing flex min-h-[120px] flex-col px-4 pt-3.5 pb-[15px]">
    <div class="text-[13px] font-semibold text-ink2">{label}</div>
    <div class="mt-3 text-[15px] font-semibold text-ink2">{installed ? `No ${APP_NAMES[app]} data yet` : `${APP_NAMES[app]} isn't installed`}</div>
    <div class="mt-1.5 text-[13px] leading-[1.35] text-muted">{installed ? "Nothing for today so far." : why[app]}</div>
  </div>
{/if}
