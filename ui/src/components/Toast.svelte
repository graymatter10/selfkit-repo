<script lang="ts">
  import Icon from "./Icon.svelte";
  import { toast, runUndo } from "../toast.svelte";
</script>

{#if toast.current}
  {#key toast.current.id}
    <div
      role="status"
      class="sk-toast pointer-events-auto flex min-h-13 items-center gap-3 rounded-card bg-toast py-1 pr-1.5 pl-4 text-toast-ink shadow-lg">
      <Icon name="check" size={18} stroke={2} />
      <span class="flex-1 truncate text-sm">{toast.current.text}</span>
      {#if toast.current.undo}
        <button type="button" class="h-11 cursor-pointer rounded-sm px-3.5 text-sm font-semibold text-toast-action" onclick={runUndo}
          >Undo</button>
      {/if}
    </div>
  {/key}
{/if}

<style>
  .sk-toast { animation: sk-rise 160ms ease-out; }
  @keyframes sk-rise { from { opacity: 0; transform: translateY(8px); } to { opacity: 1; transform: none; } }
</style>
