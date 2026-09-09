<script lang="ts">
  import { useAppState } from "$lib/appState.svelte";
  import { b64 } from "$lib/format";
  import DraftShelf from "./DraftShelf.svelte";
  import NftCard from "./NftCard.svelte";

  let { onmint }: { onmint: () => void } = $props();

  const app = useAppState();
</script>

<div class="stack" style="gap: 1.25rem">
  <DraftShelf />

  <h3>
    My Collection {#if app.myNfts.length}<span class="faint">· {app.myNfts.length}</span>{/if}
  </h3>

  {#if app.myNfts.length}
    <div class="grid">
      {#each app.myNfts as view (b64(view.nft))}
        <NftCard {view} onopen={(nft) => app.openDetail(nft)} />
      {/each}
    </div>
  {:else if app.myNftsLoading}
    <div class="grid">
      {#each Array(3) as _, i (i)}
        <div class="skeleton" style="aspect-ratio: 0.8"></div>
      {/each}
    </div>
  {:else}
    <div class="empty-hero">
      <strong>Your gallery is empty</strong>
      <span class="muted small">
        Mint an NFT of your own, or wait for a transfer to arrive.
      </span>
      <div class="actions">
        <button class="primary" onclick={onmint}>Mint your first NFT</button>
        <button class="secondary" disabled title="needs a get_all_nfts coordinator fn">
          Explore peer network
        </button>
      </div>
    </div>
  {/if}
</div>
