<script lang="ts">
  import type { ProvenanceHop } from "@p2p-art-marketplace/client";
  import { useAppState } from "$lib/appState.svelte";
  import { shortHash } from "$lib/format";

  let { hops }: { hops: ProvenanceHop[] } = $props();

  const app = useAppState();
  const who = (k: Uint8Array) => (app.isMe(k) ? "you" : shortHash(k));
</script>

{#if hops.length === 0}
  <p class="muted small">No transfers yet — held by the minter.</p>
{:else}
  <ol class="timeline">
    {#each hops as hop (hop.sequence)}
      <li>
        <span class="pill">#{hop.sequence}</span>
        <span class="mono">{who(hop.from_agent)}</span>
        <span class="faint">→</span>
        <span class="mono">{who(hop.to_agent)}</span>
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
    gap: 0.55rem;
  }
  .timeline li {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    position: relative;
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
