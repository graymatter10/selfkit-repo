<script lang="ts">
  // Bottom action sheet: "Discard changes?" (Back, Cancel, Escape with unsaved input).
  let { title, action, onconfirm, oncancel }: { title: string; action: string; onconfirm: () => void; oncancel: () => void } = $props();
</script>

<div class="fixed inset-0 z-[70] flex items-end justify-center p-3 pb-[max(12px,env(safe-area-inset-bottom))] sm:items-center">
  <button type="button" aria-label="Keep editing" class="fade absolute inset-0 bg-scrim" onclick={oncancel}></button>
  <div role="alertdialog" aria-label={title} class="rise relative flex w-full max-w-[380px] flex-col gap-2">
    <div class="card overflow-hidden text-center">
      <p class="m-0 border-b border-sep px-4 py-3.5 text-[14px] text-muted">{title}</p>
      <button type="button" class="hl h-14 w-full text-[17px] font-medium text-danger" onclick={onconfirm}>{action}</button>
    </div>
    <button type="button" class="card hl h-14 w-full text-[17px] font-semibold text-accent" onclick={oncancel}>Keep editing</button>
  </div>
</div>
