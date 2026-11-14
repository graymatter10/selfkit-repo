<script lang="ts">
  import { runUndo, toast } from "@selfkit/ui";
</script>

{#if toast.current}
  {#key toast.current.id}
    <div role="status" class="toast rise">
      <span style="flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">{toast.current.text}</span>
      {#if toast.current.undo}<button type="button" onclick={runUndo}>Undo</button>{:else}<span style="width:8px"></span>{/if}
    </div>
  {/key}
{/if}
