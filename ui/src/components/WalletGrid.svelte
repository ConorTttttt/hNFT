<script lang="ts">
  import { useAppState } from "$lib/appState.svelte";
  import { b64 } from "$lib/format";
  import DraftShelf from "./DraftShelf.svelte";
  import NftCard from "./NftCard.svelte";

  let { onmint }: { onmint: () => void } = $props();

  const app = useAppState();

  const visible = $derived(app.myNfts.filter((v) => !v.hidden));
  const hidden = $derived(app.myNfts.filter((v) => v.hidden));
</script>

<div class="stack" style="gap: 1.25rem">
  <DraftShelf />

  <h3>
    My Collection {#if visible.length}<span class="faint">· {visible.length}</span>{/if}
  </h3>

  {#if visible.length}
    <div class="grid">
      {#each visible as view (b64(view.nft))}
        <NftCard
          {view}
          onopen={(nft) => app.openDetail(nft)}
          onhide={(nft) => app.hideNft(nft)}
        />
      {/each}
    </div>
  {:else if app.myNftsLoading}
    <div class="grid">
      {#each Array(3) as _, i (i)}
        <div class="skeleton" style="aspect-ratio: 0.8"></div>
      {/each}
    </div>
  {:else if !hidden.length}
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
  {:else}
    <div class="empty small">Every NFT you own is hidden.</div>
  {/if}

  {#if hidden.length}
    <details class="hidden-section">
      <summary>Hidden · {hidden.length}</summary>
      <div class="grid" style="margin-top: 0.75rem">
        {#each hidden as view (b64(view.nft))}
          <NftCard
            {view}
            onopen={(nft) => app.openDetail(nft)}
            onunhide={(nft) => app.unhideNft(nft)}
          />
        {/each}
      </div>
    </details>
  {/if}
</div>

<style>
  .hidden-section summary {
    cursor: pointer;
    color: var(--text-dim);
    font-size: 0.85rem;
  }
</style>
