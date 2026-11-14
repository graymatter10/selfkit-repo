<script lang="ts">
  // Big number pad: digits, decimal point, backspace (hold = clear).
  import Ic from "./Ic.svelte";
  let { onkey, compact = false }: { onkey: (k: string) => void; compact?: boolean } = $props();
  const KEYS = ["1", "2", "3", "4", "5", "6", "7", "8", "9", ".", "0", "back"];
  let hold: ReturnType<typeof setTimeout> | undefined;
  let held = false;
  function down(k: string) {
    held = false;
    if (k === "back") hold = setTimeout(() => ((held = true), onkey("clear")), 550);
  }
  function click(k: string) {
    clearTimeout(hold);
    if (held) return void (held = false);
    onkey(k);
  }
</script>

<div class="grid grid-cols-3 gap-[5px]" role="group" aria-label="Number pad">
  {#each KEYS as k}
    <button type="button" aria-label={k === "back" ? "Delete" : k === "." ? "Decimal point" : k}
      class="key num flex items-center justify-center rounded-[12px] {compact ? 'h-11 text-[21px]' : 'h-[46px] text-[23px] lg:h-[52px]'} font-medium select-none"
      onpointerdown={() => down(k)} onclick={() => click(k)} onpointerleave={() => clearTimeout(hold)} oncontextmenu={(e) => e.preventDefault()}>
      {#if k === "back"}<Ic name="back" size={24} stroke={2} />{:else}{k}{/if}
    </button>
  {/each}
</div>
