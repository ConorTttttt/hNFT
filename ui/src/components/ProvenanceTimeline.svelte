<script lang="ts">
  import type { ProvenanceHop } from "@p2p-art-marketplace/client";
  import { useAppState } from "$lib/appState.svelte";
  import { shortHash } from "$lib/format";

  let { hops }: { hops: ProvenanceHop[] } = $props();

  const app = useAppState();
  const who = (k: Uint8Array) => (app.isMe(k) ? "you" : shortHash(k));
</script>

{#if hops.length === 0}
  <p class="muted">No transfers yet — held by the minter.</p>
{:else}
  <ol class="stack" style="padding-left: 0; list-style: none; margin: 0">
    {#each hops as hop (hop.sequence)}
      <li class="row">
        <span class="tag">#{hop.sequence}</span>
        <span class="mono">{who(hop.from_agent)}</span>
        <span class="muted">→</span>
        <span class="mono">{who(hop.to_agent)}</span>
      </li>
    {/each}
  </ol>
{/if}
