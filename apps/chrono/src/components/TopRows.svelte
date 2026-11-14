<script lang="ts">
  // Most used apps and sites with a category-coloured bar and a label beside it.
  import type { DeviceTotal, Total } from "../lib/api";
  import { catColor, catName, dur } from "../lib/fmt";
  let { items, devices = [], phone }: { items: Total[]; devices?: DeviceTotal[]; phone: boolean } = $props();
  const max = $derived(Math.max(1, ...items.map((t) => t.ms)));
  function where(t: Total): string {
    if (t.kind === "w") return "Browser";
    const d = devices.find((d) => t.platforms.some((p) => d.platforms.includes(p)));
    return d?.name ?? "";
  }
</script>

{#each items as t (t.kind + t.name)}
  <div class="row">
    <div style="flex:1;min-width:0">
      <div style="display:flex;align-items:baseline;gap:8px;min-width:0"><span class="nm" style="overflow:hidden;text-overflow:ellipsis;white-space:nowrap">{t.name}</span><span class="sb" style="margin:0;white-space:nowrap">{where(t)}</span>{#if t.uninstalled}<span class="tag">Uninstalled</span>{/if}</div>
      <div style="display:flex;align-items:center;gap:10px;margin-top:7px">
        <div class="bar" style="flex:1;max-width:{phone ? 999 : 380}px;height:5px"><i style="width:{(t.ms / max) * 100}%;background:{catColor(t.cat)}"></i></div>
        <span class="muted" style="font-size:12.5px;width:{phone ? 92 : 96}px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">{catName(t.cat)}</span>
      </div>
    </div>
    <span class="val" style="width:62px;text-align:right;align-self:flex-start;margin-top:15px">{dur(t.ms)}</span>
  </div>
{/each}
