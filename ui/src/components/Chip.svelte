<script lang="ts">
  import type { Snippet } from "svelte";
  import Icon from "./Icon.svelte";
  type Tone = "neutral" | "accent" | "danger" | "warn" | "outline";
  let {
    tone = "outline",
    icon,
    onclick,
    onremove,
    label,
    children,
  }: { tone?: Tone; icon?: string; onclick?: () => void; onremove?: () => void; label?: string; children: Snippet } = $props();
  const cls: Record<Tone, string> = {
    neutral: "bg-key text-ink border-transparent",
    accent: "bg-accent-soft text-accent-text border-transparent",
    danger: "bg-danger-soft text-danger-text border-transparent",
    warn: "bg-warn-soft text-warn border-transparent",
    outline: "bg-surface text-ink border-line",
  };
</script>

<button
  type="button"
  class="flex h-9 cursor-pointer items-center gap-1.5 rounded-pill border px-3 text-sm font-medium whitespace-nowrap {cls[tone]}"
  aria-label={label}
  onclick={onremove ?? onclick}>
  {#if icon}<Icon name={icon} size={15} stroke={2} />{/if}
  {@render children()}
  {#if onremove}<Icon name="close" size={14} stroke={2} />{/if}
</button>
