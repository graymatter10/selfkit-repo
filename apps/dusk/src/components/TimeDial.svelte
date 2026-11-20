<script lang="ts">
  // Clock-dial time picker: a big readout (tap the hour or the minutes to pick
  // which one you set) over a round face. Pick the hour first (tap or drag;
  // 24h: 1–12 outside, 13–00 on the inner ring; 12h: one ring + am/pm), then it
  // moves on to minutes by itself. Minutes snap to the labelled 5-minute marks
  // unless you aim between them (1-minute steps). Keys: ←↓/↑→ −/+1 (Shift: 5
  // minutes), Tab hour → minutes; Enter/Esc are handled by TimeSheet.
  let { value = $bindable("23:00"), h12 = false }: { value: string; h12?: boolean } = $props();

  let stage: "h" | "m" = $state("h");
  let dragging = $state(false);
  let face: SVGSVGElement | undefined = $state();
  let elH: HTMLButtonElement | undefined = $state();
  let elM: HTMLButtonElement | undefined = $state();
  let advance: ReturnType<typeof setTimeout> | undefined;

  const h = $derived(Number(value.slice(0, 2)) || 0);
  const m = $derived(Number(value.slice(3, 5)) || 0);
  const pm = $derived(h >= 12);
  const set = (hh: number, mm: number) => (value = `${String(((hh % 24) + 24) % 24).padStart(2, "0")}:${String(((mm % 60) + 60) % 60).padStart(2, "0")}`);

  const R_OUT = 98, R_IN = 64;
  const POS = Array.from({ length: 12 }, (_, i) => i);
  const at = (i: number, r: number) => {
    const a = (i * 30 * Math.PI) / 180;
    return { x: Math.sin(a) * r, y: -Math.cos(a) * r };
  };
  const outerHour = (i: number) => (i === 0 ? 12 : i);
  const innerHour = (i: number) => (i === 0 ? 0 : i + 12);
  const inner = $derived(stage === "h" && !h12 && (h === 0 || h > 12));
  const target = $derived(stage === "h" ? (h % 12) * 30 : m * 6);
  const len = $derived(inner ? R_IN : R_OUT);
  const onMark = $derived(stage === "h" || m % 5 === 0);

  // Turn the hand the short way round (keeps a running angle, no 330°→0° spin).
  let rot = $state(0);
  $effect(() => {
    const t = target;
    const cur = ((rot % 360) + 360) % 360;
    rot += ((t - cur + 540) % 360) - 180;
  });

  function pick(e: PointerEvent) {
    if (!face) return;
    const r = face.getBoundingClientRect();
    const k = 260 / r.width;
    const x = (e.clientX - r.left - r.width / 2) * k, y = (e.clientY - r.top - r.height / 2) * k;
    const ang = ((Math.atan2(x, -y) * 180) / Math.PI + 360) % 360;
    const dist = Math.hypot(x, y);
    if (stage === "h") {
      const i = Math.round(ang / 30) % 12;
      const hh = h12 ? i + (pm ? 12 : 0) : dist < (R_IN + R_OUT) / 2 ? innerHour(i) : outerHour(i);
      set(hh, m);
    } else {
      const f = ang / 6, n5 = Math.round(f / 5) * 5;
      set(h, Math.abs(f - n5) < 1 ? n5 : Math.round(f));
    }
  }
  function down(e: PointerEvent) {
    e.preventDefault();
    clearTimeout(advance);
    face?.setPointerCapture(e.pointerId);
    dragging = true;
    pick(e);
  }
  function move(e: PointerEvent) {
    if (dragging) pick(e);
  }
  function up() {
    if (!dragging) return;
    dragging = false;
    if (stage === "h") advance = setTimeout(() => setStage("m"), 260);
  }
  function setStage(s: "h" | "m", focus = false) {
    clearTimeout(advance);
    stage = s;
    if (focus) (s === "h" ? elH : elM)?.focus();
  }
  export function focus() {
    (stage === "h" ? elH : elM)?.focus();
  }

  function keys(e: KeyboardEvent) {
    const d = { ArrowUp: 1, ArrowRight: 1, ArrowDown: -1, ArrowLeft: -1, PageUp: 5, PageDown: -5 }[e.key];
    if (d) {
      e.preventDefault();
      const step = e.shiftKey && stage === "m" ? d * 5 : d;
      if (stage === "h") set(h + step, m);
      else {
        // Minutes carry into the hour like the hour keys carry into am/pm (10:59 → 11:00, L4-5).
        const t = (((h * 60 + m + step) % 1440) + 1440) % 1440;
        set(Math.floor(t / 60), t % 60);
      }
    } else if (/^[0-9]$/.test(e.key) && stage === "m") {
      // quick minute typing on desktop: two digits
      e.preventDefault();
      const s = (String(m).padStart(2, "0") + e.key).slice(-2);
      set(h, Math.min(59, Number(s)));
    }
  }
  const hourText = $derived(h12 ? String(h % 12 || 12) : String(h).padStart(2, "0"));
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="flex flex-col items-center" onkeydown={keys}>
  <div class="flex items-center gap-1.5">
    <button type="button" bind:this={elH} onclick={() => setStage("h")} onfocus={() => setStage("h")} aria-label="Hour {hourText}, arrow keys change it"
      class="num h-[72px] min-w-[86px] rounded-[18px] px-2 text-[50px] leading-none font-[650] tracking-[-0.03em] transition-colors duration-150 {stage === 'h' ? 'bg-accent-soft text-accent' : 'bg-fill2 text-ink'}">{hourText}</button>
    <span class="num -mt-1 text-[44px] font-[600] text-faint">:</span>
    <button type="button" bind:this={elM} onclick={() => setStage("m")} onfocus={() => setStage("m")} aria-label="Minutes {String(m).padStart(2, '0')}, arrow keys change them"
      class="num h-[72px] min-w-[86px] rounded-[18px] px-2 text-[50px] leading-none font-[650] tracking-[-0.03em] transition-colors duration-150 {stage === 'm' ? 'bg-accent-soft text-accent' : 'bg-fill2 text-ink'}">{String(m).padStart(2, "0")}</button>
    {#if h12}
      <div class="ml-2 flex flex-col gap-1" role="radiogroup" aria-label="am or pm">
        {#each [["am", false], ["pm", true]] as [label, isPm]}
          <button type="button" role="radio" aria-checked={pm === isPm} onclick={() => pm !== isPm && set(h + (isPm ? 12 : -12), m)}
            class="h-[34px] w-12 rounded-[10px] text-[14px] font-semibold transition-colors duration-150 {pm === isPm ? 'bg-accent-soft text-accent' : 'text-muted'}">{label}</button>
        {/each}
      </div>
    {/if}
  </div>

  <svg bind:this={face} viewBox="-130 -130 260 260" class="mt-5 aspect-square w-full max-w-[272px] touch-none select-none" role="slider" tabindex="-1"
    aria-label={stage === "h" ? "Hour" : "Minutes"} aria-valuenow={stage === "h" ? h : m} aria-valuemin={0} aria-valuemax={stage === "h" ? 23 : 59}
    onpointerdown={down} onpointermove={move} onpointerup={up} onpointercancel={up}>
    <circle r="126" fill="var(--fill2)" />
    {#if stage === "m"}
      {#each Array.from({ length: 60 }, (_, i) => i) as i}
        {#if i % 5}
          {@const p = { x: Math.sin((i * 6 * Math.PI) / 180), y: -Math.cos((i * 6 * Math.PI) / 180) }}
          <circle cx={p.x * 120} cy={p.y * 120} r="1.1" fill="var(--faint)" />
        {/if}
      {/each}
    {/if}
    <g class="hand" style="transform: rotate({rot}deg); transition: transform {dragging ? 70 : 220}ms cubic-bezier(0.3, 0.8, 0.3, 1)">
      <line x1="0" y1="0" x2="0" y2={-len + 19} stroke="var(--accent)" stroke-width="2.5" stroke-linecap="round" style="transition: all 160ms" />
      <circle cx="0" cy={-len} r="19" fill="var(--accent)" style="transition: all 160ms" />
      {#if !onMark}<circle cx="0" cy={-len} r="3" fill="var(--on-accent)" />{/if}
    </g>
    <circle r="4" fill="var(--accent)" />
    {#if stage === "h"}
      {#each POS as i}
        {@const o = at(i, R_OUT)}
        {@const v = h12 ? i + (pm ? 12 : 0) : outerHour(i)}
        <text x={o.x} y={o.y} dy="0.35em" text-anchor="middle" class="num" font-size="16" font-weight="560"
          fill={v === h ? "var(--on-accent)" : "var(--ink)"} style="transition: fill 120ms">{outerHour(i)}</text>
        {#if !h12}
          {@const n = at(i, R_IN)}
          <text x={n.x} y={n.y} dy="0.35em" text-anchor="middle" class="num" font-size="13" font-weight="520"
            fill={innerHour(i) === h ? "var(--on-accent)" : "var(--muted)"} style="transition: fill 120ms">{i === 0 ? "00" : innerHour(i)}</text>
        {/if}
      {/each}
    {:else}
      {#each POS as i}
        {@const o = at(i, R_OUT)}
        <text x={o.x} y={o.y} dy="0.35em" text-anchor="middle" class="num" font-size="16" font-weight="560"
          fill={i * 5 === m ? "var(--on-accent)" : "var(--ink)"} style="transition: fill 120ms">{String(i * 5).padStart(2, "0")}</text>
      {/each}
    {/if}
  </svg>
</div>

<style>
  .hand { transform-origin: 0 0; }
  @media (prefers-reduced-motion: reduce) { .hand, .hand * { transition: none !important; } }
</style>
