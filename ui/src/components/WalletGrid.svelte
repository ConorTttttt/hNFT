<script lang="ts">
  import { useAppState } from "$lib/appState.svelte";
  import { b64 } from "$lib/format";
  import NftCard from "./NftCard.svelte";

  const app = useAppState();
</script>

<div class="stack">
  <div class="row between">
    <h3>My NFTs {#if app.myNfts.length}<span class="faint">· {app.myNfts.length}</span>{/if}</h3>
    <button
      class="ghost tiny"
      onclick={() => app.loadMyNfts()}
      disabled={app.myNftsLoading}
    >
      {app.myNftsLoading ? "Refreshing…" : "Refresh"}
    </button>
  </div>

  {#if app.myNfts.length}
    <div class="grid">
      {#each app.myNfts as view (b64(view.nft))}
        <NftCard {view} onopen={(nft) => app.openDetail(nft)} />
      {/each}
    </div>
  {:else if app.myNftsLoading}
    <div class="grid">
      {#each Array(3) as _, i (i)}
        <div class="skeleton" style="aspect-ratio: 0.85"></div>
      {/each}
    </div>
  {:else}
    <div class="empty stack" style="gap: 0.4rem">
      <strong>Nothing here yet</strong>
      <span class="small">Mint an NFT, or wait for a transfer to land.</span>
    </div>
  {/if}
</div>
