<script lang="ts">
  // "Your day": one 24-hour strip per source (asleep, screens, spending, tasks),
  // a now line in the accent, the rest of the day hatched.
  import type { Has, Ribbon } from "../lib/api";
  let { r, has }: { r: Ribbon; has: Has } = $props();
  const span = $derived(r.to - r.from);
  const pc = (t: number) => `${Math.min(100, Math.max(0, ((t - r.from) / span) * 100))}%`;
  const w = (a: number, b: number) => `${Math.max(0.4, ((Math.min(b, r.to) - Math.max(a, r.from)) / span) * 100)}%`;
  const lanes = $derived(
    [
      has.dusk && { k: "sleep", label: "Asleep" },
      has.chrono && { k: "screen", label: "Screens" },
      has.hisab && { k: "money", label: "Spending" },
      has.agenda && { k: "tasks", label: "Tasks" },
    ].filter(Boolean) as { k: string; label: string }[],
  );
  const hours = $derived([0, 6, 12, 18, 24].map((h) => String((h + r.first_hour) % 24 || (h === 24 ? 24 : 0)).padStart(2, "0")));
</script>

<div class="mt-3.5 flex flex-col gap-[9px]" role="img" aria-label="Your day on a 24-hour line">
  {#each lanes as l}
    <div class="flex items-center gap-2.5">
      <span class="w-[62px] shrink-0 text-[12px] font-[550] text-muted">{l.label}</span>
      <div class="relative h-3.5 flex-1 rounded-[4px] bg-fill2">
        {#if l.k === "sleep"}{#each r.sleep as [a, b]}<b class="absolute inset-y-0 rounded-[3px] bg-bar" style="left:{pc(a)};width:{w(a, b)}"></b>{/each}
        {:else if l.k === "screen"}{#each r.screen as [a, b]}<b class="absolute inset-y-0 rounded-[3px]" style="left:{pc(a)};width:{w(a, b)};background:var(--lane)"></b>{/each}
        {:else}{#each l.k === "money" ? r.money : r.tasks as t}<u class="absolute top-[3px] -ml-1 h-2 w-2 rounded-full bg-ink2" style="left:{pc(t)}"></u>{/each}{/if}
        {#if r.now < r.to}
          <span class="absolute inset-y-0 right-0 rounded-r-[4px]" style="left:{pc(r.now)};background:repeating-linear-gradient(135deg,transparent 0 4px,var(--fill) 4px 6px)"></span>
          <span class="absolute -top-1 -bottom-1 -ml-px w-0.5 rounded-[1px] bg-accent" style="left:{pc(r.now)}"></span>
        {/if}
      </div>
    </div>
  {/each}
  <div class="num ml-[72px] flex justify-between text-[12px] text-muted">{#each hours as h}<span>{h}</span>{/each}</div>
</div>
