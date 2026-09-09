<script lang="ts">
  import { getAppStateContext } from "$lib/context";
  import { b64 } from "$lib/format";
  import NftCard from "./NftCard.svelte";

  const app = getAppStateContext();
</script>

<div class="stack">
  <div class="row" style="justify-content: space-between">
    <h3 style="margin: 0">My NFTs</h3>
    <button class="ghost" onclick={() => app.loadMyNfts()} disabled={app.myNftsLoading}>
      {app.myNftsLoading ? "…" : "Refresh"}
    </button>
  </div>

  {#if app.myNfts.length === 0}
    <p class="muted">
      {app.myNftsLoading ? "Loading…" : "Nothing here yet."}
    </p>
  {:else}
    <div class="grid">
      {#each app.myNfts as view (b64(view.nft))}
        <NftCard {view} onopen={(nft) => app.openDetail(nft)} />
      {/each}
    </div>
  {/if}
</div>
