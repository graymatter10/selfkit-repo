<script lang="ts">
  // A date that may be empty (loan due, goal deadline, recurring end, History range).
  // WebKitGTK draws an empty <input type="date"> as today's date, so an unset date looked
  // set (L2-10/C59): when empty the native text is hidden and `empty` ("None") shows
  // instead; a set date gets a clear button.
  import Ic from "./Ic.svelte";
  let { value = $bindable(""), empty = "None", label, cls = "", right = true }: { value?: string; empty?: string; label: string; cls?: string; right?: boolean } = $props();
</script>

<span class="relative flex min-w-0 flex-1 items-center">
  <input type="date" bind:value aria-label={label} class="{cls} {value ? '' : 'od-empty'}" />
  {#if !value}<span class="pointer-events-none absolute inset-y-0 flex items-center text-muted {right ? 'right-0' : 'left-2'}">{empty}</span>{/if}
  {#if value}<button type="button" aria-label="Clear {label}" class="press ml-2 shrink-0 text-muted" onclick={() => (value = "")}><Ic name="close" size={15} /></button>{/if}
</span>

<style>
  .od-empty,
  .od-empty::-webkit-datetime-edit {
    color: transparent;
  }
</style>
