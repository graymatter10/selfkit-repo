<script lang="ts">
  // Goals and limits in plain words: "26 min left", "4 min to go"; red only when a limit is over.
  import type { GoalStatus } from "../lib/api";
  import { dur, goalName } from "../lib/fmt";
  let { goals, past }: { goals: GoalStatus[]; past: boolean } = $props();
</script>

{#each goals as gs (gs.goal.id)}
  {@const g = gs.goal}
  {@const lim = g.minutes * 60000}
  {@const left = lim - gs.used}
  <div class="gr" style="padding:12px 0;border-top:1px solid var(--line2)">
    <div style="display:flex;align-items:baseline;gap:8px;margin-bottom:9px">
      <span style="font-size:15px">{goalName(g)}</span>
      <span class="say num" style="margin:0 0 0 auto;font-size:14.5px">
        {#if g.kind === "limit"}
          {#if left >= 0}{#if past}Kept · <b>{dur(left)}</b> under{:else}<b>{dur(left)}</b> left{/if}
          {:else}<b style="color:var(--danger)">{dur(-left)} over</b>{/if}
        {:else if left > 0}<b>{dur(left)}</b> {past ? "short" : "to go"}
        {:else}<b>{past ? "Met" : "Done"}</b>{/if}
      </span>
    </div>
    <div class="bar"><i style="width:{Math.min(100, (gs.used / lim) * 100)}%;background:{g.kind === 'goal' ? 'var(--goal)' : left < 0 ? 'var(--danger)' : 'var(--accent)'}"></i></div>
    <div class="muted num" style="font-size:12.5px;margin-top:7px">{g.kind === "limit" ? "Limit" : "Goal"} {dur(lim)} a day · {dur(gs.used)} used</div>
  </div>
{/each}
