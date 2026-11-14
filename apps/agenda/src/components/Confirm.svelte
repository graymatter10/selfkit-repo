<script lang="ts">
  // iOS-style action sheet.
  let { title, action, onconfirm, oncancel }: { title: string; action: string; onconfirm: () => void; oncancel: () => void } = $props();
</script>

<div class="fixed inset-0 z-[70] flex items-end justify-center p-2 pb-[max(8px,env(safe-area-inset-bottom))] sm:items-center">
  <button type="button" aria-label="Cancel" class="fade absolute inset-0 bg-scrim" onclick={oncancel}></button>
  <div role="alertdialog" aria-label={title} class="rise relative flex w-full max-w-[400px] flex-col gap-2">
    <div class="overflow-hidden rounded-[14px] bg-card text-center">
      <p class="m-0 border-b border-sep px-4 py-3.5 text-[13px] text-muted">{title}</p>
      <button type="button" class="hl h-14 w-full text-[20px] text-danger" onclick={onconfirm}>{action}</button>
    </div>
    <button type="button" class="hl h-14 w-full rounded-[14px] bg-card text-[20px] font-semibold text-accent" onclick={oncancel}>Keep Editing</button>
  </div>
</div>
