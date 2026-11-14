<script lang="ts">
  // Ask before throwing away typed input (Back / Cancel / Escape).
  let { title, action = "Discard", onconfirm, oncancel }: { title: string; action?: string; onconfirm: () => void; oncancel: () => void } = $props();
</script>

<div class="fixed inset-0 z-[80] flex items-end justify-center p-2 pb-[max(8px,env(safe-area-inset-bottom))] sm:items-center">
  <button type="button" aria-label="Keep editing" class="fade absolute inset-0 bg-scrim" onclick={oncancel}></button>
  <div role="alertdialog" aria-label={title} class="rise relative flex w-full max-w-[400px] flex-col gap-2">
    <div class="overflow-hidden rounded-[16px] bg-card text-center">
      <p class="m-0 border-b border-sep px-4 py-3.5 text-[14px] text-muted">{title}</p>
      <button type="button" class="hl h-14 w-full text-[18px] font-medium text-danger" onclick={onconfirm}>{action}</button>
    </div>
    <button type="button" class="hl h-14 w-full rounded-[16px] bg-card text-[18px] font-semibold text-accent" onclick={oncancel}>Keep editing</button>
  </div>
</div>
