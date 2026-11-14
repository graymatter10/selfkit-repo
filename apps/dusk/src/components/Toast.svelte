<script lang="ts">
  import { runUndo, toast } from "@selfkit/ui";
</script>

{#if toast.current}
  {#key toast.current.id}
    <div role="status" class="rise pointer-events-auto flex min-h-12 items-center gap-3 rounded-[14px] bg-toast py-1 pr-1 pl-4 text-toast-ink shadow-lg">
      <span class="flex-1 truncate text-[15px]">{toast.current.text}</span>
      {#if toast.current.undo}<button type="button" class="press h-10 rounded-[10px] px-3.5 text-[15px] font-semibold text-[#c4b8ff]" onclick={runUndo}>Undo</button>{/if}
    </div>
  {/key}
{/if}
