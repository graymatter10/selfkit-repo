<script lang="ts">
  // Calendar heatmap of daily spending for one year (weeks × weekdays).
  import { iso, money } from "../lib/fmt";
  let { year, data, cur }: { year: number; data: { date: string; amount: number }[]; cur: string } = $props();
  const map = $derived(new Map(data.map((d) => [d.date, d.amount])));
  const cells = $derived.by(() => {
    const out: { x: number; y: number; d: string; v: number }[] = [];
    const first = new Date(year, 0, 1);
    const offset = (first.getDay() + 6) % 7; // Monday first
    const d = new Date(first);
    while (d.getFullYear() === year) {
      const i = Math.round((d.getTime() - first.getTime()) / 86400000) + offset;
      const k = iso(d);
      out.push({ x: Math.floor(i / 7), y: i % 7, d: k, v: map.get(k) ?? 0 });
      d.setDate(d.getDate() + 1);
    }
    return out;
  });
  // Quartile buckets of days with spending.
  const qs = $derived.by(() => {
    const v = data.map((d) => d.amount).filter((x) => x > 0).sort((a, b) => a - b);
    const q = (p: number) => v[Math.floor(p * (v.length - 1))] ?? 0;
    return [q(0.25), q(0.5), q(0.75)];
  });
  const op = (v: number) => (v <= 0 ? 0 : v <= qs[0] ? 0.2 : v <= qs[1] ? 0.42 : v <= qs[2] ? 0.68 : 1);
  const S = 6.4, G = 1.1;
  const today = iso(new Date());
</script>

<svg width="100%" viewBox="0 0 {53 * (S + G)} {7 * (S + G) + 12}" role="img" aria-label="Spending calendar {year}">
  {#each ["J", "F", "M", "A", "M", "J", "J", "A", "S", "O", "N", "D"] as m, i}
    <text x={((new Date(year, i, 1).getTime() - new Date(year, 0, 1).getTime()) / 86400000 / 7) * (S + G)} y={7 * (S + G) + 10} font-size="7" fill="var(--muted)">{m}</text>
  {/each}
  {#each cells as c (c.d)}
    <rect x={c.x * (S + G)} y={c.y * (S + G)} width={S} height={S} rx="1.4" fill={c.v > 0 ? "var(--accent)" : "var(--fill)"} fill-opacity={c.v > 0 ? op(c.v) : 1}
      stroke={c.d === today ? "var(--ink)" : "none"} stroke-width="0.8"><title>{c.d}: {money(c.v, cur)}</title></rect>
  {/each}
</svg>
