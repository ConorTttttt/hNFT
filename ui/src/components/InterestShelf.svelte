<script lang="ts">
  import { useAppState, type InterestRow } from "$lib/appState.svelte";
  import { b64, shortHash } from "$lib/format";
  import AgentBadge from "./AgentBadge.svelte";

  const app = useAppState();

  let busy = $state<string | null>(null);

  async function offer(row: InterestRow) {
    const key = b64(row.nft) + b64(row.buyer);
    busy = key;
    try {
      await app.initiateTransfer(row.nft, row.buyer);
    } finally {
      busy = null;
    }
  }
</script>

{#if app.interestInMyListings.length}
  <div class="panel stack">
    <div class="row between">
      <h3>
        Interest in your listings <span class="faint">· {app.interestInMyListings.length}</span>
      </h3>
      <span class="faint small">send a transfer offer to whoever you'd like to sell to</span>
    </div>
    <div class="stack tight">
      {#each app.interestInMyListings as row (b64(row.nft) + b64(row.buyer))}
        {@const key = b64(row.nft) + b64(row.buyer)}
        <div class="row between wrap row-item">
          <div class="row" style="gap: 0.5rem; align-items: center">
            <span class="mono small truncate">{shortHash(row.nft)}</span>
            <span class="faint small">· {row.price} {row.currency} ·</span>
            <AgentBadge agent={row.buyer} size={18} />
          </div>
          <button class="primary tiny" disabled={busy === key} onclick={() => offer(row)}>
            {busy === key ? "…" : "Offer to them"}
          </button>
        </div>
      {/each}
    </div>
  </div>
{/if}

<style>
  .row-item {
    padding: 0.6rem;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-elev);
    gap: 0.5rem;
  }
</style>
