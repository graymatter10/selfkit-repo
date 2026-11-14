<script lang="ts">
  // "● Chrono": the only place app colours appear (tiny marker of where data came from).
  import type { AppKey } from "../lib/api";
  import { APP_NAMES } from "../lib/fmt";
  let { apps }: { apps: AppKey[] } = $props();
</script>

<span class="inline-flex shrink-0 gap-2.5">
  {#each apps as a}<span class="inline-flex items-center gap-[5px] text-[12px] font-medium whitespace-nowrap text-muted"><i class="inline-block h-1.5 w-1.5 rounded-full" style="background:var(--{a})"></i>{APP_NAMES[a]}</span>{/each}
</span>
