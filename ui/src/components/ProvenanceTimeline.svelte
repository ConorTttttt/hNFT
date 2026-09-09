<script lang="ts">
  import type { ProvenanceHop } from "@p2p-art-marketplace/client";
  import AgentBadge from "./AgentBadge.svelte";

  let { hops }: { hops: ProvenanceHop[] } = $props();
</script>

{#if hops.length === 0}
  <p class="muted small">No transfers yet — held by the minter.</p>
{:else}
  <ol class="timeline">
    {#each hops as hop (hop.sequence)}
      <li>
        <span class="pill">#{hop.sequence}</span>
        <AgentBadge agent={hop.from_agent} size={16} />
        <span class="faint">→</span>
        <AgentBadge agent={hop.to_agent} size={16} />
      </li>
    {/each}
  </ol>
{/if}

<style>
  .timeline {
    list-style: none;
    margin: 0;
    padding: 0 0 0 0.75rem;
    border-left: 2px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .timeline li {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    position: relative;
    flex-wrap: wrap;
  }
  .timeline li::before {
    content: "";
    position: absolute;
    left: calc(-0.75rem - 5px);
    width: 8px;
    height: 8px;
    border-radius: 999px;
    background: var(--accent);
  }
</style>
