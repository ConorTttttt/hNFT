<script lang="ts">
  import { useAppState } from "$lib/appState.svelte";
  import { b64 } from "$lib/format";
  import NftCard from "./NftCard.svelte";

  const app = useAppState();

  let forSaleOnly = $state(false);
  let listedHashes = $state<Set<string> | null>(null);

  $effect(() => {
    if (!forSaleOnly || listedHashes) return;
    app.nft
      .allListings()
      .then((views) => (listedHashes = new Set(views.map((v) => b64(v.nft)))))
      .catch(() => (listedHashes = new Set()));
  });

  const visible = $derived(
    app.discover.filter(
      (v) => !v.hidden && (!forSaleOnly || listedHashes?.has(b64(v.nft))),
    ),
  );
  const hidden = $derived(app.discover.filter((v) => v.hidden));
</script>

<div class="stack" style="gap: 1.25rem">
  <div class="row between wrap">
    <h3>
      Discover {#if visible.length}<span class="faint">· {visible.length}</span>{/if}
    </h3>
    <label class="row" style="gap: 0.4rem; align-items: center">
      <input type="checkbox" style="width: auto" bind:checked={forSaleOnly} />
      <span class="small">For sale only</span>
    </label>
    <span class="faint small">Every NFT minted in this collection</span>
  </div>

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
  {:else if app.discoverLoading}
    <div class="grid">
      {#each Array(3) as _, i (i)}
        <div class="skeleton" style="aspect-ratio: 0.8"></div>
      {/each}
    </div>
  {:else if !hidden.length}
    <div class="empty-hero">
      <strong>Nothing minted yet</strong>
      <span class="muted small">
        Once someone mints into this collection, it shows up here.
      </span>
    </div>
  {:else}
    <div class="empty small">Every NFT here is hidden.</div>
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
