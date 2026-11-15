<script lang="ts">
  // The night on one line: screen use (grey), the sleep (violet) and, when you
  // changed the times, the device estimate as a thin line underneath.
  import type { Night } from "../lib/api";
  import { hmd } from "../lib/fmt";
  let { night }: { night: Night } = $props();

  const H = 3_600_000;
  const W = 1000;
  const range = $derived.by(() => {
    const [ws, we] = night.window;
    let a = ws + 2 * H, b = ws + 18 * H; // 20:00 → 12:00
    for (const t of [night.bed, night.estimate.bed]) if (t != null) a = Math.min(a, t - H / 2);
    for (const t of [night.wake, night.estimate.wake]) if (t != null) b = Math.max(b, t + H / 2);
    a = Math.max(ws, Math.floor((a - ws) / H) * H + ws);
    b = Math.min(we, Math.ceil((b - ws) / H) * H + ws);
    return [a, b] as const;
  });
  const x = (t: number) => ((Math.min(Math.max(t, range[0]), range[1]) - range[0]) / (range[1] - range[0])) * W;
  const ticks = $derived.by(() => {
    const out: { x: number; label: string }[] = [];
    // 21:00, 00:00, 03:00 … (the window starts at 18:00)
    for (let t = night.window[0] + 3 * H; t < range[1] - H / 2; t += 3 * H) if (t > range[0] + H / 2) out.push({ x: x(t), label: hmd(t, night.tz_offset).replace(":00 ", " ") });
    return out;
  });
  const asleep = $derived(night.bed != null && night.wake != null);
  const showEst = $derived(night.source === "logged" && night.estimate.status === "found" && night.estimate.bed != null);
  const label = $derived(
    asleep
      ? `Asleep ${hmd(night.bed, night.tz_offset)} to ${hmd(night.wake, night.tz_offset)}; screen on ${night.estimate.activity.length} times`
      : `Screen on ${night.estimate.activity.length} times; no sleep found`,
  );
</script>

<svg viewBox="0 0 {W} 52" class="block h-[52px] w-full" role="img" aria-label={label} preserveAspectRatio="none">
  {#each ticks as t}<line x1={t.x} x2={t.x} y1="0" y2="52" stroke="var(--sep)" stroke-width="1" vector-effect="non-scaling-stroke" />{/each}
  <rect x="0" y="14" width={W} height="18" rx="5" fill="var(--fill2)" />
  {#each night.estimate.activity as [a, b]}
    {#if b > range[0] && a < range[1]}<rect x={x(a)} y="14" width={Math.max(3, x(b) - x(a))} height="18" rx="2" fill="var(--act)" />{/if}
  {/each}
  {#if asleep}
    <rect x={x(night.bed!)} y="8" width={Math.max(4, x(night.wake!) - x(night.bed!))} height="30" rx="7" fill="var(--accent)" />
  {/if}
  {#if showEst}
    <rect x={x(night.estimate.bed!)} y="44" width={Math.max(4, x(night.estimate.wake!) - x(night.estimate.bed!))} height="4" rx="2" fill="var(--accent2)" />
  {/if}
</svg>
<div class="num relative mt-1 h-4 text-[11px] text-muted" aria-hidden="true">
  {#each ticks as t}<span class="absolute -translate-x-1/2" style="left:{(t.x / W) * 100}%">{t.label}</span>{/each}
</div>
<div class="legend mt-2">
  {#if asleep}<span><i style="background:var(--accent)"></i>Asleep</span>{/if}
  <span><i style="background:var(--act)"></i>Screen on</span>
  {#if showEst}<span><i style="background:var(--accent2);height:4px;vertical-align:2px"></i>Estimate</span>{/if}
</div>
